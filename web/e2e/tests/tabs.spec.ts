// Showing something again: kelve opens a new tab, which is what brings the
// browser to the front with it shown, and a kelve tab already showing the
// same thing gives it its place, where you were in it, and closes. A tab
// you've moved around in can't close itself: it says it's open in a newer tab.
// One holding a note you haven't kept stays as it is.

import { expect, test, type Page } from '@playwright/test'
import { me, newLink, openFile, repo, selectLines, shown, signIn, world } from '../world'

/** The agent shows something, as its `open` tool does: what's open, and the link kelve gave the person. */
async function show(what: Record<string, unknown>) {
  const w = world()
  const agent = me(w)
  await agent.hello()
  const before = shown(w.logFile).length
  const opened = await agent.open(what)
  return { id: opened.id, link: await newLink(w.logFile, before) }
}

/**
 * A new tab on a link, as the person's browser opens one for kelve: the
 * link is its first page, where a test's new page has a blank one before it.
 */
async function newTab(page: Page, link: string) {
  const tab = await page.context().newPage()
  await tab.evaluate((to) => location.replace(to), link)
  await tab.waitForURL(/#\/o\/\d+/)
  return tab
}

const RETRIES = { what: 'change', title: 'Retries that back off', said: 'Each retry now waits twice as long as the last.' }

test('showing again what a tab shows takes its place, where you were in it, and the old tab closes', async ({ page }) => {
  await signIn(page)
  const root = repo()
  const first = await show({ ...RETRIES, path: root })
  const old = await newTab(page, first.link)
  await expect(old.getByRole('heading', { level: 1, name: 'Retries that back off' })).toBeVisible()
  // Where you were in it: a file, there from the start, so the tab has been nowhere else.
  await old.evaluate((id) => location.replace(`#/o/${id}?f=src%2Fretry.rs&r=code`), first.id)
  await expect(old.locator('.fv-name')).toContainText('src/retry.rs')

  const again = await show({ ...RETRIES, path: root, said: 'Each retry now waits a little at random too.' })
  expect(again.id, 'the same thing, brought up to date').toBe(first.id)
  const closed = old.waitForEvent('close')
  const fresh = await newTab(page, again.link)
  await closed
  await expect(fresh.locator('.fv-name')).toContainText('src/retry.rs')
  expect(new URL(fresh.url()).hash).toBe(`#/o/${first.id}?f=src%2Fretry.rs&r=code`)
  // A kelve tab on something else stays as it was.
  expect(page.isClosed()).toBe(false)
  await expect(page.getByText('This is open in a newer tab.')).toHaveCount(0)
})

test('a tab you moved around in says it’s open in a newer tab, and shows it here again if you ask', async ({ page }) => {
  await signIn(page)
  const root = repo()
  const first = await show({ ...RETRIES, path: root })
  await page.goto(first.link)
  await expect(page.getByRole('heading', { level: 1, name: 'Retries that back off' })).toBeVisible()
  await openFile(page, 'src/retry.rs')

  const again = await show({ ...RETRIES, path: root, said: 'Each retry now waits a little at random too.' })
  const fresh = await newTab(page, again.link)
  await expect(fresh.locator('.fv-name')).toContainText('src/retry.rs')
  await expect(page.getByRole('heading', { level: 1, name: 'This is open in a newer tab.' })).toBeVisible()
  await expect(page.getByText('kelve showed it again there, up to date. You can close this tab.')).toBeVisible()

  await page.getByRole('button', { name: 'Show it here instead' }).click()
  await expect(page.locator('.fv-name')).toContainText('src/retry.rs')
})

test('a tab holding a note you haven’t kept stays as it is', async ({ page }) => {
  await signIn(page)
  const root = repo()
  const first = await show({ ...RETRIES, path: root })
  const old = await newTab(page, first.link)
  await old.evaluate((id) => location.replace(`#/o/${id}?f=src%2Fretry.rs&r=code`), first.id)
  await expect(old.locator('.fv-name')).toContainText('src/retry.rs')
  await expect(old.locator('.cm-line').filter({ hasText: '200 * 2u64' })).toBeVisible()
  await selectLines(old, 'pub fn wait', '200 * 2u64')
  await old.getByRole('button', { name: 'Note', exact: true }).click()
  await old.locator('.nbox').getByRole('textbox').fill('Cap the wait at 30 seconds.')

  const again = await show({ ...RETRIES, path: root, said: 'Each retry now waits a little at random too.' })
  const fresh = await newTab(page, again.link)
  await expect(fresh.getByRole('heading', { level: 1, name: 'Retries that back off' })).toBeVisible()
  // Long enough for it to have gone, had it gone.
  await fresh.waitForTimeout(500)
  expect(old.isClosed()).toBe(false)
  await expect(old.locator('.nbox').getByRole('textbox')).toHaveValue('Cap the wait at 30 seconds.')
  await expect(old.getByText('This is open in a newer tab.')).toHaveCount(0)
})
