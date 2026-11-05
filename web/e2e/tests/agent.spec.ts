// Your agent (boards Onboarding, Customise and YourAgent): meeting it the
// first time kelve opens, then home; changing how it looks, kept on Save;
// and asking it to draw a part or a whole character, which the stand-in
// agent does as kelve's `draw` job, to try on.

import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, type Page, test } from '@playwright/test'
import type { CharacterView } from '../../src/api/types'
import { startKelve } from '../stack'
import { me, signIn, world } from '../world'

/** The person's agent, as kelve keeps it. */
async function character(page: Page, base = world().base): Promise<CharacterView> {
  return (await (await page.request.get(`${base}/api/character`)).json()) as CharacterView
}

/** Back to how kelve ships it, with no name given, for the tests after. */
async function asShipped(page: Page) {
  const shipped = (await (await page.request.get(`${world().base}/api/character`)).json()) as CharacterView
  const look = { ...shipped.look, character: 'robot' }
  look.robot = { ...look.robot, parts: { head: 'round', antenna: 'ball', hat: 'none', eyes: 'big', mouth: 'smile', ears: 'bolts', body: 'box', chest: 'panel', hands: 'hands', legs: 'legs' } }
  await page.request.post(`${world().base}/api/character`, { data: { name: '', look } })
}

