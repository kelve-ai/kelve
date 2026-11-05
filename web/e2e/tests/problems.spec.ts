// When something goes wrong: an error on the page becomes a short report,
// kept and shown in settings with nothing private in it, sent nowhere,
// to post as an issue or forget.

import { expect, test } from '@playwright/test'
import { me, signIn, world } from '../world'

test('an error on the page is kept as a report you can read, post or forget', async ({ page }) => {
  await me().hello()
  await signIn(page)
  await page.goto('/#/settings/privacy')
  const privacy = page.getByRole('region', { name: 'Privacy' })
  await expect(privacy).toBeVisible()
  const sent: string[] = []
  page.on('request', (r) => {
    if (!r.url().startsWith(new URL(page.url()).origin)) sent.push(r.url())
  })
  const kept = page.waitForResponse((r) => r.url().endsWith('/api/problems') && r.request().method() === 'POST')
  await page.evaluate(() => {
    setTimeout(() => {
      throw new Error('Could not read /Users/sam/plans/secret-launch.md')
    })
  })
  expect((await kept).ok()).toBe(true)
  // Settings say so, the latest first.
  await page.reload()
  await expect(privacy).toContainText('Something went wrong on the page')
  await privacy.getByRole('button', { name: 'See what it says' }).click()
  const report = privacy.locator('.sreport')
  await expect(report).toContainText('Could not read [path]')
  await expect(report).not.toContainText('sam')
  await expect(report).not.toContainText('secret-launch')
  const issue = await privacy.getByRole('link', { name: 'Report it on GitHub' }).getAttribute('href')
  expect(issue).toMatch(/^https:\/\/github\.com\/kelve-ai\/kelve\/issues\/new\?title=/)
  expect(sent, 'kelve sends nothing by itself').toEqual([])

  const count = async () => ((await (await page.request.get(`${world().base}/api/problems`)).json()) as unknown[]).length
  const before = await count()
  await privacy.getByRole('button', { name: 'Forget it' }).click()
  await expect.poll(count).toBe(before - 1)
})
