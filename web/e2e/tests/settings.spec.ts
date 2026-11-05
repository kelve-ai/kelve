// Settings (board Settings): reading, with the ideas you know; your agent's
// hook; and privacy, with anonymous counts.

import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test } from '@playwright/test'
import { open, repo, signIn, world } from '../world'

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

test('reading: the ideas you know, with nothing to fill in', async ({ page }) => {
  await signIn(page)
  await page.goto('/#/settings/reading')
  const reading = page.getByRole('region', { name: 'Reading' })
  await expect(reading).toContainText('Ideas you know')
  await expect(reading.getByRole('button', { name: 'Edit marks.md' })).toHaveCount(0)
})

test('the hook goes into Claude Code’s settings only when you turn it on', async ({ page }) => {
  await open(page, { what: 'folder', path: repo() })
  const file = join(world().claudeDir, 'settings.json')
  const set = (on: boolean) => page.evaluate((on) => fetch('/api/settings', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ hook: on }) }).then((r) => r.json()), on)
  expect((await set(true)).hook).toBe(true)
  expect(readFileSync(file, 'utf8')).toContain('kelve moment')
  expect((await set(false)).hook).toBe(false)
  expect(readFileSync(file, 'utf8')).not.toContain('kelve moment')
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
