// The example project as a git repository: the ledger as it was, committed,
// then its change, "Calls to the bank that are safe to retry", as its last
// commit. The sandbox and the end-to-end tests both make it from these files.

import { spawnSync } from 'node:child_process'
import { cpSync, mkdirSync, readdirSync, readFileSync, rmSync } from 'node:fs'
import { dirname, join } from 'node:path'

const example = import.meta.dirname

/** The change's commit message, and the title and line its agent gives it. */
export const exampleChange = () => JSON.parse(readFileSync(join(example, 'change.json'), 'utf8')) as { commit: string; title: string; said: string }

/** git, in `cwd`, as the example's own author, at a fixed time, with no hooks. */
export function git(cwd: string, ...args: string[]) {
  const at = { GIT_AUTHOR_DATE: '2026-10-20T09:00:00Z', GIT_COMMITTER_DATE: '2026-10-20T09:00:00Z' }
  const who = { GIT_AUTHOR_NAME: 'Ledger', GIT_AUTHOR_EMAIL: 'ledger@example.com', GIT_COMMITTER_NAME: 'Ledger', GIT_COMMITTER_EMAIL: 'ledger@example.com' }
  const r = spawnSync('git', ['-c', 'core.hooksPath=/dev/null', '-c', 'commit.gpgsign=false', ...args], { cwd, env: { ...process.env, ...at, ...who } })
  if (r.status !== 0) throw new Error(`git ${args.join(' ')}: ${r.stderr}`)
  return r.stdout.toString().trim()
}

/** The files the example's change writes, from the ledger's root. */
export function changeFiles(): string[] {
  const out: string[] = []
  const walk = (dir: string, rel: string) => {
    for (const f of readdirSync(dir, { withFileTypes: true })) {
      if (f.isDirectory()) walk(join(dir, f.name), `${rel}${f.name}/`)
      else out.push(`${rel}${f.name}`)
    }
  }
  walk(join(example, 'change'), '')
  return out.sort()
}

/** Writes one file of the example's change into the ledger at `dir`, as its agent would. */
export function writeChange(dir: string, path: string) {
  mkdirSync(dirname(join(dir, path)), { recursive: true })
  cpSync(join(example, 'change', path), join(dir, path))
}

/** Makes the example at `dir`, afresh; with `change` false, only the ledger as it was, its change not yet made. */
export function makeLedger(dir: string, change = true) {
  rmSync(dir, { recursive: true, force: true })
  cpSync(join(example, 'base'), dir, { recursive: true })
  git(dir, 'init', '-q', '-b', 'main')
  git(dir, 'add', '-A')
  git(dir, 'commit', '-q', '-m', 'A ledger that never charges twice')
  if (!change) return
  cpSync(join(example, 'change'), dir, { recursive: true })
  git(dir, 'add', '-A')
  git(dir, 'commit', '-q', '-m', exampleChange().commit)
}
