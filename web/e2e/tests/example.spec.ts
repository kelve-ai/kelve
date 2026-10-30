// The example, read as people read it: the ledger and its change, "Retries that
// back off with jitter", opened by the example's own agent, with its real
// marks, relations and entries, beside a shared graph holding the example's.
// What's new in the change, an idea in the margin with how the work uses it, an
// entry the agent writes there and then, and the way down to a first principle.

import { join } from 'node:path'
import { expect, test } from '@playwright/test'
import { answer, pages } from '../../../dev/example/agent'
import { exampleChange, git } from '../../../dev/example/make'
import { agent, graph } from '../agent'
import { startGraph, startKelve } from '../stack'
import { ledger, marginNote, openFile, pane, secret, signIn, toFiles } from '../world'

test('the example change, read down to a first principle', async ({ page, browserName }) => {
  const at = Number(process.env.E2E_EXAMPLE_PORT ?? 4631) + (browserName === 'webkit' ? 2 : 0)
  const g = await startGraph(at + 1, { from: join(import.meta.dirname, '../../../dev/example/graph'), log: `e2e/.example-graph-${browserName}.log` })
  const k = await startKelve(at, g.url, { key: true, log: `e2e/.example-${browserName}.log` })
  try {
    // The pages the example's entries quote, and the terms, agreed long ago.
    for (const p of pages()) await graph(g.url, { page: p })
    const { version } = (await (await fetch(`${g.url}/v1/terms`)).json()) as { version: string }
    await fetch(`${g.url}/v1/terms`, { method: 'POST', headers: { authorization: 'Bearer stand-in', 'content-type': 'application/json' }, body: JSON.stringify({ version }) })

    const root = ledger()
    const me = agent(k.base, secret(k), 'claude-code', (job) => answer(job))
    await me.hello()
    await signIn(page, k)
    const c = exampleChange()
    await me.open({ what: 'change', path: root, commit: git(root, 'rev-parse', 'HEAD'), title: c.title, said: c.said })
    await page.goto(`${k.base}/#/o/1`)
    await me.work()

    await expect(page.getByRole('heading', { level: 1, name: c.title })).toBeVisible()
    // It opens on the change's own page, plainly: what it does, by purpose,
    // each idea woven in, a click from its entry.
    await me.work()
    const how = page.getByRole('region', { name: 'How it works now' })
    await expect(how).toContainText('Waits longer, at a random point', { timeout: 15_000 })
    await expect(page.locator('.pl p.chg')).toContainText('Fixes')
    await expect(how.getByRole('button', { name: /Idempotency keys/ })).toBeVisible()
    await expect(page.getByRole('region', { name: 'Its files, in the order to read them' })).toContainText('src/retry.rs')
    // Every idea is in search before you type, what's new red, what was already there too.
    const side = page.getByRole('complementary', { name: 'What’s open' })
    await page.getByRole('button', { name: /^Search/ }).click()
    const ideas = page.getByRole('region', { name: 'Ideas here' })
    for (const idea of ['Exponential backoff', 'Idempotency keys', 'Unique keys', 'Timeouts', 'Jitter']) {
      await expect(ideas.getByRole('img', { name: `${idea}, new in this change, not read yet` })).toBeVisible()
    }
    await expect(ideas.getByText('Authorisation holds')).toBeVisible()
    await page.keyboard.press('Escape')
    await side.getByRole('tab', { name: 'Files' }).click()

    // Each folder says what it's for, as the agent said while it read.
    await expect(side.getByText('Talking to the bank, and nothing else.')).toBeVisible()
    // A folder's page, read plainly as part of reading the change: how it
    // works, by its parts, each opening plainly.
    await toFiles(page)
    await side.getByRole('button', { name: 'src', exact: true }).click()
    await expect(page.getByRole('heading', { level: 2, name: 'src' })).toBeVisible()
    await expect(page.getByRole('region', { name: 'How it works' })).toContainText('The bank is asked')

    // The margin says how this work uses each idea, as the agent put it.
    await openFile(page, 'src/retry.rs')
    const jitter = marginNote(page, 'Jitter')
    await expect(jitter).toContainText('A random share of that, so clients don’t retry together.'.replace('’', "'"))

    // An entry the graph has, in two layers: its folds closed, opened in place.
    await marginNote(page, 'Exponential backoff').click()
    const backoff = pane(page, 'Exponential backoff')
    // Under its first line, where it came from (Value-2).
    await expect(backoff.locator('.ep-from')).toHaveText(/^From the shared graph · checked against \d+ sources? · read (once|[\d,]+ times)$/)
    const worked = 'Say the base wait is 1 second and the ceiling is 30.'
    await expect(backoff.getByText('A worked example', { exact: true })).toBeVisible()
    await expect(backoff.getByText(worked, { exact: false })).toBeHidden()
    await backoff.getByText('A worked example', { exact: true }).click()
    await expect(backoff.getByText(worked, { exact: false })).toBeVisible()
    // Didn't help: what would, sent with nothing but the choice.
    await backoff.getByRole('button', { name: 'Didn’t help me understand' }).click()
    await backoff.getByLabel('Needs an example').check()
    const asked = page.waitForRequest((r) => r.url().endsWith('/ask') && r.method() === 'POST')
    await backoff.getByRole('button', { name: 'Send it' }).click()
    expect(JSON.parse((await asked).postData() ?? '{}')).toEqual({ kind: 'example' })
    await expect(backoff).toContainText('Sent. The next agent that adds to the shared graph')

    // Jitter's entry isn't in the shared graph: the agent writes it, there and then.
    await jitter.click()
    const entry = pane(page, 'Jitter')
    await expect(entry.locator('.wline')).toContainText('Claude Code')
    await me.work()
    await expect(entry).toContainText('If every client waits the same time before retrying', { timeout: 15_000 })
    await expect(entry.getByRole('region', { name: 'How it relates' })).toContainText('Randomness', { timeout: 20_000 })

    // Down to a first principle: the ground everything above it stands on.
    await entry.getByRole('button', { name: /Randomness/ }).click()
    const ground = pane(page, 'Randomness')
    await expect(ground.getByRole('heading', { level: 2, name: 'Randomness' })).toBeVisible()
    await expect(page.getByRole('navigation', { name: 'Where you are' })).toContainText('Jitter')
    await expect(ground).toContainText('Nothing: this is a first principle, the ground everything above it stands on.')
    // A first principle's square is black, on a line, wherever it shows.
    await expect(entry.getByRole('img', { name: 'Randomness, a first principle' })).toBeVisible()
  } finally {
    k.stop()
    g.stop()
  }
})
