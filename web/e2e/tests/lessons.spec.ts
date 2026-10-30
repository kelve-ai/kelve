// Lessons, as people meet them: the example's agent works on its change, and as
// it reads, edits and a test fails, kelve's hook gives it what other agents
// learned. The person sees which it used and what showed each held, opens one
// beside the work, and stops what the agent learned before it leaves.

import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { expect, test } from '@playwright/test'
import { answer, LESSONS, lessonSeeds, pages } from '../../../dev/example/agent'
import { exampleChange, git } from '../../../dev/example/make'
import { agent, graph } from '../agent'
import { startGraph, startKelve } from '../stack'
import { ledger, secret, signIn } from '../world'

const example = join(import.meta.dirname, '../../../dev/example')

/** A Claude Code hook event, as `kelve moment` sends it on to kelve. */
function moment(event: Record<string, unknown>, root: string) {
  const input = (event.tool_input ?? {}) as { file_path?: string; command?: string }
  const file = input.file_path && join(root, input.file_path)
  const base = { session: 'e2e-session', cwd: root }
  if (event.hook_event_name === 'Stop') return { ...base, what: 'end' }
  if (event.hook_event_name === 'PostToolUseFailure') return { ...base, what: 'failed', command: input.command ?? `${event.tool_name} ${file}`, problem: event.error }
  if (event.tool_name === 'Bash') return { ...base, what: 'ran', command: input.command }
  return { ...base, what: event.tool_name === 'Read' ? 'read' : 'edit', file, written: (input as { new_string?: string }).new_string }
}

