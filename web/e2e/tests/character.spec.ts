// The app draws your agent's character itself, from core's parts library, as
// people choose how theirs looks: here it draws every case kelve's own tests
// wrote (src/ui/gen/character.json, from kelve_core::character) and must
// give the very same SVG, character for character. No browser needed. And
// how the character on home varies how it moves, which does need one.

import { readFileSync } from 'node:fs'
import { expect, type Page, test } from '@playwright/test'
import type { AgentLook, CharacterMove, DrawnCharacter, DrawnPart } from '../../src/api/types'
import { draw, svgText } from '../../src/ui/pixels'

type Case = {
  name: string
  look: AgentLook
  parts: Record<string, DrawnPart>
  characters: Record<string, DrawnCharacter>
  move: CharacterMove
  label: string
  style: boolean
  svg: string
}

const gen = new URL('../../src/ui/gen/', import.meta.url)
const { cases } = JSON.parse(readFileSync(new URL('character.json', gen), 'utf8')) as { cases: Case[] }
/** The moves' CSS, as kelve puts it inside an SVG: without the file's opening comment and closing line break. */
const css = readFileSync(new URL('character.css', gen), 'utf8').replace(/^\/\*[\s\S]*?\*\/\n/, '').replace(/\n$/, '')

test('the app draws every character exactly as kelve does', async ({}, info) => {
  test.skip(info.project.name !== 'desktop', 'drawn the same in every browser: once is enough')
  expect(cases.length).toBeGreaterThan(40)
  for (const c of cases) {
    const drawn = svgText(draw(c.look, { parts: c.parts, characters: c.characters }, c.move, c.label), c.style ? css : undefined)
    expect(drawn, c.name).toBe(c.svg)
  }
})

/** Every class the character on home has for `minutes`, by the clock that moves it. */
async function movesOnHome(page: Page, path: string, minutes: number): Promise<string[]> {
  await page.clock.install()
  await page.goto(path)
  const char = page.locator('.hm-me svg.px')
  await expect(char).toBeVisible()
  await char.evaluate((svg) => {
    const seen = [svg.getAttribute('class') ?? '']
    Object.assign(window, { seen })
    new MutationObserver(() => seen.push(svg.getAttribute('class') ?? '')).observe(svg, { attributes: true, attributeFilter: ['class'] })
  })
  await page.clock.runFor(minutes * 60_000)
  return page.evaluate(() => (window as unknown as { seen: string[] }).seen)
}

test('on home, your agent mostly rests, and now and then does something', async ({ page }) => {
  const seen = await movesOnHome(page, '/#/example/home', 10)
  expect(seen.length).toBeGreaterThan(20)
  expect(seen.some((c) => c.includes('b-blink'))).toBe(true)
  expect(seen.some((c) => c.includes('b-look-'))).toBe(true)
  expect(seen.filter((c) => c === 'px a-still').length).toBeGreaterThan(seen.length / 3)
  expect(seen.some((c) => c.includes('a-work'))).toBe(false)
})

test('on home, your agent works in stretches, with a breath between them', async ({ page }) => {
  const seen = await movesOnHome(page, '/#/example/home?as=working', 3)
  expect(seen.filter((c) => c === 'px a-work').length).toBeGreaterThan(3)
  expect(seen.some((c) => c === 'px a-still')).toBe(true)
})

test('on home, nothing varies for anyone who asks for less motion', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' })
  expect(await movesOnHome(page, '/#/example/home', 5)).toEqual(['px a-idle'])
})
