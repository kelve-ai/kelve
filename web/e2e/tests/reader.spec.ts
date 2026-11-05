// What your agent opens in kelve: work it hands over opens as its debrief,
// or file by file while there's none; a folder or files open plainly, the
// files in the side and a file beside them.

import { execFileSync } from 'node:child_process'
import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, type Page, test } from '@playwright/test'
import { open, openFile, repo, toFiles, world } from '../world'

test('work handed over opens as its debrief, or file by file while there’s none', async ({ page }) => {
  await open(page, { what: 'change', path: repo(), title: 'Retries that back off', said: 'Each retry now waits twice as long as the last.' }, { work: false })
  // Never the old reader: the debrief, or the change as it was handed over.
  await expect(page).toHaveURL(/#\/(debrief|handover)\/\d+/)
  await expect(page.getByText('Retries that back off').first()).toBeVisible()
  if (/#\/handover\//.test(page.url())) {
    await expect(page.getByRole('link', { name: '← Back to home' })).toBeVisible()
    await expect(page.getByRole('navigation', { name: 'Files' })).toContainText('retry.rs')
  }
})

test('a folder opens plainly: its files in the side, a file a click away', async ({ page }) => {
  const root = repo()
  await open(page, { what: 'folder', path: root })
  const side = page.getByRole('complementary', { name: 'What’s open' })
  await toFiles(page)
  await expect(side.getByRole('treeitem', { name: /^ledger\.rs/ })).toBeVisible()
  await openFile(page, 'src/retry.rs')
  await expect(page.locator('.cm-line').filter({ hasText: 'pub fn wait' })).toBeVisible()
  // Nothing of what kelve no longer does: no squares, no margin, no search.
  await expect(page.locator('.sq, .mnote, .search-key')).toHaveCount(0)
})

test('kelve’s name leads home, and Escape goes there once nothing else has it', async ({ page }) => {
  await open(page, { what: 'folder', path: repo() })
  const logo = page.getByRole('link', { name: 'kelve: home' })
  await logo.hover()
  await expect(page.getByRole('tooltip')).toHaveText('Home, or Esc')
  await page.mouse.move(700, 500)
  await toFiles(page)
  const here = page.url()
  // Escape in a field is the field's.
  const filter = page.getByRole('complementary', { name: 'What’s open' }).locator('input[type="search"]')
  await filter.fill('retry')
  await filter.press('Escape')
  expect(page.url()).toBe(here)
  await filter.fill('')
  await filter.blur()
  // Your agent's list beside the work: Escape closes it first.
  await page.getByRole('button', { name: /^Claude Code .*: its list,/ }).click()
  const list = page.getByRole('region', { name: 'Claude Code’s list' })
  await expect(list).toBeVisible()
  await page.keyboard.press('Escape')
  await expect(list).toHaveCount(0)
  // With nothing open beside the work, Escape goes home.
  await page.keyboard.press('Escape')
  await expect(page).toHaveURL(/#\/$/)
})

test('a tab out of sight lets its stream go, and catches up once it is back', async ({ page }) => {
  const root = repo()
  const opened = await open(page, { what: 'folder', path: root })
  const side = page.getByRole('complementary', { name: 'What’s open' })
  await side.getByRole('tab', { name: 'Notes' }).click()
  // As the browser says a tab is in sight, or not.
  const sight = (hidden: boolean) =>
    page.evaluate((h) => {
      Object.defineProperty(document, 'hidden', { configurable: true, get: () => h })
      Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => (h ? 'hidden' : 'visible') })
      document.dispatchEvent(new Event('visibilitychange'))
    }, hidden)
  await sight(true)
  // A note kept while it's out of sight, as another tab would keep one.
  const kept = await page.request.post(`${world().base}/api/opened/${opened.id}/notes`, {
    headers: { origin: world().base },
    data: { id: null, place: 'you', text: 'Read the ledger first.', on: { path: 'src/ledger.rs', start: 1, end: 1 } },
  })
  expect(kept.ok()).toBe(true)
  // Out of sight, it hears nothing: it shows what it had.
  await page.waitForTimeout(1_000)
  await expect(side.getByRole('button', { name: /Read the ledger first/ })).toHaveCount(0)
  // Back in sight, it listens again and fetches what it missed.
  await sight(false)
  await expect(side.getByRole('button', { name: /Read the ledger first/ })).toBeVisible({ timeout: 30_000 })
})

/** The colour each of these is drawn in. */
const coloursOf = (page: Page, of: Record<string, string>) =>
  page.evaluate((of) => Object.fromEntries(Object.entries(of).map(([k, sel]) => [k, getComputedStyle(document.querySelector(sel)!).color])), of)

test('code is coloured as Highlighting says, in any language', async ({ page }) => {
  const root = repo()
  writeFileSync(join(root, 'main.tf'), 'resource "aws_s3_bucket" "logs" {\n  bucket = "logs"\n}\n')
  writeFileSync(join(root, 'index.php'), '<?php\nfunction greet() { return "hello"; }\n')
  writeFileSync(join(root, 'init.lua'), 'local name = "world"\n')
  execFileSync('git', ['add', '.'], { cwd: root })
  await open(page, { what: 'folder', path: root })
  for (const [file, keyword, string] of [['main.tf', 'resource', '"aws_s3_bucket"'], ['index.php', 'function', '"hello"'], ['init.lua', 'local', '"world"']]) {
    await openFile(page, file)
    await expect(page.locator('.fv .cm-line span', { hasText: string })).toHaveCount(1)
    for (const mode of ['full', 'quiet', 'none']) {
      await page.evaluate((mode) => (document.documentElement.dataset.highlighting = mode), mode)
      const c = await coloursOf(page, { code: '.fv .cm-content' })
      const drawn = await page.evaluate(([keyword, string]) => {
        const spans = [...document.querySelectorAll<HTMLElement>('.fv .cm-line span')]
        const colour = (text: string) => getComputedStyle(spans.find((s) => s.textContent === text)!).color
        return { keyword: colour(keyword), string: colour(string) }
      }, [keyword, string])
      const why = `${file}, ${mode}`
      // Full colours keywords too; quiet, strings and the like alone; none, nothing.
      expect(drawn.keyword === c.code, why).toBe(mode !== 'full')
      expect(drawn.string === c.code, why).toBe(mode === 'none')
    }
  }
})
