// Settings (boards O-6 to O-9, N-6): your agent, the shared graph, reading,
// notes and what's shared, each a page.

import { mkdtempSync, readFileSync, realpathSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { expect, test } from '@playwright/test'
import { agent as agentAt, graph } from '../agent'
import { startGraph, startKelve } from '../stack'
import { marginNote, me, openFile, picks, repo, secret, signIn, world } from '../world'

test('a browser that isn’t signed in is told how to open kelve', async ({ page }) => {
  await page.goto('/')
  await expect(page.getByText('kelve open')).toBeVisible()
})

test('your agent: which is connected, the line to connect it, and the notes hook', async ({ page }) => {
  await me().hello()
  await signIn(page)
  await page.goto('/#/you/agent')
  const agent = page.getByRole('region', { name: 'Your agent', exact: true })
  // One agent, or, once another has connected too, each.
  await expect(agent).toContainText(/Claude Code( is connected)?, on stand-in\./)
  await agent.getByRole('radio', { name: 'Gemini CLI' }).click()
  await expect(agent.getByText('gemini mcp add --scope user kelve kelve mcp')).toBeVisible()
  await agent.getByRole('radio', { name: 'Claude Code' }).click()
  // On or off, as plain words, like any other choice.
  const hook = agent.getByRole('radiogroup', { name: 'Let Claude Code pick up your notes, and lessons' })
  await hook.getByRole('radio', { name: 'On' }).click()
  await expect(hook.getByRole('radio', { name: 'On' })).toHaveAttribute('aria-checked', 'true')
  expect(readFileSync(join(world().claudeDir, 'settings.json'), 'utf8')).toContain('kelve moment')
  // Another agent chosen: its own hook, and that Claude Code has kelve's too.
  await agent.getByRole('radio', { name: 'Codex' }).click()
  await expect(agent).toContainText('kelve’s hook is in Claude Code’s settings too.')
  // Another agent connects: each is listed, the latest first.
  await agentAt(world().base, secret(world()), 'codex-mcp-client').hello()
  await page.reload()
  const lately = agent.getByRole('list', { name: 'Agents connected lately' })
  await expect(lately.getByRole('listitem')).toHaveText([/^Codex, on stand-in\. It last asked kelve/, /^Claude Code, on stand-in\. It last asked kelve/])
  await me().hello()
  await agent.getByRole('radio', { name: 'Claude Code' }).click()
  await hook.getByRole('radio', { name: 'Off' }).click()
  await expect(hook.getByRole('radio', { name: 'Off' })).toHaveAttribute('aria-checked', 'true')
  await expect(page.getByText(/model key|spending|ceiling/i)).toHaveCount(0)
})

test('reading: sizes and highlighting change how code is drawn', async ({ page }) => {
  await signIn(page)
  await page.goto('/#/you/reading')
  const reading = page.getByRole('region', { name: 'Reading' })
  await reading.getByRole('radiogroup', { name: 'Code size' }).getByRole('radio', { name: 'Large' }).click()
  await expect(page.locator('html')).toHaveAttribute('data-code', 'large')
  await reading.getByRole('radiogroup', { name: 'Highlighting' }).getByRole('radio', { name: 'None' }).click()
  await expect(page.locator('html')).toHaveAttribute('data-highlighting', 'none')
  await reading.getByRole('radiogroup', { name: 'Code size' }).getByRole('radio', { name: 'Medium' }).click()
  await reading.getByRole('radiogroup', { name: 'Highlighting' }).getByRole('radio', { name: 'Full' }).click()
  await expect(page.locator('html')).toHaveAttribute('data-highlighting', 'full')
})

test('what kelve marks: set in settings, kept, and written by your agent', async ({ page }) => {
  await signIn(page)
  await page.goto('/#/you/marks')
  const marks = page.getByRole('region', { name: 'What kelve marks' })
  await expect(marks).toContainText('marks.md')
  // Changed here, each choice kept at once.
  await marks.getByRole('radiogroup', { name: 'Mark as if I’m' }).getByRole('radio', { name: 'Working' }).click()
  await marks.getByRole('textbox', { name: 'Add an area' }).fill('Rust')
  await marks.getByRole('textbox', { name: 'Add an area' }).press('Enter')
  await marks.getByRole('radiogroup', { name: 'rust', exact: true }).getByRole('radio', { name: 'New' }).click()
  // How files read: overall, and an area's own, by its language.
  await marks.getByRole('radiogroup', { name: 'Read rust files' }).getByRole('radio', { name: 'Plainly' }).click()
  await expect(marks.getByRole('radiogroup', { name: 'Read files' }).getByRole('radio', { name: 'As code' })).toHaveAttribute('aria-checked', 'true')
  await page.reload()
  await expect(marks.getByRole('radiogroup', { name: 'Mark as if I’m' }).getByRole('radio', { name: 'Working' })).toHaveAttribute('aria-checked', 'true')
  await expect(marks.getByRole('radiogroup', { name: 'rust', exact: true }).getByRole('radio', { name: 'New' })).toHaveAttribute('aria-checked', 'true')
  await expect(marks.getByRole('radiogroup', { name: 'Read rust files' }).getByRole('radio', { name: 'Plainly' })).toHaveAttribute('aria-checked', 'true')
  await expect(marks).toContainText('Written by you')
  // Written by the agent, as set_marks does: the page shows it, and who wrote it.
  await me().setMarks({ mark: 'learning', areas: [], always: [], never: [], words: '' })
  await expect(marks).toContainText('Written by Claude Code')
  await expect(marks.getByRole('radiogroup', { name: 'Mark as if I’m' }).getByRole('radio', { name: 'Learning' })).toHaveAttribute('aria-checked', 'true')
  await expect(marks.getByRole('radiogroup', { name: 'rust', exact: true })).toHaveCount(0)
  // Saying nothing of how files read, they read plainly; as code again, as the tests' person reads them.
  const read = marks.getByRole('radiogroup', { name: 'Read files' })
  await expect(read.getByRole('radio', { name: 'Plainly' })).toHaveAttribute('aria-checked', 'true')
  await read.getByRole('radio', { name: 'As code' }).click()
  await expect(read.getByRole('radio', { name: 'As code' })).toHaveAttribute('aria-checked', 'true')
})

test('your own notes go to a folder you choose in the picker, and back to kelve’s', async ({ page }) => {
  await signIn(page)
  await page.goto('/#/you/notes')
  const notes = page.getByRole('region', { name: 'Notes' })
  const folder = realpathSync(mkdtempSync(join(tmpdir(), 'kelve-e2e-notes-')))
  picks(folder)
  await notes.getByRole('button', { name: 'Choose a folder' }).click()
  await expect(notes).toContainText(folder.split('/').pop()!)
  await page.reload()
  await expect(notes).toContainText(folder.split('/').pop()!)
  await notes.getByRole('button', { name: 'Use kelve’s own folder' }).click()
  await expect(notes).toContainText('kelve’s own folder')
})

test('what’s shared says who gets what, and anonymous counts are shared only when you say', async ({ page }) => {
  await signIn(page)
  await page.goto('/#/you/shared')
  const shared = page.getByRole('region', { name: 'What’s shared' })
  await expect(shared).toContainText('Never your code, file names, paths or notes')
  const count = shared.getByRole('radiogroup', { name: 'Share anonymous counts' })
  const on = count.getByRole('radio', { name: 'On' })
  const off = count.getByRole('radio', { name: 'Off' })
  // Every browser shares one kelve: start from it off, and leave it so.
  await off.click()
  await shared.getByText('See exactly what this week’s says so far').click()
  await expect(shared.locator('pre').first()).toContainText('"week"')
  await on.click()
  await expect(on).toHaveAttribute('aria-checked', 'true')
  await page.reload()
  await expect(on).toHaveAttribute('aria-checked', 'true')
  await off.click()
  await expect(off).toHaveAttribute('aria-checked', 'true')
})

test('once today’s limit on adding is reached, the rest waits, and settings say when it goes', async ({ page, browserName }) => {
  // A graph and a kelve of their own: the limit holds every send till midnight UTC.
  const at = Number(process.env.E2E_LIMIT_PORT ?? 4641) + (browserName === 'webkit' ? 2 : 0)
  const g = await startGraph(at + 1, { log: `e2e/.limit-graph-${browserName}.log` })
  const fresh = await startKelve(at, g.url, { key: true, log: `e2e/.limit-${browserName}.log` })
  try {
    const { version } = (await (await fetch(`${g.url}/v1/terms`)).json()) as { version: string }
    await fetch(`${g.url}/v1/terms`, { method: 'POST', headers: { authorization: 'Bearer stand-in', 'content-type': 'application/json' }, body: JSON.stringify({ version }) })
    await graph(g.url, { enough: 'ideas' })
    const agentHere = agentAt(fresh.base, secret(fresh))
    await agentHere.hello()
    await signIn(page, fresh)
    const idea = `Request keys ${Date.now().toString(36).slice(-5)}`
    await agentHere.open({ what: 'change', path: repo(idea), title: 'Retries that back off' })
    await page.goto(`${fresh.base}/#/o/1`)
    await agentHere.work()
    await openFile(page, 'src/keys.rs')
    await marginNote(page, idea).click()
    await agentHere.work()
    // The entry your agent wrote is read at once, and says it waits.
    const entry = page.getByRole('article', { name: idea })
    await expect(entry).toContainText('Waiting to go to the shared graph until today’s limit resets.', { timeout: 15_000 })

    await page.goto(`${fresh.base}/#/you/graph`)
    const today = page.getByLabel('Sent today')
    await expect(today).toContainText('None yet.')
    await expect(today).toContainText('You’ve added as many ideas as a new account can today. The rest goes at')
    await expect(today).toContainText('The limit rises once ten of your entries are accepted, or after a week.')
  } finally {
    fresh.stop()
    g.stop()
  }
})
