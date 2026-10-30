// Notes: select lines and leave a note under them, just for you, with the work,
// or for your agent, which picks it up the next time you message it, does it,
// and says what it did.

import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, type Page, test } from '@playwright/test'
import { marginNote, me, open, openFile, pane, repo, world } from '../world'

/** Selects lines of the file shown, from the first through the last, by their text. */
async function selectLines(page: Page, first: string, last: string) {
  await page.evaluate(([a, b]) => {
    const lines = [...document.querySelectorAll<HTMLElement>('.cm-line')]
    const from = lines.find((l) => l.textContent?.includes(a))!
    const to = lines.find((l) => l.textContent?.includes(b))!
    const range = document.createRange()
    range.setStart(from.firstChild ?? from, 0)
    const end = to.lastChild ?? to
    range.setEnd(end, end.textContent?.length ?? 0)
    getSelection()!.removeAllRanges()
    getSelection()!.addRange(range)
  }, [first, last])
}

test('a note for your agent waits under its lines, is picked up, and says what was done', async ({ page }) => {
  const root = repo()
  await open(page, { what: 'change', path: root, title: 'Retries that back off' })
  await openFile(page, 'src/retry.rs')
  await expect(marginNote(page, 'Exponential backoff')).toBeVisible()
  await selectLines(page, 'pub fn wait', '200 * 2u64')
  await page.getByRole('button', { name: 'Note', exact: true }).click()
  const box = page.locator('.nbox')
  await box.getByRole('textbox').fill('Cap the wait at 30 seconds.')
  await box.getByRole('radio', { name: 'For Claude Code' }).click()
  // It says where on Claude Code's list it'll be.
  await expect(box.getByRole('radio', { name: /For Claude Code/ })).toContainText(/\d(st|nd|rd|th)/)
  await box.getByRole('button', { name: 'Keep' }).click()
  const well = page.locator('.nwell').filter({ hasText: 'Cap the wait at 30 seconds.' })
  await expect(well).toContainText('For Claude Code · lines 2–3 · Waiting for your next message to it')

  // The agent reads it, fenced, with its lines as they are now; and does it.
  const waiting = await me().notes(join(root, 'src'))
  expect(waiting).toHaveLength(1)
  expect(waiting[0].note).toContain('Cap the wait at 30 seconds.')
  expect(waiting[0].textNow).toContain('pub fn wait')
  await me().noteDone(root, waiting[0].id, waiting[0].version, 'Capped it with `.min(30_000)`.')
  await expect(well).toContainText('Done just now')
  await expect(well.locator('code')).toHaveText('.min(30_000)')
  expect(await me().notes(root)).toEqual([])

  // Changed since it was done: it waits again, what was done stays under
  // it, and the agent is given only what's new.
  const side0 = page.getByRole('complementary', { name: 'What’s open' })
  await side0.getByRole('tab', { name: 'Notes' }).click()
  await side0.getByRole('button', { name: /Cap the wait/ }).click()
  const editing = page.getByRole('region', { name: 'The note' })
  await editing.getByRole('textbox').click()
  await page.keyboard.press('ControlOrMeta+End')
  await page.keyboard.type('\nAnd log when it is reached.')
  await expect(editing.getByRole('status')).toHaveText('Saved')
  await expect(editing).toContainText('Changed since it was done, so waiting for your next message to it')
  await expect(editing).toContainText('Capped it with')
  const again = await me().notes(root)
  expect(again).toHaveLength(1)
  expect(again[0].doneBefore?.did).toBe('Capped it with `.min(30_000)`.')
  expect(again[0].added).toEqual(['And log when it is reached.'])
  // Done for the words it read before, it isn't: the note changed since.
  await expect(me().noteDone(root, again[0].id, waiting[0].version, 'Logged it.')).rejects.toThrow('read it again with notes')
  await me().noteDone(root, again[0].id, again[0].version, 'Logged it, once.')
  await editing.getByRole('button', { name: 'Close the note' }).click()
  await expect(well).toContainText('Done just now')
  await expect(well).toContainText('Logged it, once.')

  // In the Notes tab, under For Claude Code; opened, a page beside the work.
  const side = page.getByRole('complementary', { name: 'What’s open' })
  await side.getByRole('tab', { name: 'Notes' }).click()
  await expect(side.getByRole('region', { name: 'For Claude Code' })).toContainText('Cap the wait at 30 seconds.')
  await side.getByRole('button', { name: /Cap the wait/ }).click()
  const note = page.getByRole('article', { name: 'Cap the wait at 30 seconds.' })
  await expect(note).toContainText('Logged it, once.')

  // Done, it's cleared for fresh notes: kept, empty, waiting for nothing,
  // until something's written in it, which waits for the agent again.
  await note.getByRole('button', { name: 'Clear it for new notes' }).click()
  const page2 = page.getByRole('region', { name: 'The note' })
  await expect(page2).not.toContainText('Logged it, once.')
  await expect(page2.getByRole('textbox')).not.toContainText('Cap the wait')
  expect(await me().notes(root)).toEqual([])
  await page2.getByRole('textbox').click()
  await page.keyboard.type('Now do the same for refunds.')
  await expect(page2.getByRole('status')).toHaveText('Saved')
  await expect(page2).toContainText('Waiting for your next message to it')
  const fresh = await me().notes(root)
  expect(fresh).toHaveLength(1)
  expect(fresh[0].note).toContain('Now do the same for refunds.')
  await page2.getByRole('button', { name: 'Close the note' }).click()
  await expect(page2).toHaveCount(0)
})

