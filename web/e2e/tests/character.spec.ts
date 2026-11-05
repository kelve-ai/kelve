// The app draws your agent's character itself, from core's parts library, as
// people choose how theirs looks: here it draws every case kelve's own tests
// wrote (src/ui/gen/character.json, from kelve_core::character) and must
// give the very same SVG, character for character. No browser needed.

import { readFileSync } from 'node:fs'
import { expect, test } from '@playwright/test'
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
