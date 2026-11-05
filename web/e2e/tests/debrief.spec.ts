// Debriefs: as the agent hands its work over, kelve decides from the change
// whether it's worth a debrief, and asks for it at once, by the number of
// what was opened, so the agent writes it while it remembers why. A debrief
// whose lines aren't in the change goes back once, with what to fix; one
// that passes is kept, and more on the same work brings it up to date.

import { expect, test } from '@playwright/test'
import type { Job } from '../agent'
import { ledger, me, secret, world } from '../world'
import { debrief } from '../../../dev/example/loop'

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

test('work handed over is debriefed at once, a wrong debrief goes back, and the one kept is brought up to date', async () => {
  const root = ledger()
  const agent = me()
  await agent.hello()
  const handover = { what: 'change', path: root, commit: 'HEAD', title: 'Calls to the bank that are safe to retry', said: 'Retries back off with jitter; I chose a breaker over retrying every call.' }
  const opened = await agent.open(handover)
  expect(opened.debrief?.due).toBe(true)
  expect(opened.debrief?.again).toBe(false)

  // The agent takes it at once, by the number of what it opened.
  const job = await call<Job>('/api/agent/next', { ...caller, for: opened.id })
  expect(job.kind).toBe('debrief')
  expect(String(job.material.change)).toContain('src/retry.rs')
  expect(job.material.changed).toBeUndefined()

  // Lines that aren't in the change come back once, with what to fix.
  const wrong = debrief(job).debrief.replace(/- lines: src\/retry\.rs [\d-]+/, '- lines: src/retry.rs 900-910')
  const back: { taken: string; why?: string; put_aside?: boolean } = await call('/api/agent/answer', { ...caller, for: opened.id, job: job.job, answer: { debrief: wrong }, more: false })
  expect(back.taken).toBe('no')
  expect(back.put_aside).toBe(false)
  expect(back.why).toContain('Fix the debrief')

  // Fixed, it's kept.
  const taken: { taken: string } = await call('/api/agent/answer', { ...caller, for: opened.id, job: job.job, answer: debrief(job), more: false })
  expect(taken.taken).toBe('yes')

  // Handed over again before it's sent: the same debrief, brought up to date
  // from the one kept.
  const again = await agent.open(handover)
  expect(again.debrief?.debrief).toBe(opened.debrief?.debrief)
  expect(again.debrief?.again).toBe(true)
  const update = await call<Job>('/api/agent/next', { ...caller, for: again.id })
  expect(update.kind).toBe('debrief')
  expect(String(update.material.debrief)).toContain('## Decisions')
  expect(String(update.material.debrief)).toContain('Wait a random time below a ceiling that doubles')
})
