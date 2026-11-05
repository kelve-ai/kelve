// Debriefs for the end-to-end tests, until the stand-in agent writes them:
// the agent hands its work over, as its `open` tool does, and its debrief,
// research and explanations are kept in kelve's store as the agent's
// answers would keep them. The store is the tests' own, in their temporary
// folder (stack.ts); sqlite3 writes to it as kelve runs.

import { execFileSync } from 'node:child_process'
import { readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { me, repo, world } from './world'

/** The tests' kelve's store. */
function store(): string {
  const data = join(dirname(world().secretFile), 'data')
  const file = readdirSync(data).find((f) => /^kelve.*\.db$/.test(f))
  if (!file) throw new Error(`There's no store in ${data}.`)
  return join(data, file)
}

const quote = (text: string) => `'${text.replaceAll("'", "''")}'`

/** Runs SQL on the store: what it printed, a line a row. */
export function sql(statement: string): string {
  return execFileSync('sqlite3', ['-cmd', '.timeout 5000', store(), statement], { encoding: 'utf8' }).trim()
}

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

/** The agent hands over a small change: the handover's id. */
export async function handOver(title: string): Promise<{ handover: number; root: string }> {
  const agent = me()
  await agent.hello()
  const root = repo()
  // Its own words in the code, so what's explained of it is its own.
  const retry = join(root, 'src/retry.rs')
  writeFileSync(retry, readFileSync(retry, 'utf8').replace('/// Waits', `/// ${title}: waits`))
  await agent.open({ what: 'change', path: root, title, said: 'Each retry now waits twice as long as the last.' })
  const handover = Number(sql(`select id from handovers where root = ${quote(root)} order by id desc limit 1`))
  return { handover, root }
}

/** The agent hands over its work and writes the debrief: the debrief's id, and where the work is. */
export async function debriefed(title: string): Promise<{ id: number; root: string }> {
  const { handover, root } = await handOver(title)
  const at = Date.now()
  const id = Number(
    sql(`insert into debriefs (handover, root, session, state, title, size, markdown, tries, made_at, updated_at)
         values (${handover}, ${quote(root)}, '', 'ready', ${quote(title)}, 'small', ${quote(debriefOf(title))}, 1, ${at}, ${at})
         returning id`),
  )
  return { id, root }
}

/** The agent's answer to the research asked on a decision. */
export function researched(debrief: number, n: number, answer: string) {
  sql(`update research set state = 'ready', answer = ${quote(answer)}, answered_at = ${Date.now()}
        where decision = (select id from decisions where debrief = ${debrief} and number = ${n})`)
}

/** The agent's explanation of an idea, asked from a debrief. */
export function explained(debrief: number, idea: string, answer: string) {
  sql(`update explanations set state = 'ready', answer = ${quote(answer)}, answered_at = ${Date.now()}
        where idea = ${quote(idea)} and (debrief = ${debrief} or state != 'ready')`)
}

