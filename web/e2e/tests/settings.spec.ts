// Settings (board Settings): reading, with what kelve marks for you; your
// own notes, under your agent; and privacy, with anonymous counts.

import { mkdtempSync, realpathSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { expect, test } from '@playwright/test'
import { me, picks, signIn } from '../world'

test('a browser that isn’t signed in is told how to open kelve', async ({ page }) => {
  await page.goto('/')
  await expect(page.getByText('kelve open')).toBeVisible()
})

test('reading: text size and code colours change how code is drawn', async ({ page }) => {
  await signIn(page)
  await page.goto('/#/settings/reading')
  const reading = page.getByRole('region', { name: 'Reading' })
  await reading.getByRole('group', { name: 'Text size' }).getByRole('button', { name: 'Large' }).click()
  await expect(page.locator('html')).toHaveAttribute('data-code', 'large')
  await reading.getByRole('group', { name: 'Code colours' }).getByRole('button', { name: 'None' }).click()
  await expect(page.locator('html')).toHaveAttribute('data-highlighting', 'none')
  await reading.getByRole('group', { name: 'Text size' }).getByRole('button', { name: 'Medium' }).click()
  await reading.getByRole('group', { name: 'Code colours' }).getByRole('button', { name: 'Full' }).click()
  await expect(page.locator('html')).toHaveAttribute('data-highlighting', 'full')
})

test('what kelve marks: set in settings, kept, and written by your agent', async ({ page }) => {
  await signIn(page)
  await page.goto('/#/settings/reading')
  const reading = page.getByRole('region', { name: 'Reading' })
  await reading.getByRole('button', { name: 'Edit marks.md' }).click()
  await expect(reading).toContainText('marks.md')
  // Changed here, each choice kept at once.
  await reading.getByRole('radiogroup', { name: 'Mark as if I’m' }).getByRole('radio', { name: 'Working' }).click()
  await reading.getByRole('textbox', { name: 'Add an area' }).fill('Rust')
  await reading.getByRole('textbox', { name: 'Add an area' }).press('Enter')
  await reading.getByRole('radiogroup', { name: 'rust', exact: true }).getByRole('radio', { name: 'New' }).click()
  await page.reload()
  await reading.getByRole('button', { name: 'Edit marks.md' }).click()
  await expect(reading.getByRole('radiogroup', { name: 'Mark as if I’m' }).getByRole('radio', { name: 'Working' })).toHaveAttribute('aria-checked', 'true')
  await expect(reading.getByRole('radiogroup', { name: 'rust', exact: true }).getByRole('radio', { name: 'New' })).toHaveAttribute('aria-checked', 'true')
  await expect(reading).toContainText('Written by you')
  // Written by the agent, as set_marks does: the page shows it, and who wrote it.
  await me().setMarks({ mark: 'learning', areas: [], always: [], never: [], words: '' })
  await expect(reading).toContainText('Written by Claude Code')
  await expect(reading.getByRole('radiogroup', { name: 'Mark as if I’m' }).getByRole('radio', { name: 'Learning' })).toHaveAttribute('aria-checked', 'true')
  await expect(reading.getByRole('radiogroup', { name: 'rust', exact: true })).toHaveCount(0)
})

test('your own notes go to a folder you choose in the picker, and back to kelve’s', async ({ page }) => {
  await me().hello()
  await signIn(page)
  await page.goto('/#/settings/agent')
  const agent = page.getByRole('region', { name: 'Your agent' })
  const folder = realpathSync(mkdtempSync(join(tmpdir(), 'kelve-e2e-notes-')))
  picks(folder)
  await agent.getByRole('button', { name: 'Choose a folder' }).click()
  await expect(agent).toContainText(folder.split('/').pop()!)
  await page.reload()
  await expect(agent).toContainText(folder.split('/').pop()!)
  await agent.getByRole('button', { name: 'Use kelve’s own folder' }).click()
  await expect(agent).toContainText('kelve’s own folder')
})

test('privacy says who gets what, and anonymous counts are shared only when you say', async ({ page }) => {
  await signIn(page)
  await page.goto('/#/settings/privacy')
  const privacy = page.getByRole('region', { name: 'Privacy' })
  await expect(privacy).toContainText('No one else gets anything.')
  const counts = privacy.getByRole('switch', { name: 'Share anonymous counts' })
  // Every browser shares one kelve: start from it off, and leave it so.
  if ((await counts.getAttribute('aria-checked')) === 'true') await counts.click()
  await expect(counts).toHaveAttribute('aria-checked', 'false')
  await privacy.getByRole('button', { name: 'See what it sends' }).click()
  await expect(privacy.locator('pre').first()).toContainText('"week"')
  await counts.click()
  await expect(counts).toHaveAttribute('aria-checked', 'true')
  await page.reload()
  await expect(counts).toHaveAttribute('aria-checked', 'true')
  await counts.click()
  await expect(counts).toHaveAttribute('aria-checked', 'false')
})