test('meet your agent: pick a look, give it a name, and start', async ({ page, browserName }) => {
  const port = Number(process.env.E2E_MEET_PORT ?? 4675) + (browserName === 'webkit' ? 2 : 0)
  const fresh = await startKelve(port, world().graph, { log: `e2e/.meet-${browserName}.log` })
  try {
    // Sam's first name, from their git config: their agent's until they give it one.
    const home = join(fresh.claudeDir, '..', 'home')
    mkdirSync(home, { recursive: true })
    writeFileSync(join(home, '.gitconfig'), '[user]\n\tname = Sam Clark\n')
    await signIn(page, fresh, { new: true })
    await expect(page).toHaveURL(/#\/welcome$/)
    await expect(page.getByRole('heading', { level: 1, name: 'Meet your agent' })).toBeVisible()
    await expect(page.getByRole('navigation', { name: 'kelve' })).toHaveCount(0)
    const looks = page.getByRole('radiogroup', { name: 'How it looks' })
    await expect(looks.getByRole('radio', { name: 'Sky' })).toHaveAttribute('aria-checked', 'true')
    await looks.getByRole('radio', { name: 'Sunny' }).click()
    await expect(looks.getByRole('radio', { name: 'Sunny' })).toHaveAttribute('aria-checked', 'true')
    const name = page.getByLabel('Give it a name, if you like')
    await expect(name).toHaveAttribute('placeholder', 'Sam’s agent')
    // A name with markup in it is turned away, in a sentence.
    await name.fill('<b>Bolt</b>')
    await page.getByRole('button', { name: 'Start' }).click()
    await expect(page.getByRole('alert')).toContainText('markup')
    await name.fill('Bolt')
    await page.getByRole('button', { name: 'Start' }).click()
    await expect(page).toHaveURL(/#\/$/)
    await expect(page.getByRole('navigation', { name: 'kelve' }).getByRole('link', { name: 'Home' })).toHaveAttribute('aria-current', 'page')
    const kept = await character(page, fresh.base)
    expect(kept.name).toBe('Bolt')
    expect(kept.look.robot.parts.head).toBe('box')
    expect(kept.look.robot.colours.body).toBe('#ffd166')
    // Met: home stays home.
    await page.reload()
    await expect(page).toHaveURL(/#\/$/)
    await page.getByRole('navigation', { name: 'kelve' }).getByRole('link', { name: 'Your agent' }).click()
    await expect(page.getByRole('heading', { level: 1, name: 'Bolt' })).toBeVisible()
  } finally {
    fresh.stop()
  }
})

test('change how it looks, and keep it', async ({ page }) => {
  await me().hello()
  await signIn(page)
  try {
    await page.goto('/#/agent')
    await expect(page.getByRole('heading', { level: 2, name: 'What it knows' })).toBeVisible()
    await expect(page.getByRole('link', { name: 'Get a verified score' })).toBeVisible()
    await page.getByRole('link', { name: 'Change how it looks' }).click()
    await expect(page).toHaveURL(/#\/agent\/look$/)
    const parts = page.getByRole('group', { name: 'Parts' })
    await parts.getByRole('button', { name: 'Hat' }).click()
    await page.getByRole('group', { name: 'Hat' }).getByRole('button', { name: 'Crown', exact: true }).click()
    await expect(page.getByRole('group', { name: 'Hat' }).getByRole('button', { name: 'Crown', exact: true })).toHaveAttribute('aria-pressed', 'true')
    await parts.getByRole('button', { name: 'Colours' }).click()
    await page.getByRole('button', { name: 'Body 3' }).click()
    await page.getByRole('group', { name: 'See it move' }).getByRole('button', { name: 'Pleased' }).click()
    await expect(page.getByRole('region', { name: 'How it looks now' }).locator('svg.a-happy')).toBeVisible()
    await page.getByLabel('Name, if you’d like one').fill('Bolt')
    await page.getByRole('button', { name: 'Save' }).click()
    await expect(page).toHaveURL(/#\/agent$/)
    await expect(page.getByRole('heading', { level: 1, name: 'Bolt' })).toBeVisible()
    let kept = await character(page)
    expect(kept.look.robot.parts.hat).toBe('crown')
    expect(kept.look.robot.colours.body).toBe('#ffd166')

    // A whole character instead; Cancel keeps what was saved.
    await page.goto('/#/agent/look')
    await page.getByRole('group', { name: 'Characters' }).getByRole('button', { name: 'Cat', exact: true }).click()
    await expect(page.getByRole('group', { name: 'Parts' }).getByRole('button', { name: 'Hat' })).toHaveCount(0)
    await page.getByRole('link', { name: 'Cancel' }).click()
    expect((await character(page)).look.character).toBe('robot')
    await page.goto('/#/agent/look')
    await page.getByRole('group', { name: 'Characters' }).getByRole('button', { name: 'Cat', exact: true }).click()
    await page.getByRole('button', { name: 'Save' }).click()
    await expect(page).toHaveURL(/#\/agent$/)
    kept = await character(page)
    expect(kept.look.character).toBe('cat')
  } finally {
    await asShipped(page)
  }
})

test('your agent draws a part and a character, to try on', async ({ page }) => {
  await me().hello()
  await signIn(page)
  try {
    await page.goto('/#/agent/look')
    const ask = page.getByLabel('What to draw')
    await ask.fill('a party hat')
    await page.getByRole('button', { name: 'Ask your agent' }).click()
    await expect(page.getByText('Waiting for your agent to draw “a party hat”.')).toBeVisible()
    // Your agent does kelve's jobs: it draws the hat, and kelve checks and keeps it.
    const given = await me().work()
    expect(given.map((j) => j.kind)).toContain('draw')
    // The newest first: what the agent drew in another browser's run is there too.
    const made = page.getByRole('group', { name: 'Made by your agent' })
    await expect(made.getByRole('button', { name: /Party hat/ }).first()).toBeVisible()
    await expect(page.getByText('Waiting for your agent to draw “a party hat”.')).toHaveCount(0)
    await made.getByRole('button', { name: /Party hat/ }).first().click()
    await expect(page.getByRole('group', { name: 'Hat' }).getByRole('button', { name: /Made by your agent/ }).first()).toHaveAttribute('aria-pressed', 'true')
    await page.getByRole('button', { name: 'Save' }).click()
    await expect(page).toHaveURL(/#\/agent$/)
    const kept = await character(page)
    const hat = kept.made.find((m) => m.name === 'Party hat')
    expect(hat?.slot).toBe('hat')
    expect(kept.look.robot.parts.hat).toBe(hat?.key)

    // A whole character, worn from the characters.
    await page.goto('/#/agent/look')
    await ask.fill('a cat')
    await page.getByRole('button', { name: 'Ask your agent' }).click()
    await me().work()
    const cats = page.getByRole('group', { name: 'Characters' })
    await expect(cats.getByRole('button', { name: /Stand-in cat/ }).first()).toBeVisible()
    await cats.getByRole('button', { name: /Stand-in cat/ }).first().click()
    await page.getByRole('button', { name: 'Save' }).click()
    await expect(page).toHaveURL(/#\/agent$/)
    const now = await character(page)
    expect(now.look.character).toBe(now.made.find((m) => m.name === 'Stand-in cat')?.key)
  } finally {
    await asShipped(page)
  }
})
