// The reader: what your agent opened, each idea in the margin beside its lines;
// a change's file, by what changed; an entry opened beside the work; tracing
// down by clicking down; squares that fill as you read, and the key at the
// side's foot that says so.

import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test } from '@playwright/test'
import { marginNote, me, open, openFile, pane, repo, toFiles } from '../world'

test('a change reads whole, with each idea beside its lines, and its entry opens beside the work', async ({ page }) => {
  const root = repo()
  await open(page, { what: 'change', path: root, title: 'Retries that back off', said: 'Each retry now waits twice as long as the last.' })
  await expect(page.getByRole('heading', { level: 1, name: 'Retries that back off' })).toBeVisible()
  // The agent's line is a click away in the bar, so the work keeps the height.
  const said = page.locator('.said-key').getByText('Each retry now waits twice as long as the last.')
  await expect(said).toBeHidden()
  await page.getByRole('button', { name: 'What your agent said' }).click()
  await expect(said).toBeVisible()
  await page.keyboard.press('Escape')
  await expect(said).toBeHidden()
  // What's new shows on the file that has it, in the side's tree.
  const side = page.getByRole('complementary', { name: 'What’s open' })
  await toFiles(page)
  await expect(side.getByRole('treeitem', { name: /^retry\.rs.*something new in this change/ })).toBeVisible()
  // The key at the side's foot says what the squares mean, and opens what kelve marks.
  const key = side.getByRole('list', { name: 'What the squares mean' })
  await expect(key).toHaveText(/New\s*Known\s*Read/)
  await side.getByRole('button', { name: 'What kelve marks' }).click()
  await expect(page.getByRole('region', { name: 'What kelve marks' })).toBeVisible()
  await page.keyboard.press('Escape')

  // What changed, its idea in the margin with how this work uses it.
  await openFile(page, 'src/retry.rs')
  const note = marginNote(page, 'Exponential backoff')
  await expect(note).toContainText('It’s what this line does.')
  await expect(page.locator('.cm-diff .added').filter({ visible: true }).first()).toBeVisible()

  // Clicked, its entry opens beside the work, which stays.
  await note.click()
  await me().work()
  const entry = pane(page, 'Exponential backoff')
  await expect(entry.getByRole('heading', { level: 2, name: 'Exponential backoff' })).toBeVisible()
  await expect(page.locator('.fv-name')).toContainText('src/retry.rs')
  await expect(page.getByRole('navigation', { name: 'Where you are' })).toContainText('Exponential backoff')
  // Read now: its square fills, wherever it is.
  await expect(note.getByRole('img', { name: 'Exponential backoff, new in this change, read' })).toBeVisible()
  // It closes from its corner, as a note does, and opens again from its line.
  await page.getByRole('button', { name: 'Close the entry' }).click()
  await expect(entry).toBeHidden()
  await note.click()
  await expect(entry.getByRole('heading', { level: 2, name: 'Exponential backoff' })).toBeVisible()

  // What it rests on, clicked, opens beside it; Escape closes it again.
  await expect(entry.getByRole('region', { name: 'How it relates' })).toContainText('Retrying', { timeout: 15_000 })
  // Followed from an entry: kept, to be counted once, with nothing about who.
  const followed = page.waitForRequest((r) => r.url().endsWith('/api/follows') && r.method() === 'POST')
  await entry.getByRole('button', { name: /Retrying/ }).click()
  expect(JSON.parse((await followed).postData() ?? '{}')).toMatchObject({ kind: 'restsOn' })
  await me().work()
  await expect(pane(page, 'Retrying').getByRole('heading', { level: 2, name: 'Retrying' })).toBeVisible()
  await expect(page.getByRole('navigation', { name: 'Where you are' })).toContainText('Retrying')
  await page.keyboard.press('Escape')
  await expect(pane(page, 'Retrying')).toHaveCount(0)
  await expect(entry).toBeVisible()
})

