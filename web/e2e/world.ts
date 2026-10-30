// The world the end-to-end tests run in, as the stack seeded it; signing a
// browser in the way `kelve open` does; and the person's agent, standing
// in, opening things in kelve and doing its work.

import { execFileSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { expect, type Page } from '@playwright/test'
import { agent, type Agent } from './agent'
import { makeLedger } from '../../dev/example/make'

export interface World {
  base: string
  /** The secret kelve shares with its command line, and `kelve mcp`. */
  secretFile: string
  kelve: string
  /** What the stand-in for the system's picker chooses next. */
  pickFile: string
  /** Where kelve writes what it says, as its person sees it where `kelve serve` runs. */
  logFile: string
  /** The stand-in shared graph. */
  graph: string
  /** The tests' own Claude Code folder, where the notes hook goes. */
  claudeDir: string
}

export const WORLD_FILE = new URL('./.world.json', import.meta.url)

export function world(): World {
  return JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World
}

/** The path the system's picker, standing in, chooses next: "" closes it. */
export function picks(path: string) {
  writeFileSync(world().pickFile, path)
}

export function secret(w: Pick<World, 'secretFile'> = world()): string {
  return readFileSync(w.secretFile, 'utf8').trim()
}

/** The person's agent, standing in, for the tests' kelve, or another. */
export function me(w: Pick<World, 'base' | 'secretFile'> = world()): Agent {
  return agent(w.base, secret(w))
}

/** Every sign-in link kelve has shown where it runs, oldest first. */
export function shown(logFile: string): string[] {
  try {
    return [...readFileSync(new URL(`../${logFile}`, import.meta.url), 'utf8').matchAll(/https?:\/\/\S+\/signin\?token=[\w-]+(?:&opened=\d+)?/g)].map((m) => m[0])
  } catch {
    return []
  }
}

/** Waits for kelve to show a sign-in link where it runs, newer than `before` of them. */
export async function newLink(logFile: string, before: number): Promise<string> {
  const until = Date.now() + 5_000
  while (Date.now() < until) {
    const links = shown(logFile)
    if (links.length > before) return links[links.length - 1]
    await new Promise((done) => setTimeout(done, 50))
  }
  throw new Error(`kelve didn’t show a sign-in link where it runs: see ${logFile}`)
}

/**
 * A one-time link that signs a browser in, as `kelve open` asks for one.
 * kelve never hands it back: it opens a browser, or, as here, shows it
 * where `kelve serve` runs, which is where the tests read it. They run one
 * at a time, so the newest link is theirs.
 */
export async function link(w: Pick<World, 'base' | 'secretFile' | 'logFile'> = world()): Promise<string> {
  const before = shown(w.logFile).length
  const res = await fetch(`${w.base}/api/signin-link`, { method: 'POST', headers: { authorization: `Bearer ${secret(w)}` } })
  if (!res.ok) throw new Error(`kelve wouldn't do it: ${await res.text()}`)
  const said = (await res.json()) as Record<string, unknown>
  if ('url' in said) throw new Error('kelve told its command line more than whether a browser opened')
  return newLink(w.logFile, before)
}

/** Signs the page's browser in. */
export async function signIn(page: Page, w?: Pick<World, 'base' | 'secretFile' | 'logFile'>, opts: { new?: boolean } = {}) {
  await page.goto(await link(w))
  if (!opts.new) await started(page, (w ?? world()).base)
}

/** The person has been through the first run, so what's opened goes straight to the reader. */
async function started(page: Page, base: string) {
  await page.request.post(`${base}/api/settings`, { data: { hintSeen: 'started' } })
}

/**
 * The agent opens something, as its `open` tool does, and does kelve's
 * work on it, or, with `work: false`, only finds its way in. Each opening
 * is a new tab: with no browser to open it in, kelve shows its link where
 * it runs, and the browser follows it to what was opened.
 */
export async function open(page: Page, what: Record<string, unknown>, opts: { work?: boolean } = {}) {
  const w = world()
  const agent = me(w)
  await agent.hello()
  const before = shown(w.logFile).length
  const opened = await agent.open(what)
  if (!opened.shown) await page.goto(await newLink(w.logFile, before))
  else await page.waitForURL(/#\/o\/\d+/)
  await started(page, w.base)
  if (opts.work !== false) await agent.work()
  // Not working yet: the way in is still found first, as it always is.
  else if (what.what !== 'file') await agent.findWay()
  return opened
}

/** An idea in the margin, beside its first line, by its name. */
export const marginNote = (page: Page, name: string) => page.locator('.mnote').filter({ has: page.getByText(name, { exact: true }) }).first()

/** The entry open on top, by its idea's name. */
export const pane = (page: Page, name: string) => page.getByRole('article', { name, exact: true })

/** Shows the Files tab: In order comes first once there's a way in. */
export async function toFiles(page: Page) {
  const tab = page.getByRole('complementary', { name: 'What’s open' }).getByRole('tab', { name: 'Files' })
  if ((await tab.getAttribute('aria-selected')) !== 'true') await tab.click()
}

/** Opens a file from the side panel's tree, opening the folders on its way. */
export async function openFile(page: Page, path: string) {
  const side = page.getByRole('complementary', { name: 'What’s open' })
  await toFiles(page)
  const name = path.split('/').pop()!.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  await side.getByRole('treeitem', { name: new RegExp(`^${name}\\b`) }).first().click()
  await expect(page.locator('.fv-name')).toContainText(path)
}

const LEDGER = `use sqlx::PgPool;

/// Holds back the most a call could cost, before it's made.
pub async fn reserve(tx: &mut Transaction<'_>, key: i64) -> Result<i64> {
    sqlx::query("select pg_advisory_xact_lock($1)").bind(key).execute(&mut **tx).await?;
    Ok(total(tx).await?)
}
`

const RETRY = `/// Waits before trying again: with backoff, twice as long each time.
pub fn wait(attempt: u32) -> u64 {
    200 * 2u64.pow(attempt)
}
`

const git = (root: string, ...args: string[]) =>
  execFileSync('git', ['-c', 'user.name=Test', '-c', 'user.email=test@example.com', '-c', 'commit.gpgsign=false', ...args], { cwd: root })

/** A small repo with a ledger committed, and a retry added since, not committed: with `idea`, a file naming it too. */
export function repo(idea?: string): string {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'kelve-e2e-repo-')))
  mkdirSync(join(root, 'src'))
  writeFileSync(join(root, 'src/ledger.rs'), LEDGER)
  git(root, 'init', '-q', '-b', 'main')
  git(root, 'add', '.')
  git(root, 'commit', '-q', '-m', 'ledger')
  writeFileSync(join(root, 'src/retry.rs'), RETRY)
  // A file naming an idea no other test has, so the shared graph has no entry for it.
  if (idea) writeFileSync(join(root, 'src/keys.rs'), `pub fn key() -> String {\n    // idea: ${idea}\n    String::new()\n}\n`)
  git(root, 'add', '.')
  return root
}

/** The example project, made afresh in a folder of its own: the ledger, and its change as its last commit. */
export function ledger(): string {
  const root = join(realpathSync(mkdtempSync(join(tmpdir(), 'kelve-e2e-ledger-'))), 'ledger')
  makeLedger(root)
  return root
}
