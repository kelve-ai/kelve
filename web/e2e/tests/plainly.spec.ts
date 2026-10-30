// A file read plainly: the switch on its name line; while your agent writes it,
// where it is on its list, with the code meanwhile; then what it's for, and
// each part, marked as the change left it, with the ideas woven in, what it did
// before, and its code a click away, in the lines around it, drawn as the
// file's code is. Set-up is one line, Also here. A text file reads the same
// way, its sections' words a click away. A change opens on its own page, and a
// folder's page is the folder, each part a click from what it names.

import { execFileSync } from 'node:child_process'
import { appendFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test } from '@playwright/test'
import { marginNote, me, open, openFile, pane, repo, toFiles } from '../world'

test('a code file reads plainly, a part at a time, its code a click away', async ({ page }, info) => {
  const root = repo()
  // A read plainly is kept by the file's text: this run's text is its own.
  appendFileSync(join(root, 'src/retry.rs'), `\n// Read plainly in ${info.project.name}.\n`)
  await open(page, { what: 'change', path: root, title: 'Retries that back off' })
  await openFile(page, 'src/retry.rs')
  const name = page.locator('.fv-name')
  const switcher = name.getByRole('radiogroup', { name: 'Read it' })
  await expect(switcher.getByRole('radio', { name: 'As code' })).toHaveAttribute('aria-checked', 'true')
  await switcher.getByRole('radio', { name: 'Plainly' }).click()

  // Asked of your agent, on its list, until it's written; the code meanwhile.
  await expect(page.locator('.pl-waiting .wline')).toContainText('Claude Code')
  await expect(page.locator('.fv .cm-content')).toContainText('200 * 2u64.pow(attempt)')
  await page.getByRole('button', { name: /^Claude Code .*: its list,/ }).click()
  const list = page.locator('.lpane')
  // Reading it plainly is part of reading what's open.
  await expect(list.getByRole('listitem').filter({ hasText: 'Reading' })).toContainText('1 file')
  await list.getByRole('button', { name: 'Close the list' }).click()
  await me().work()

  // What it's for, what the change did, then each part, marked as the change left it.
  const plain = page.locator('.fv .pl')
  await expect(plain.locator('.lede')).toHaveText('Does what its lines say, as the stand-in agent reads it.')
  await expect(plain.locator('p.chg')).toContainText('It changed, as the stand-in agent reads it.')
  await expect(plain.getByRole('heading', { name: /How it works/ })).toContainText('1 part')
  const part = page.getByRole('region', { name: 'Does lines 1 to 4' })
  await expect(part.locator('.mkt')).toHaveText('New')
  await expect(part).toContainText('Does what lines 1 to 4 say, as the stand-in agent reads them.')
  await expect(part.locator('.was')).toContainText('Did less, before the change.')
  await expect(part.locator('.pln')).toHaveText('1–4')
  // Set-up, in a line at the end, not a part.
  await expect(plain.locator('.fold3')).toContainText('Does lines 6 to 6')
  await expect(page.getByRole('region', { name: 'Does lines 6 to 6' })).toHaveCount(0)

  // Its code a click away: its lines and the ones around them, numbered, with what the change added.
  await part.getByRole('button', { name: 'Show the code' }).click()
  await expect(part.locator('.cm-content')).toContainText('200 * 2u64.pow(attempt)')
  await expect(part.locator('.cm-lineNumbers')).toContainText('4')
  await expect(part.locator('.cm-diff .added:visible')).toHaveCount(6)
  await expect(part.locator('.cm-line.cm-around')).toHaveCount(2)
  await expect(part.locator('.cm-line.cm-around').last()).toContainText('Read plainly in')
  // Each idea in it is named under its lines, as in the file.
  await expect(part.locator('.mnote.under')).toContainText('Exponential backoff')
  // An idea, woven into its sentence, opens its entry beside it, and its part is tinted.
  await part.getByRole('button', { name: /Exponential backoff/ }).first().click()
  await me().work()
  await expect(pane(page, 'Exponential backoff').getByRole('heading', { level: 2, name: 'Exponential backoff' })).toBeVisible()
  await expect(part).toHaveClass(/\bon\b/)
  await part.getByRole('button', { name: 'Hide the code' }).click()
  await expect(part.locator('.cm-content')).toHaveCount(0)

  // Back to the code, with its ideas in the margin; another file reads as you chose.
  await page.reload()
  await expect(page.getByRole('region', { name: 'Does lines 1 to 4' })).toBeVisible()
  await page.locator('.fv-name').getByRole('radiogroup', { name: 'Read it' }).getByRole('radio', { name: 'As code' }).click()
  await expect(marginNote(page, 'Exponential backoff')).toBeVisible()
  await page.getByRole('radio', { name: 'Every file' }).click()
  await openFile(page, 'src/ledger.rs')
  await expect(page.locator('.fv-name').getByRole('radio', { name: 'As code' })).toHaveAttribute('aria-checked', 'true')
})

