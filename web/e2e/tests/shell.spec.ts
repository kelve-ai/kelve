// kelve's header and its screens (board Shell): Home, Skills and Your agent,
// Settings and its sections, and light or dark, one click away. Each screen
// still being built says what it will show; the old start page and settings
// lead to where they went.

import { expect, test } from '@playwright/test'
import { me, signIn } from '../world'

test('the header leads to each place, and says which one you’re in', async ({ page }) => {
  await me().hello()
  await signIn(page)
  await page.goto('/#/')
  const nav = page.getByRole('navigation', { name: 'kelve' })
  await expect(nav.getByRole('link', { name: 'Home' })).toHaveAttribute('aria-current', 'page')
  await expect(page.getByRole('region', { name: 'What happened' })).toBeVisible()
  await expect(page.getByRole('img', { name: 'Your agent' })).toBeVisible()

  await nav.getByRole('link', { name: 'Skills' }).click()
  await expect(page).toHaveURL(/#\/skills$/)
  // Whether there are skills yet depends on what ran before: only the page.
  await expect(page.getByRole('main').getByRole('heading', { level: 1 })).toBeVisible()
  await expect(nav.getByRole('link', { name: 'Skills' })).toHaveAttribute('aria-current', 'page')

  await nav.getByRole('link', { name: 'Your agent' }).click()
  await expect(page.getByRole('heading', { level: 1, name: 'Your agent' })).toBeVisible()

  await page.getByRole('link', { name: 'Settings' }).click()
  await expect(page).toHaveURL(/#\/settings\/agent$/)
  const sections = page.getByRole('navigation', { name: 'Settings' })
  await sections.getByRole('link', { name: 'Skill checks' }).click()
  await expect(page.getByRole('region', { name: 'Skill checks' }).getByRole('heading', { level: 2 })).toHaveText('Skill checks')
  await expect(sections.getByRole('link', { name: 'Skill checks' })).toHaveAttribute('aria-current', 'page')

  // Home, by kelve's name; and the old start page leads home.
  await page.getByRole('link', { name: 'kelve', exact: true }).click()
  await expect(page.getByRole('region', { name: 'What happened' })).toBeVisible()
  await page.goto('/#/start')
  await expect(page).toHaveURL(/#\/$/)
})

test('light or dark, one click away, kept as chosen', async ({ page }) => {
  await me().hello()
  await signIn(page)
  await page.goto('/#/skills')
  await page.getByRole('button', { name: 'Switch to the dark theme' }).click()
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')
  await page.reload()
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')
  await page.getByRole('button', { name: 'Switch to the light theme' }).click()
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light')
})

test('every screen still being built says what it will show', async ({ page }) => {
  await me().hello()
  await signIn(page)
  const screens: Array<[string, string]> = [
  ]
  for (const [path, name] of screens) {
    await page.goto(`/#/${path}`)
    await expect(page.getByRole('heading', { level: 1, name, exact: true })).toBeVisible()
    await expect(page.getByText('This page is still being built.')).toBeVisible()
  }
  // Meeting your agent has no places to go: only a way to skip.
  await page.goto('/#/welcome')
  await expect(page.getByRole('heading', { level: 1, name: 'Meet your agent' })).toBeVisible()
  await page.getByRole('link', { name: 'Skip for now' }).click()
  await expect(page.getByRole('region', { name: 'What happened' })).toBeVisible()
  // Anywhere else, there's no page.
  await page.goto('/#/nowhere')
  await expect(page.getByRole('heading', { name: 'There’s no page here.' })).toBeVisible()
  // The old settings lead to the new.
  await page.goto('/#/you/reading')
  await expect(page).toHaveURL(/#\/settings\/reading$/)
  await expect(page.getByRole('region', { name: 'Reading' })).toBeVisible()
})

test('the header leads to the docs, in a new tab', async ({ page }) => {
  await page.goto('/#/example/home')
  const docs = page.getByRole('banner').getByRole('link', { name: 'Docs' })
  await expect(docs).toHaveAttribute('href', 'https://kelve.ai/docs/')
  await expect(docs).toHaveAttribute('target', '_blank')
})

test('nothing moves between the places and between settings’ sections', async ({ page }) => {
  for (const width of [1440, 390]) {
    await page.setViewportSize({ width, height: 900 })
    const at = async (selector: string) => {
      const box = await page.locator(selector).first().boundingBox()
      return box && [box.x, box.y, box.width]
    }
    const header = async () => ({ logo: await at('.hd-logo'), home: await at('.hd-nav a:nth-child(1)'), skills: await at('.hd-nav a:nth-child(2)'), agent: await at('.hd-nav a:nth-child(3)'), docs: await at('.hd-docs'), settings: await at('.hd-set'), theme: await at('.hd .theme-switch') })
    const go = async (path: string, wait: string) => {
      await page.goto(`/#/example/${path}`)
      await expect(page.locator(wait).first()).toBeVisible()
    }
    await go('home', '.hm-me')
    const top = await header()
    const left = (await at('.hm-me'))?.[0]
    for (const [path, wait] of [['skills', '.sk-item'], ['agent', '.ya-top']] as const) {
      await go(path, wait)
      expect(await header(), `the header on ${path} at ${width}`).toEqual(top)
    }
    await go('settings/agent', '.sets-nav')
    expect(await header(), `the header on settings at ${width}`).toEqual(top)
    const nav = await at('.sets-nav')
    const heading = await at('.sets-body > h2')
    if (width > 900) expect((await at('.sets-nav'))?.[0], 'home and settings start at the same edge').toBe(left)
    for (const section of ['account', 'checks', 'reading', 'privacy', 'about']) {
      await go(`settings/${section}`, '.sets-nav')
      expect(await at('.sets-nav'), `its sections on ${section} at ${width}`).toEqual(nav)
      expect(await at('.sets-body > h2'), `its heading on ${section} at ${width}`).toEqual(heading)
      expect(await header(), `the header on ${section} at ${width}`).toEqual(top)
    }
  }
})
