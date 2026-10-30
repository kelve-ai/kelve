// When something goes wrong: an error on the page becomes a short report,
// kept and shown in settings with nothing private in it, sent nowhere,
// to post as an issue or forget.

import { expect, test } from '@playwright/test'
import { me, signIn } from '../world'

test('an error on the page is kept as a report you can read, post or forget', async ({ page }) => {
  await me().hello()
  await signIn(page)
  await page.goto('/')
  // What to say to your agent: on the start page whether or not you've opened anything yet.
  await expect(page.getByText('Show me what you did in kelve.')).toBeVisible()
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
  // The start page says so when it's next shown.
  await page.reload()
  await expect(page.getByText('Something went wrong in kelve, and it kept a short report.')).toBeVisible()
  await page.getByRole('link', { name: 'See it in your settings' }).click()

  const section = page.getByRole('region', { name: 'When something goes wrong' })
  const mine = section.locator('li', { hasText: 'Something went wrong on the page' }).first()
  await mine.getByText('See exactly what it says').click()
  const report = mine.locator('.report')
  await expect(report).toContainText('Could not read [path]')
  await expect(report).not.toContainText('sam')
  await expect(report).not.toContainText('secret-launch')
  const issue = await mine.getByRole('link', { name: 'Report it on GitHub' }).getAttribute('href')
  expect(issue).toMatch(/^https:\/\/github\.com\/kelve-ai\/kelve\/issues\/new\?title=/)
  expect(sent, 'kelve sends nothing by itself').toEqual([])

  const before = await section.locator('li').count()
  await mine.getByRole('button', { name: 'forget it' }).click()
  await expect(section.locator('li')).toHaveCount(before - 1)
})
