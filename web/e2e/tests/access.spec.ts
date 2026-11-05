// Every screen, and what your agent opens, scanned with axe for what keeps
// people out: missing names, poor contrast, broken roles.

import { AxeBuilder } from '@axe-core/playwright'
import { expect, type Page, test } from '@playwright/test'
import { me, open, openFile, repo, signIn } from '../world'

async function scan(page: Page, include?: string) {
  const builder = new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'])
  const found = await (include ? builder.include(include) : builder).analyze()
  const said = found.violations.map((v) => `${v.id}: ${v.help} (${v.nodes.map((n) => `${n.target.join(' ')}: ${n.any.map((a) => a.message).join(' ')}`).join('; ')})`)
  expect(said, said.join('\n')).toEqual([])
}

test('kelve’s screens under its header, and every control they’re built from', async ({ page }) => {
  await me().hello()
  await signIn(page)
  // Still, so nothing is read half way through coming in.
  await page.emulateMedia({ reducedMotion: 'reduce' })
  for (const path of ['', 'debrief/1', 'skills', 'skills/rust/edit', 'agent', 'agent/look', 'agent/score/new', 'settings/agent', 'settings/checks', 'settings/reading', 'settings/privacy', 'settings/about', 'welcome']) {
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

test('a folder your agent opened, and a file in it', async ({ page }) => {
  await open(page, { what: 'folder', path: repo() })
  await expect(page.getByRole('complementary', { name: 'What’s open' })).toBeVisible()
  await scan(page)
  await openFile(page, 'src/retry.rs')
  await scan(page)
})
