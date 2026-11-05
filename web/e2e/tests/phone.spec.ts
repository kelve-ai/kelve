// kelve on a phone: everything fits, with nothing to scroll sideways.

import { expect, test } from '@playwright/test'
import { me, open, openFile, repo, signIn } from '../world'

const sideways = (page: import('@playwright/test').Page) => page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)

for (const path of ['/', '/#/skills', '/#/agent', '/#/agent/look', '/#/settings/agent', '/#/welcome']) {
  test(`${path} fits a phone`, async ({ page }) => {
    await me().hello()
    await signIn(page)
    await page.goto(path)
    await expect(page.locator('main p, main h1').first()).toBeVisible()
    expect(await sideways(page)).toBeLessThanOrEqual(0)
  })
}

test('what your agent opened fits a phone, its files a sheet away', async ({ page }) => {
  await open(page, { what: 'folder', path: repo() })
  expect(await sideways(page)).toBeLessThanOrEqual(0)
  await page.getByRole('button', { name: 'Files', exact: true }).click()
  await openFile(page, 'src/retry.rs')
  await expect(page.locator('.cm-line').filter({ hasText: 'pub fn wait' })).toBeVisible()
  expect(await sideways(page)).toBeLessThanOrEqual(0)
})