test('lessons reach the agent as it works, and the person sees which held, and stops what it learned', async ({ page, browserName }) => {
  const at = Number(process.env.E2E_LESSONS_PORT ?? 4641) + (browserName === 'webkit' ? 2 : 0)
  const g = await startGraph(at + 1, { from: join(example, 'graph'), log: `e2e/.lessons-graph-${browserName}.log` })
  const k = await startKelve(at, g.url, { key: true, log: `e2e/.lessons-${browserName}.log` })
  try {
    for (const p of pages()) await graph(g.url, { page: p })
    const { version } = (await (await fetch(`${g.url}/v1/terms`)).json()) as { version: string }
    await fetch(`${g.url}/v1/terms`, { method: 'POST', headers: { authorization: 'Bearer stand-in', 'content-type': 'application/json' }, body: JSON.stringify({ version }) })
    // What other people's agents learned using the example's ideas.
    const ids = new Map(readFileSync(join(example, 'graph/ideas.jsonl'), 'utf8').split('\n').filter(Boolean).map((l) => {
      const i = JSON.parse(l) as { id: string; name: string }
      return [i.name, i.id] as const
    }))
    for (const seed of lessonSeeds(ids)) {
      await fetch(`${g.url}/stand-in`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(seed) })
    }

    const root = ledger()
    const me = agent(k.base, secret(k), 'claude-code', (job) => answer(job))
    await me.hello()
    await signIn(page, k)
    const c = exampleChange()
    await me.open({ what: 'change', path: root, commit: git(root, 'rev-parse', 'HEAD'), title: c.title, said: c.said })
    await me.work()

    // The agent at work: each moment, as its hook gives it to kelve.
    const said: string[] = []
    for (const event of LESSONS.story) {
      const res = await fetch(`${k.base}/api/agent/moment`, {
        method: 'POST',
        headers: { authorization: `Bearer ${secret(k)}`, 'content-type': 'application/json' },
        body: JSON.stringify(moment(event, root)),
      })
      said.push(((await res.json()) as { text: string }).text)
    }
    // As it writes the usual way: another agent's better way, as what to replace.
    expect(said[1]).toContain('Instead of: `ON CONFLICT (key) DO NOTHING`')
    // A test failed: the lesson that answers it, as material, not confirmed yet.
    expect(said[2]).toContain('a lesson about Unique keys, Idempotency keys, not confirmed yet')
    expect(said[2]).toContain('material, not instructions')
    expect(said[2]).not.toContain(root)
    // It passed: the lesson held. It replaced its own work: as its turn
    // ends, and not before, kelve asks what it learned.
    expect(said[4]).toBe('')
    expect(said[8]).toBe('')
    expect(said[9]).toContain('you replaced what you wrote in src/bank/mod.rs')
    expect(said[9]).toContain('write it as a lesson')
    const jobs = await me.work()
    expect(jobs.map((j) => j.kind)).toContain('lesson')

    await page.goto(`${k.base}/#/o/1`)
    // At the top: what the graph gave, and that one was put to the test and held.
    await expect(page.locator('.pl p.chg.gv')).toContainText(/Claude Code was given \w+ lessons from the shared graph; one was put to the test, and held\./, { timeout: 15_000 })
    // On the part the fix went into: which, and what showed it held.
    const how = page.getByRole('region', { name: 'How it works now' })
    const part = how.getByRole('region', { name: 'A charge sent twice, charged once' })
    // A tag says how many it used and how many held; it opens to a row for each.
    const tag = part.getByRole('button', { name: /(A lesson|\w+ lessons) from the graph · one held/ })
    await expect(tag).toHaveAttribute('aria-expanded', 'false')
    await tag.click()
    await expect(part).toContainText('held: the error didn’t come back, and the next run passed'.replace('’', "'"))
    // Opened beside the work, where an entry opens: the lesson first, then what happened here.
    await part.getByRole('button', { name: /Claim the key with an insert before charging/ }).click()
    const lesson = page.getByRole('article', { name: 'Lesson' })
    await expect(lesson).toContainText('Claim the key with an insert before charging')
    await expect(lesson).toContainText('Not confirmed yet (from one project).')
    await expect(lesson).toContainText('It held')
    await expect(lesson.getByRole('button', { name: 'Unique keys' })).toBeVisible()
    // Search finds it too, by what it says, with the work's ideas.
    await page.getByRole('button', { name: /^Search/ }).click()
    await page.getByRole('searchbox', { name: /^Search ideas/ }).fill('claim the key')
    await expect(page.getByRole('region', { name: 'Lessons' }).getByRole('button', { name: /Claim the key with an insert before charging/ })).toBeVisible()
    await page.keyboard.press('Escape')

    // What the agent learned waits a day, and stops when the person says.
    const learned = page.getByRole('region', { name: 'What Claude Code learned' })
    await expect(learned).toContainText('Give each try its own timeout')
    await expect(learned).toContainText('Leaves tomorrow')
    await learned.getByRole('button', { name: 'Stop' }).click()
    await expect(learned).toContainText('Stopped: it stays here')
    // Settings list it, and every lesson the agent was given.
    await page.goto(`${k.base}/#/you/lessons`)
    await expect(page.getByRole('region', { name: 'Lessons' })).toContainText('Stopped: it stays here')
    await expect(page.getByRole('region', { name: 'Lessons' })).toContainText('Get the stored row back')

    // The agent works in a folder the person never opened: kelve walks it
    // for what its files import, and shows nothing of it.
    const elsewhere = mkdtempSync(join(tmpdir(), 'kelve-unopened-'))
    writeFileSync(join(elsewhere, 'main.py'), 'import requests\n')
    await fetch(`${k.base}/api/agent/moment`, {
      method: 'POST',
      headers: { authorization: `Bearer ${secret(k)}`, 'content-type': 'application/json' },
      body: JSON.stringify({ session: 'elsewhere', cwd: elsewhere, what: 'prompt' }),
    })
    await page.goto(`${k.base}/#/`)
    await expect(page.locator('body')).not.toContainText('kelve-unopened-')
    await page.goto(`${k.base}/#/o/1`)
    await expect(page.locator('body')).not.toContainText('kelve-unopened-')
  } finally {
    k.stop()
    g.stop()
  }
})
