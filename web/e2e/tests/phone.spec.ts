// kelve on a phone: everything fits, with nothing to scroll sideways.

import { expect, test } from '@playwright/test'
import { marginNote, me, open, openFile, pane, repo, signIn } from '../world'

const sideways = (page: import('@playwright/test').Page) => page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)

for (const path of ['/', '/#/you']) {
  test(`${path} fits a phone`, async ({ page }) => {
    await me().hello()
    await signIn(page)
    await page.goto(path)
    await expect(page.locator('main p, main h1').first()).toBeVisible()
    expect(await sideways(page)).toBeLessThanOrEqual(0)
  })
}

test('the reader fits a phone, each idea under its lines and its entry full screen', async ({ page }) => {
  await open(page, { what: 'change', path: repo(), title: 'Retries that back off' })
  // It opens on the change's own page; its files are a sheet away.
  await expect(page.getByRole('heading', { level: 2, name: 'Retries that back off' })).toBeVisible()
  expect(await sideways(page)).toBeLessThanOrEqual(0)
  await page.getByRole('button', { name: 'Files', exact: true }).click()
  await openFile(page, 'src/retry.rs')
  const note = marginNote(page, 'Exponential backoff')
  await expect(note).toBeVisible()
  expect(await sideways(page)).toBeLessThanOrEqual(0)
  await note.click()
  await expect(pane(page, 'Exponential backoff').getByRole('heading', { level: 2 })).toBeVisible()
  expect(await sideways(page)).toBeLessThanOrEqual(0)
})