test('kelve’s name says it’s the way to the start page, and Escape goes there once nothing else has it', async ({ page }) => {
  await open(page, { what: 'change', path: repo(), title: 'Retries that back off' })
  await expect(page.getByRole('heading', { level: 1, name: 'Retries that back off' })).toBeVisible()
  // Pointed at, kelve's name says where it leads (Home-1).
  const logo = page.getByRole('link', { name: 'kelve: the start page' })
  await logo.hover()
  await expect(page.getByRole('tooltip')).toHaveText('Start page · Esc')
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
  // Search has it next: Escape closes search, and nothing more.
  await page.getByRole('button', { name: /^Search/ }).click()
  await expect(page.getByRole('dialog')).toBeVisible()
  await page.keyboard.press('Escape')
  await expect(page.getByRole('dialog')).toHaveCount(0)
  await expect(page.getByRole('heading', { level: 1, name: 'Retries that back off' })).toBeVisible()
  // Then an entry beside the work: Escape closes it first.
  await openFile(page, 'src/retry.rs')
  await marginNote(page, 'Exponential backoff').click()
  const entry = pane(page, 'Exponential backoff')
  await expect(entry).toBeVisible()
  await page.keyboard.press('Escape')
  await expect(entry).toHaveCount(0)
  await expect(page.getByRole('heading', { level: 1, name: 'Retries that back off' })).toBeVisible()
  // With nothing open beside the work, Escape goes to the start page.
  await page.keyboard.press('Escape')
  await expect(page).toHaveURL(/#\/$/)
  await expect(page.getByRole('heading', { level: 1, name: 'Retries that back off' })).toHaveCount(0)
})

test('a change’s file shows what changed, with what’s as before folded to a line', async ({ page }) => {
  const root = repo()
  // The ledger, changed at its end: its first six lines are as before.
  const ledger = join(root, 'src/ledger.rs')
  writeFileSync(ledger, `${readFileSync(ledger, 'utf8')}\n/// Gives back what was held.\npub fn release() {}\n`)
  await open(page, { what: 'change', path: root, title: 'Retries that back off' })
  // The side says how much changed, all told and file by file.
  const side = page.getByRole('complementary', { name: 'What’s open' })
  // Not kelve's own, though your agent's way in is kept beside the work.
  await toFiles(page)
  await expect(side.locator('.lgroup.changed')).toContainText('2 files')
  await expect(side.getByRole('treeitem', { name: /^ledger\.rs/ })).toContainText('+3')
  await openFile(page, 'src/ledger.rs')

  // What changed: the lines before folded to one, which opens them.
  await expect(page.getByRole('radio', { name: 'The whole file' })).toHaveCount(0)
  const fold = page.getByRole('button', { name: '6 lines as before' })
  await expect(fold).toBeVisible()
  await expect(page.locator('.cm-line').filter({ hasText: 'use sqlx::PgPool;' })).toHaveCount(0)
  await expect(page.locator('.cm-line.cm-added').first()).toBeVisible()
  await fold.click()
  await expect(fold).toHaveCount(0)
  await expect(page.locator('.cm-line').filter({ hasText: 'use sqlx::PgPool;' })).toBeVisible()

  // Opened again, it shows what changed again.
  await openFile(page, 'src/retry.rs')
  await openFile(page, 'src/ledger.rs')
  await expect(page.getByRole('button', { name: '6 lines as before' })).toBeVisible()

  // A line asked for by the address, folded away, is opened.
  await page.goto(`${page.url()}&l=1`)
  await expect(page.locator('.cm-line').filter({ hasText: 'use sqlx::PgPool;' })).toBeVisible()
})

test('a folder lists every file, and search shows its ideas before you type', async ({ page }) => {
  const root = repo()
  await open(page, { what: 'folder', path: root })
  const side = page.getByRole('complementary', { name: 'What’s open' })
  await toFiles(page)
  await expect(side.getByRole('treeitem', { name: /^ledger\.rs/ })).toBeVisible({ timeout: 15_000 })
  // Every file, with no choice of which: most files have ideas.
  await expect(side.getByRole('radiogroup', { name: 'Which files' })).toHaveCount(0)
  // No Ideas tab: search lists this work's ideas before you type, each opening its entry.
  await expect(side.getByRole('tab', { name: 'Ideas' })).toHaveCount(0)
  await page.getByRole('button', { name: /^Search/ }).click()
  await page.getByRole('region', { name: 'Ideas here' }).getByRole('button', { name: /Advisory locks/ }).click()
  await expect(pane(page, 'Advisory locks')).toBeVisible()
  await side.getByRole('treeitem', { name: /^ledger\.rs/ }).click()
  await expect(marginNote(page, 'Advisory locks')).toBeVisible()
  // The entry closed, so the side has room to widen.
  await page.keyboard.press('Escape')
  await expect(pane(page, 'Advisory locks')).toHaveCount(0)
  // Repos have no change, so every square is grey, and the key says nothing of new ones.
  await expect(page.locator('.sq.new')).toHaveCount(0)
  await expect(side.getByRole('list', { name: 'What the squares mean' })).toHaveText(/^Known\s*Read$/)
  // The side's right edge makes it wider, and it stays that wide.
  const before = (await side.boundingBox())!.width
  const edge = page.getByRole('separator', { name: 'The width of the side' })
  await edge.focus()
  await page.keyboard.press('ArrowRight')
  await page.keyboard.press('ArrowRight')
  await expect.poll(async () => (await side.boundingBox())!.width).toBe(before + 48)
  await page.reload()
  await expect.poll(async () => (await side.boundingBox())!.width).toBe(before + 48)
})

test('search finds what the files say, and opens a file at the line that says it', async ({ page }) => {
  const root = repo()
  await open(page, { what: 'folder', path: root }, { work: false })
  await page.getByRole('button', { name: /^Search/ }).click()
  await page.getByRole('searchbox', { name: /^Search ideas/ }).fill('2u64.pow')
  const hit = page.getByRole('region', { name: 'In the files' }).getByRole('button').filter({ hasText: 'src/retry.rs:' }).first()
  await expect(hit).toBeVisible()
  await hit.click()
  await expect(page.locator('.fv-name')).toContainText('src/retry.rs')
  await expect(page).toHaveURL(/[?&]l=\d+/)
})

test('your agent finds the way in first, and a folder says what it’s for', async ({ page }) => {
  const root = repo()
  await open(page, { what: 'folder', path: root, title: 'The ledger' }, { work: false })
  // In order, first: the way in, each step and why to read it now, as
  // the agent's way-in job gave it.
  const side = page.getByRole('complementary', { name: 'What’s open' })
  await expect(side.getByRole('tab', { name: 'In order' })).toHaveAttribute('aria-selected', 'true')
  await expect(side.getByRole('button', { name: /^The way in/ })).toBeVisible()
  // Its overview first, what it is as a whole, then its files.
  const rows = side.locator('.ord li button')
  await expect(rows).toHaveCount(3)
  await expect(rows.nth(0)).toContainText('What this is')
  const steps = side.locator('.ord li button:has(code)')
  const [first, second] = await Promise.all([0, 1].map((n) => steps.nth(n).locator('code').innerText()))
  await expect(side.getByText(`Why to read ${first} now, said by the stand-in agent.`)).toBeVisible()
  // It opens on the overview, its own page; the next you haven't read says Next.
  await expect(rows.nth(0)).toHaveAttribute('aria-current', 'step')
  await expect(steps.nth(0)).toContainText('Next')
  // Each step is read once it's seen as your agent left it: marked, read
  // as code here. Files read before, by content, may be read at once.
  await steps.nth(0).click()
  await expect(page.locator('.fv-name')).toContainText(first.replace(/\s/g, ''))
  await steps.nth(1).click()
  await expect(page.locator('.fv-name')).toContainText(second.replace(/\s/g, ''))
  await me().work()
  await steps.nth(0).click()
  await expect(page.locator('.fv-name')).toContainText(first.replace(/\s/g, ''))
  await rows.nth(0).click()
  await expect(side.getByText('2 of 3 read')).toBeVisible()
  // All read, or being read: nothing says Next.
  await expect(side.locator('.ord-next')).toHaveCount(0)

  // What you've read is kept with what's open, not in the browser: a fresh
  // one still has it read.
  await page.evaluate(() => localStorage.clear())
  await page.reload()
  await side.getByRole('tab', { name: 'In order' }).click()
  await expect(side.getByText('2 of 3 read')).toBeVisible()

  // A folder says what it's for, under its name, and on its page what it
  // does, as your agent read it plainly.
  await side.getByRole('tab', { name: 'Files' }).click()
  await expect(side.getByText('What src holds, said by the stand-in agent.')).toBeVisible()
  await side.getByRole('button', { name: 'src', exact: true }).click()
  await expect(page.getByRole('heading', { level: 2, name: 'src' })).toBeVisible()
  await expect(page.locator('.pl .lede')).toHaveText('Holds what its parts do, as the stand-in agent reads it.')
})

test('what you asked for is ready for you once written, a count in the bar, until you open it', async ({ page }) => {
  const idea = `Request keys ${Date.now().toString(36).slice(-5)}`
  await open(page, { what: 'change', path: repo(idea), title: 'Retries that back off' })
  await openFile(page, 'src/keys.rs')
  // Asked for by name: an idea with no entry yet, opened.
  await marginNote(page, idea).click()
  await expect(pane(page, idea).locator('.wline')).toContainText('Claude Code')
  await page.keyboard.press('Escape')
  await expect(pane(page, idea)).toHaveCount(0)
  const ready = page.getByRole('button', { name: /^\d+ ready for you$/ })
  await expect(ready).toHaveCount(0)
  // Written while you look elsewhere: the bar says so.
  await me().work()
  await expect(ready).toHaveAccessibleName('1 ready for you', { timeout: 15_000 })
  // Claude Code's list, from the bar, says the same.
  await page.getByRole('button', { name: /^Claude Code .*: its list,/ }).click()
  const list = page.getByRole('region', { name: 'Claude Code’s list' })
  await expect(list.getByRole('region', { name: 'Ready for you' })).toContainText(idea)
  await expect(list.getByRole('button', { name: 'Note for Claude Code' })).toBeVisible()
  await page.keyboard.press('Escape')
  await expect(list).toHaveCount(0)
  // The count opens what's ready; Escape closes it, and a click opens what's in it.
  await ready.click()
  const drop = page.getByRole('region', { name: 'Ready for you' })
  await expect(drop).toContainText(idea)
  await page.keyboard.press('Escape')
  await expect(drop).toHaveCount(0)
  await ready.click()
  await drop.getByRole('button', { name: new RegExp(idea) }).click()
  await expect(pane(page, idea).getByRole('heading', { level: 2, name: idea })).toBeVisible()
  // Opened, it leaves the bar.
  await expect(ready).toHaveCount(0)
})

test('what you take off your agent’s list is gone, and stays off when it’s opened again', async ({ page }) => {
  const path = repo()
  const what = { what: 'change', path, title: 'Retries, taken off' }
  await open(page, what, { work: false })
  await page.getByRole('button', { name: /^Claude Code .*: its list,/ }).click()
  const next = page.getByRole('region', { name: 'Claude Code’s list' }).getByRole('region', { name: 'Next' })
  // Reading what's open: its files, and its pages, each a line.
  const reading = next.getByRole('listitem').filter({ hasText: /^(Reading|Pages|Folder pages)/ })
  await expect(reading.first()).toBeVisible()
  // Pointed at, a row says what you can do with it: Take off is one.
  for (let left = await reading.count(); left > 0; left -= 1) {
    const row = reading.first()
    await row.hover()
    await row.getByRole('button', { name: /^Take (reading|pages|folder pages) .* off the list$/ }).click()
    await expect(reading).toHaveCount(left - 1)
  }
  await expect(reading).toHaveCount(0)
  // Your agent isn't given it, and opening the same thing again doesn't put it back.
  const read = ['read', 'choose', 'plainly']
  expect((await me().work()).filter((j) => read.includes(j.kind))).toEqual([])
  await me().open(what)
  expect((await me().work()).filter((j) => read.includes(j.kind))).toEqual([])
  await expect(next.getByRole('listitem').filter({ hasText: /^(Reading|Pages|Folder pages)/ })).toHaveCount(0)
})

test('how far reading has got moves as your agent reads, with no reload', async ({ page }) => {
  const root = repo()
  await open(page, { what: 'folder', path: root }, { work: false })
  // Being read: the bar says how far. Read, each file once it's read
  // plainly too, and each folder's page: the count goes, with no reload.
  await expect(page.getByText(/reading \d+ of \d+/)).toBeVisible({ timeout: 15_000 })
  await me().work()
  await expect(page.getByText(/reading \d+ of \d+/)).toHaveCount(0, { timeout: 30_000 })
})
