// Debriefs for the end-to-end tests, as people get them: the agent hands its
// work over, as its `open` tool does, kelve asks it for the debrief at once,
// and the stand-in agent writes it, then the research and explanations the
// person asks for, through the same calls `kelve mcp` makes.

import { execFileSync } from 'node:child_process'
import { mkdtempSync, readFileSync, realpathSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { answer, type Job } from './agent'
import { me, repo, secret, world } from './world'

const caller = { agent: 'claude-code', model: 'stand-in', plan: 'pro' }

async function call<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${world().base}${path}`, {
    method: 'POST',
    headers: { authorization: `Bearer ${secret()}`, 'content-type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`kelve said no to ${path}: ${await res.text()}`)
  return (await res.json()) as T
}

/**
 * The stand-in agent does kelve's jobs for what it opened, by its number,
 * until it has handed in one of `kind`, answered with `with`: the rest it
 * answers as it always does.
 */
async function doUntil(opened: number, kind: string, with_: (job: Job) => unknown) {
  for (let i = 0; i < 200; i++) {
    const job = await call<Job | { preparing: true } | null>('/api/agent/next', { ...caller, for: opened })
    if (!job) throw new Error(`kelve had no ${kind} job for what was opened.`)
    if ('preparing' in job) {
      await new Promise((done) => setTimeout(done, 250))
      continue
    }
    const mine = job.kind === kind
    const handed: { taken: string; why?: string } = await call('/api/agent/answer', { ...caller, for: opened, job: job.job, answer: mine ? with_(job) : answer(job), more: false })
    if (handed.taken !== 'yes') throw new Error(`kelve didn't take the answer to ${job.about}: ${handed.why}`)
    if (mine) return
  }
  throw new Error(`kelve never gave a ${kind} job.`)
}

/** What each debrief's work was opened as, by the debrief's id. */
const openedAs = new Map<number, number>()

/** A debrief of the retry `repo()` adds, `src/retry.rs` lines 1 to 4. */
export function debriefOf(title: string): string {
  return `---
kelve: debrief/1
title: ${title}
---

## What it did

When a request fails, it now waits before trying again, twice as long each time.

## How it works

\`wait()\`, in \`src/retry.rs\`, says how long to wait before each try.

## Decisions

### Wait twice as long each time
- from: what you said
- lines: src/retry.rs 1-4
- rests on: Exponential backoff

Doubling gives a struggling server room to recover.

### Start from 200 milliseconds
- from: its own call
- lines: src/retry.rs 3
- could have: start from a second

Short enough that a blip is barely noticed.

## What it rests on

- **Exponential backoff:** waiting longer after each failure: 1, 2, 4, 8 seconds.
- **Jitter:** a little randomness added to a wait.
`
}

/** The agent hands over a small change, a typo fixed in a README: no debrief is due for it. */
export async function handOver(title: string): Promise<{ root: string }> {
  const agent = me()
  await agent.hello()
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'kelve-e2e-typo-')))
  const git = (...args: string[]) =>
    execFileSync('git', ['-c', 'user.name=Test', '-c', 'user.email=test@example.com', '-c', 'commit.gpgsign=false', ...args], { cwd: root })
  writeFileSync(join(root, 'README.md'), '# Ledger\n\nA ledgr that never charges twice.\n')
  git('init', '-q', '-b', 'main')
  git('add', '.')
  git('commit', '-q', '-m', 'README')
  writeFileSync(join(root, 'README.md'), '# Ledger\n\nA ledger that never charges twice.\n')
  const opened = await agent.open({ what: 'change', path: root, title, said: 'Fixed a typo.' })
  if (opened.debrief?.due) throw new Error(`A typo was thought worth a debrief: ${opened.debrief.reason}`)
  return { root }
}

/** The agent hands over its work and writes the debrief kelve asks for at once: the debrief's id, and where the work is. */
export async function debriefed(title: string): Promise<{ id: number; root: string }> {
  const agent = me()
  await agent.hello()
  const root = repo()
  // Its own words in the code, so what's explained of it is its own.
  const retry = join(root, 'src/retry.rs')
  writeFileSync(retry, readFileSync(retry, 'utf8').replace('/// Waits', `/// ${title}: waits`))
  const opened = await agent.open({ what: 'change', path: root, title, said: 'Each retry now waits twice as long as the last.' })
  const id = opened.debrief?.debrief
  if (!opened.debrief?.due || !id) throw new Error(`No debrief was asked for: ${opened.debrief?.reason}`)
  await doUntil(opened.id, 'debrief', () => ({ debrief: debriefOf(title) }))
  openedAs.set(id, opened.id)
  return { id, root }
}

/** The agent's answer to the research asked on a decision of a debrief. */
export async function researched(debrief: number, _n: number, research: string) {
  await doUntil(openedAs.get(debrief) ?? 0, 'research', () => ({ research }))
}

/** The agent's explanation of an idea, asked from a debrief. */
export async function explained(debrief: number, _idea: string, explanation: string) {
  await doUntil(openedAs.get(debrief) ?? 0, 'explain', () => ({ explanation }))
}
