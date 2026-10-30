// The tests' stand-in for the person's agent, on its own, for developing
// kelve without an agent or spending anything:
//
//   bun e2e/stand-in.ts
//
// It says hello to the kelve at KELVE_URL (http://localhost:4600 unless
// set), with the secret in KELVE_SECRET_FILE, then does kelve's work as
// it comes, the way the browser tests need: not well, but always, and at
// once. Start the stand-in graph beside it, to see entries written and
// checked.

import { readFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { agent } from './agent'

const base = process.env.KELVE_URL ?? 'http://localhost:4600'
const secretFile = process.env.KELVE_SECRET_FILE ?? join(process.env.XDG_CONFIG_HOME ?? join(homedir(), '.config'), 'kelve', 'secret')
const me = agent(base, readFileSync(secretFile, 'utf8').trim(), process.env.AGENT ?? 'claude-code')
await me.hello()
console.log(`The stand-in agent is doing kelve’s work at ${base}. Stop it with Ctrl C.`)
for (;;) {
  const done = await me.work()
  if (done.length) console.log(done.map((j) => j.about).join('\n'))
  await new Promise((wait) => setTimeout(wait, 1000))
}
