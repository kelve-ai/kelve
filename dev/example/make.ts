// The example project as a git repository: the ledger as it was, committed,
// then its change, "Retries that back off with jitter", as its last commit. The
// sandbox and the end-to-end tests both make it from these files.

import { spawnSync } from 'node:child_process'
import { cpSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'

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

/** Makes the example at `dir`, afresh. */
export function makeLedger(dir: string) {
  rmSync(dir, { recursive: true, force: true })
  cpSync(join(example, 'base'), dir, { recursive: true })
  git(dir, 'init', '-q', '-b', 'main')
  git(dir, 'add', '-A')
  git(dir, 'commit', '-q', '-m', 'A ledger that never charges twice')
  cpSync(join(example, 'change'), dir, { recursive: true })
  git(dir, 'add', '-A')
  git(dir, 'commit', '-q', '-m', exampleChange().commit)
}