test('your own notes on an idea show in its entry, in any project', async ({ page }) => {
  await open(page, { what: 'change', path: repo(), title: 'Retries that back off' })
  await openFile(page, 'src/retry.rs')
  await marginNote(page, 'Exponential backoff').click()
  const entry = pane(page, 'Exponential backoff')
  await entry.getByRole('button', { name: 'Add a note' }).click()
  const pageNote = page.getByRole('region', { name: 'The note' })
  await pageNote.getByRole('textbox').click()
  await page.keyboard.type('Backoff alone still lets clients collide.')
  // [[ finds an idea, a file or a line as you type, and links it.
  await page.keyboard.type(' See [[expon')
  await expect(page.getByRole('option', { name: /Exponential backoff/ }).first()).toBeVisible()
  // CodeMirror takes Enter as a choice only 75ms after the list shows
  // (interactionDelay), so a key already on its way isn't one; sooner, it's
  // a new line. Anyone choosing takes longer than that to see the list.
  await page.waitForTimeout(100)
  await page.keyboard.press('Enter')
  await expect(pageNote.getByRole('textbox')).toContainText('See [[Exponential backoff]]')
  await expect(pageNote.getByRole('status')).toHaveText('Saved')
  await pageNote.getByRole('button', { name: 'Close the note' }).click()

  // Another project: the note is there, beside the same idea.
  await open(page, { what: 'change', path: repo(), title: 'Another change' })
  await openFile(page, 'src/retry.rs')
  await marginNote(page, 'Exponential backoff').click()
  const again = pane(page, 'Exponential backoff')
  await expect(again.getByRole('region', { name: 'Your notes on it' })).toContainText('Backoff alone still lets clients collide.')
  await expect(again).toContainText('Shown wherever Exponential backoff turns up, in any project.')
})

test('the hook goes into Claude Code’s settings only when you turn it on', async ({ page }) => {
  await open(page, { what: 'change', path: repo(), title: 'Retries that back off' })
  const file = join(world().claudeDir, 'settings.json')
  const set = (on: boolean) => page.evaluate((on) => fetch('/api/settings', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ notesHook: on }) }).then((r) => r.json()), on)
  expect((await set(true)).notesHook).toBe(true)
  expect(readFileSync(file, 'utf8')).toContain('kelve moment')
  expect((await set(false)).notesHook).toBe(false)
  expect(readFileSync(file, 'utf8')).not.toContain('kelve moment')
})
