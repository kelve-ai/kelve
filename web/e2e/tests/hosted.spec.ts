// kelve.ai from the app, against the stand-in service: signing in from Your
// agent, choosing the page's address, a verified run from what goes to its
// score, publishing it, the page on Your agent, and the account in settings,
// down to deleting it.

import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test } from '@playwright/test'
import { me, signIn, world } from '../world'

/** Tells the stand-in service something, as a test can. */
async function standIn(what: Record<string, unknown>) {
  const res = await fetch(`${world().graph}/stand-in/v2`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(what) })
  expect(res.ok).toBeTruthy()
}

/** The person's skills: one the stand-in's runs come out better with. */
function skills() {
  const dir = world().skillsDir
  for (const [name, text] of [
    ['rust', '---\nname: rust\ndescription: How you like Rust.\n---\n# Rust\n'],
    ['standin-expert', '---\nname: standin-expert\ndescription: Does it well.\n---\n'],
  ]) {
    mkdirSync(join(dir, name), { recursive: true })
    writeFileSync(join(dir, name, 'SKILL.md'), text)
  }
}

test('signing in, choosing an address and a verified run, to a score on your page', async ({ page }) => {
  await me().hello()
  await signIn(page)
  skills()
  // Each browser's run is the day's first.
  await standIn({ runs_a_day: 100 })

  await page.goto('/#/agent')
  await page.getByRole('button', { name: 'Get a verified score' }).click()
  const sheet = page.getByRole('dialog', { name: 'Sign in to kelve.ai' })
  await expect(sheet).toContainText('Your work never leaves')
  const tab = page.context().waitForEvent('page')
  await sheet.getByRole('button', { name: 'Continue with GitHub' }).click()
  const back = await tab
  await expect(back.getByRole('heading', { name: 'You’re signed in' })).toBeVisible()
  await back.close()

  // Signed in, the address comes first.
  await expect(page).toHaveURL(/#\/agent\/address$/)
  const address = page.getByLabel('Address')
  await address.fill('alex')
  await expect(page.getByRole('status').filter({ hasText: 'Someone has that one.' })).toBeVisible()
  await expect(page.getByRole('button', { name: 'alex-agent' })).toBeVisible()
  await address.fill('sam')
  await expect(page.getByText('You can have that one.')).toBeVisible()
  await page.getByRole('button', { name: 'Save address' }).click()

  await expect(page.getByRole('heading', { level: 1, name: 'Get a verified score' })).toBeVisible()
  await page.getByRole('button', { name: 'See exactly what goes' }).click()
  await expect(page.getByText('standin-expert/SKILL.md')).toBeVisible()
  await expect(page.getByText('Secrets and paths are taken out first.')).toBeVisible()
  await page.getByRole('button', { name: 'Start the run' }).click()

  // What pays: the stand-in runs locally, so a subscription can too.
  const start = page.getByRole('form', { name: 'Start the run' })
  await expect(start.getByText('Your Claude subscription (from claude setup-token)')).toBeVisible()
  await start.getByText('An API key', { exact: true }).click()
  await expect(start.getByText('About $60 on your Anthropic key.')).toBeVisible()
  const key = start.getByLabel('Your Anthropic API key')
  await key.fill('sk-ant-oat01-not-a-key')
  await start.getByRole('button', { name: 'Start the run' }).click()
  await expect(start.getByRole('alert')).toContainText('That’s a subscription token, not an API key.')
  await key.fill('sk-ant-api03-wrong')
  await start.getByRole('button', { name: 'Start the run' }).click()
  await expect(start.getByRole('alert')).toContainText('Anthropic didn’t accept that key.')
  await key.fill('sk-test-e2e')
  await start.getByRole('button', { name: 'Start the run' }).click()

  await expect(page.getByRole('heading', { level: 1, name: 'Your agent is doing Rust tasks' })).toBeVisible()
  await expect(page.getByRole('button', { name: 'Cancel the run' })).toBeVisible()
  const run = decodeURIComponent(page.url().split('/').pop() ?? '')
  await standIn({ finish: run })

  await expect(page.getByText('Rust tasks better than a standard agent')).toBeVisible({ timeout: 15_000 })
  await expect(page.getByLabel('Your score')).toContainText('10')
  await expect(page.getByText('Verified.')).toBeVisible()
  await page.getByRole('button', { name: 'Publish to your page' }).click()
  await expect(page.getByText('It’s on your page.')).toBeVisible()

  // Your agent, with its page.
  await page.goto('/#/agent')
  await expect(page.getByRole('link', { name: 'kelve.ai/@sam' })).toBeVisible()
  await expect(page.getByText('1 score on your page.')).toBeVisible()
  await expect(page.getByRole('switch', { name: 'Show Rust on Sonnet 5.6 on your page' })).toHaveAttribute('aria-checked', 'true')
  await page.getByRole('button', { name: 'Take your page down' }).click()
  await page.getByRole('button', { name: 'Take it down' }).click()
  await expect(page.getByText('Your page is down. Turn a score on to put it back up.')).toBeVisible()

  // The account in settings: the key kept, then forgotten; then the account deleted.
  await page.goto('/#/settings/account')
  await expect(page.getByText('Signed in as @sam')).toBeVisible()
  await expect(page.getByText('Your Anthropic key')).toBeVisible()
  await page.getByRole('button', { name: 'Forget your Anthropic key' }).click()
  await expect(page.getByText('Your Anthropic key')).toHaveCount(0)
  await page.getByRole('button', { name: 'Delete', exact: true }).click()
  await page.getByRole('button', { name: 'Delete my account and page' }).click()
  await expect(page.getByRole('button', { name: 'Sign in to kelve.ai' })).toBeVisible()
})

test('every kelve.ai screen keeps no one out', async ({ page }) => {
  const { AxeBuilder } = await import('@axe-core/playwright')
  await page.emulateMedia({ reducedMotion: 'reduce' })
  for (const path of ['agent', 'agent?as=signed-out', 'agent?as=sign-in', 'agent/address?as=taken', 'agent/score/new?as=confirm', 'agent/score/new?as=subscription', 'agent/score/2', 'agent/score/4', 'agent/score/1', 'agent/score/3', 'settings/account', 'settings/account?as=signed-out']) {
    for (const theme of ['light', 'dark']) {
      await page.goto(`/?${theme}${path}#/example/${path}${path.includes('?') ? '&' : '?'}theme=${theme}`)
      await expect(page.locator('main h1, main h2').first()).toBeVisible()
      if (path.endsWith('as=subscription')) await page.getByRole('button', { name: 'Start the run' }).click()
      if (path.endsWith('as=taken')) await expect(page.getByText('Someone has that one.')).toBeVisible()
      const found = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa']).analyze()
      const said = found.violations.map((v) => `${path} ${theme} ${v.id}: ${v.help} (${v.nodes.map((n) => n.target.join(' ')).join('; ')})`)
      expect(said, said.join('\n')).toEqual([])
    }
  }
})
