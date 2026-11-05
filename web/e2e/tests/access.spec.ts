// Every page and state of the reader, scanned with axe for what keeps
// people out: missing names, poor contrast, broken roles.

import { AxeBuilder } from '@axe-core/playwright'
import { expect, type Page, test } from '@playwright/test'
import { marginNote, me, open, openFile, pane, repo, signIn, toFiles } from '../world'

async function scan(page: Page, include?: string) {
  const builder = new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'])
  const found = await (include ? builder.include(include) : builder).analyze()
  const said = found.violations.map((v) => `${v.id}: ${v.help} (${v.nodes.map((n) => `${n.target.join(' ')}: ${n.any.map((a) => a.message).join(' ')}`).join('; ')})`)
  expect(said, said.join('\n')).toEqual([])
}

test('the start page and settings', async ({ page }) => {
  await me().hello()
  await signIn(page)
  await page.goto('/#/start')
  // What to say to your agent: on the start page whether or not you've opened anything yet.
  await expect(page.getByText('Show me what you did in kelve.')).toBeVisible()
  await scan(page)
  for (const section of ['agent', 'graph', 'reading', 'notes', 'shared']) {
    await page.goto(`/#/you/${section}`)
    await expect(page.locator('.set-body > h2')).toBeVisible()
    await scan(page)
  }
})

test('kelve’s screens under its header, and every control they’re built from', async ({ page }) => {
  await me().hello()
  await signIn(page)
  // Still, so nothing is read half way through coming in.
  await page.emulateMedia({ reducedMotion: 'reduce' })
  for (const path of ['', 'debrief/example', 'skills', 'skills/rust/edit', 'agent', 'agent/score/new', 'settings/agent', 'settings/privacy', 'welcome']) {
    await page.goto(`/#/${path}`)
    await expect(page.locator('main h1, main h2').first()).toBeVisible()
    await scan(page)
  }
  // Still, so no colour is read half way from light to dark.
  await page.emulateMedia({ reducedMotion: 'reduce' })
  for (const theme of ['light', 'dark']) {
    await page.goto(`/?${theme}#/example/kit?theme=${theme}`)
    await expect(page.getByRole('heading', { level: 1, name: 'Controls' })).toBeVisible()
    await scan(page)
  }
  // Home, the debrief in each of its states, and the change, light and dark.
  for (const theme of ['light', 'dark']) {
    for (const path of ['home', 'home?as=attention', 'debrief/1', 'debrief/1?as=writing', 'debrief/3?as=sent', 'change/1']) {
      await page.goto(`/?${theme}#/example/${path}${path.includes('?') ? '&' : '?'}theme=${theme}`)
      await expect(page.locator('main h1, main h2').first()).toBeVisible()
      await scan(page)
    }
  }
  await page.goto('/?dialog#/example/kit?as=dialog')
  await expect(page.getByRole('dialog', { name: 'Send to your agent?' })).toBeVisible()
  await scan(page)
})

test('the reader, and an entry beside it', async ({ page }) => {
  await open(page, { what: 'change', path: repo(), title: 'Retries that back off', said: 'Each retry now waits twice as long as the last.' })
  // The change's own page, and a folder's.
  await me().work()
  await expect(page.getByRole('region', { name: 'How it works now' })).toBeVisible({ timeout: 15_000 })
  await scan(page)
  await toFiles(page)
  await page.getByRole('complementary', { name: 'What’s open' }).getByRole('button', { name: 'src', exact: true }).click()
  await expect(page.getByRole('heading', { level: 2, name: 'src' })).toBeVisible()
  await scan(page)
  await openFile(page, 'src/retry.rs')
  await expect(marginNote(page, 'Exponential backoff')).toBeVisible()
  await scan(page)
  await marginNote(page, 'Exponential backoff').click()
  await expect(pane(page, 'Exponential backoff').getByRole('heading', { level: 2 })).toBeVisible()
  await scan(page)
})