test('a text file reads plainly, a section at a time, its words a click away', async ({ page }, info) => {
  const root = repo()
  writeFileSync(join(root, 'RETRIES.md'), `# Retries\n\nA call that fails is tried again, in ${info.project.name}.\n\n## Why\n\nSo the bank has room to recover.\n`)
  execFileSync('git', ['add', '.'], { cwd: root })
  await open(page, { what: 'change', path: root, title: 'How retries work' })
  await openFile(page, 'RETRIES.md')
  // Text has the switch too.
  await page.locator('.fv-name').getByRole('radiogroup', { name: 'Read it' }).getByRole('radio', { name: 'Plainly' }).click()
  await expect(page.locator('.pl-waiting .wline')).toContainText('Claude Code')
  await me().work()

  const plain = page.locator('.fv .pl')
  await expect(plain.getByRole('heading', { name: /What it says/ })).toContainText('4 sections')
  const section = page.getByRole('region', { name: 'Does lines 3 to 3' })
  await expect(section.locator('.mkt')).toHaveText('New')
  await expect(section.locator('.pln')).toHaveText('§2')
  await expect(plain.locator('.fold3')).toHaveCount(0)
  await section.getByRole('button', { name: 'Show the text' }).click()
  await expect(section.locator('.ptext')).toContainText('A call that fails is tried again')
  await section.getByRole('button', { name: 'Hide the text' }).click()
  await expect(section.locator('.ptext')).toHaveCount(0)
})

test('a change opens on its own page, plainly, each part a click from its files', async ({ page }) => {
  await open(page, { what: 'change', path: repo(), title: 'Retries that back off', said: 'Each retry now waits twice as long as the last.' })
  // The change's own page, not a file: why, as your agent said it, first.
  await expect(page.getByRole('heading', { level: 2, name: 'Retries that back off' })).toBeVisible()
  await expect(page.locator('.pl')).toContainText('Claude Code said: “Each retry now waits twice as long as the last.”')
  await me().work()
  const how = page.getByRole('region', { name: 'How it works now' })
  await expect(how).toContainText('1 part, by purpose', { timeout: 15_000 })
  await expect(page.locator('.pl .lede')).toHaveText('Changes what its files do, as the stand-in agent reads it.')
  await expect(page.locator('.pl p.chg')).toContainText('Updates')
  // Its files, in the order to read them, each with how much changed.
  const files = page.getByRole('region', { name: 'Its files, in the order to read them' })
  await expect(files.getByRole('button', { name: /src\/retry\.rs/ })).toContainText('+4')
  // A part's file opens, plainly; the head of the side's files brings the page back.
  await how.getByRole('region', { name: 'Changes a file' }).getByRole('button', { name: 'src/retry.rs' }).click()
  await expect(page.locator('.fv-name')).toContainText('src/retry.rs')
  const side = page.getByRole('complementary', { name: 'What’s open' })
  await toFiles(page)
  await side.getByRole('button', { name: /^Changed/ }).click()
  await expect(how).toBeVisible()
  await expect(side.getByRole('button', { name: /^Changed/ })).toHaveAttribute('aria-current', 'page')
})

test('a folder’s page is the folder, plainly: its parts meanwhile, then how it works', async ({ page }) => {
  const root = repo()
  // Its folders are read plainly as part of reading it: before your agent
  // has.
  await open(page, { what: 'folder', path: root }, { work: false })
  const side = page.getByRole('complementary', { name: 'What’s open' })
  await toFiles(page)
  await side.getByRole('button', { name: 'src', exact: true }).click()
  await expect(page.getByRole('heading', { level: 2, name: 'src' })).toBeVisible()
  // While your agent writes it: where it is, and its parts meanwhile.
  await expect(page.locator('.pl-waiting .wline')).toContainText('Claude Code')
  await expect(page.getByRole('region', { name: 'Its parts' })).toContainText('ledger.rs')
  await me().work()
  const how = page.getByRole('region', { name: 'How it works' })
  await expect(how).toContainText('2 parts, by what each does', { timeout: 15_000 })
  await expect(page.locator('.pl .lede')).toHaveText('Holds what its parts do, as the stand-in agent reads it.')
  // Each part opens, plainly.
  await how.getByRole('button', { name: /^Open it\s*: ledger\.rs$/ }).click()
  await expect(page.locator('.fv-name')).toContainText('src/ledger.rs')
})
