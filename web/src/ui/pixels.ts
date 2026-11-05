// Your agent's character, drawn as kelve draws it (kelve_core::character):
// from the parts library in crates/core/src/character/parts.json, the one
// source both read, so a character looks the same here, on a public page
// and in a link's preview. The app draws it itself as people choose how
// theirs looks; e2e/tests/character.spec.ts holds it to what kelve draws.
//
// Every character is on a grid 24 pixels wide and 27 tall, in layers each
// moved its own way by gen/character.css: `base` bobs, `eyes` and `shut`
// blink, `light` glows, the arms wave, and `legs` stay still.

import LIBRARY from '../../../crates/core/src/character/parts.json' with { type: 'json' }
import type { AgentLook, CharacterColours, CharacterLayer, CharacterMove, DrawnCharacter, DrawnPart, PartSlot } from '../api/types'

export const WIDTH = 24
export const HEIGHT = 27

type Placed = { at?: [number, number]; rows?: string[] }
type PartOption = { key: string; label: string; at?: [number, number]; rows?: string[]; shut?: Placed; light?: Placed; down?: string[]; up?: string[] }
type SlotOptions = { slot: PartSlot; label: string; box: [number, number, number, number]; notes: string; options: PartOption[] }
type Ink = { name: string; colour: string; opacity?: number; flicker?: boolean }

/** Everything in parts.json. */
export type Library = {
  grid: [number, number]
  palette: Record<string, Ink>
  colours: { body: string[]; light: string[]; screen: string[]; eyes: string[]; background: string[] }
  moves: Array<{ key: CharacterMove; label: string }>
  robot: {
    label: string
    parts: AgentLook['robot']['parts']
    colours: CharacterColours
    background: string
    pleasedEyes: string
    neck: Placed
    arms: { down: Placed; up: Placed; hands: { down: [number, number]; up: [number, number] } }
    slots: SlotOptions[]
  }
  characters: Array<{ key: string; label: string; colours: CharacterColours; character: DrawnCharacter }>
}

export const library = LIBRARY as unknown as Library

/** The layers, in the order kelve keeps them. */
const LAYERS: CharacterLayer[] = ['base', 'eyes', 'shut', 'light', 'arm-left-down', 'arm-left-up', 'arm-right-down', 'arm-right-up', 'legs']
const CLASSES: Record<CharacterLayer, string> = {
  base: 'base',
  eyes: 'eyes eo',
  shut: 'shut ec',
  light: 'light',
  'arm-left-down': 'arm-left-down al-d',
  'arm-left-up': 'arm-left-up al-u',
  'arm-right-down': 'arm-right-down ar-d',
  'arm-right-up': 'arm-right-up ar-u',
  legs: 'legs',
}
const ROLES = ['body', 'light', 'screen', 'eyes'] as const
type Role = (typeof ROLES)[number]

/** A colour as `#rrggbb`, lower case, if it is one. */
function hex(text: string): string | null {
  const h = text.trim().replace(/^#/, '')
  return text.trim().startsWith('#') && /^[0-9a-fA-F]{6}$/.test(h) ? `#${h.toLowerCase()}` : null
}

/** A colour as a drawing writes it: a role, or `#rrggbb`. */
function colourOf(text: string): string | null {
  const t = text.trim()
  return (ROLES as readonly string[]).includes(t) ? t : hex(t)
}

/** What a pixel is painted with. */
type Paint = { colour: string; opacity?: number; flicker: boolean }
const same = (a: Paint, b: Paint) => a.colour === b.colour && a.opacity === b.opacity && a.flicker === b.flicker

/** The colour a role has: the one chosen, if it's a colour, else the robot's own. */
function colourFor(colours: CharacterColours, role: Role): string {
  return hex(colours[role]) ?? library.robot.colours[role]
}

/** A character's pixels, layer by layer. */
class Picture {
  layers: Array<Array<Paint | null>> = LAYERS.map(() => new Array<Paint | null>(WIDTH * HEIGHT).fill(null))

  put(layer: CharacterLayer, at: [number, number], rows: string[], ink: (letter: string) => Paint | null, mirror = false) {
    const grid = this.layers[LAYERS.indexOf(layer)]
    rows.forEach((row, dy) => {
      Array.from(row).forEach((letter, dx) => {
        if (letter === '.') return
        let x = at[0] + dx
        if (mirror) x = WIDTH - 1 - x
        const y = at[1] + dy
        if (x < 0 || x >= WIDTH || y < 0 || y >= HEIGHT) return
        const paint = ink(letter)
        if (paint) grid[y * WIDTH + x] = paint
      })
    })
  }

  leftmost(layer: CharacterLayer): number | null {
    let min: number | null = null
    this.layers[LAYERS.indexOf(layer)].forEach((p, i) => {
      if (p && (min === null || i % WIDTH < min)) min = i % WIDTH
    })
    return min
  }
}

/** kelve's own letters. */
function palette(letter: string): Paint | null {
  const ink = library.palette[letter]
  if (!ink) return null
  const colour = colourOf(ink.colour)
  return colour ? { colour, opacity: ink.opacity, flicker: !!ink.flicker } : null
}

/** A drawing's own letters: plain colours. */
function inkOf(colours: Record<string, string>) {
  return (letter: string): Paint | null => {
    const c = colours[letter]
    const colour = c === undefined ? null : colourOf(c)
    return colour ? { colour, flicker: false } : null
  }
}

type Source = { own: PartOption } | { made: DrawnPart }

const optionOf = (slot: PartSlot, key: string) => library.robot.slots.find((s) => s.slot === slot)?.options.find((o) => o.key === key)

function place(picture: Picture, layer: CharacterLayer, source: Source, mirror = false) {
  if ('own' in source) picture.put(layer, source.own.at ?? [0, 0], source.own.rows ?? [], palette, mirror)
  else picture.put(layer, source.made.at, source.made.pixels, inkOf(source.made.colours), mirror)
}

/** The robot's picture: `made` holds the parts the agent drew, by key. While pleased, its eyes are happy. */
function robotPicture(parts: AgentLook['robot']['parts'], made: Record<string, DrawnPart>, pleased: boolean): Picture {
  const source = (slot: PartSlot, key: string): Source | null => {
    const own = optionOf(slot, key)
    if (own) return { own }
    const m = made[key]
    if (m && m.slot === slot) return { made: m }
    const fallback = optionOf(slot, library.robot.parts[slot])
    return fallback ? { own: fallback } : null
  }
  const picture = new Picture()
  const ears = source('ears', parts.ears)
  if (ears) for (const mirror of [false, true]) place(picture, 'base', ears, mirror)
  const hatted = parts.hat !== 'none' && parts.hat !== ''
  const antenna = hatted ? null : source('antenna', parts.antenna)
  if (antenna) {
    place(picture, 'base', antenna)
    if ('own' in antenna) {
      if (antenna.own.light) picture.put('light', antenna.own.light.at ?? [0, 0], antenna.own.light.rows ?? [], palette)
    } else picture.put('light', antenna.made.at, antenna.made.light ?? [], inkOf(antenna.made.colours))
  }
  const head = source('head', parts.head)
  if (head) place(picture, 'base', head)
  const eyes = source('eyes', pleased ? library.robot.pleasedEyes : parts.eyes)
  if (eyes) {
    place(picture, 'eyes', eyes)
    if ('own' in eyes) {
      if (eyes.own.shut) picture.put('shut', eyes.own.shut.at ?? [0, 0], eyes.own.shut.rows ?? [], palette)
    } else picture.put('shut', eyes.made.at, eyes.made.shut ?? [], inkOf(eyes.made.colours))
  }
  for (const slot of ['mouth', 'hat'] as const) {
    const s = source(slot, parts[slot])
    if (s) place(picture, 'base', s)
  }
  picture.put('base', library.robot.neck.at ?? [0, 0], library.robot.neck.rows ?? [], palette)
  const body = new Picture()
  const b = source('body', parts.body)
  if (b) {
    place(picture, 'base', b)
    place(body, 'base', b)
  }
  const chest = source('chest', parts.chest)
  if (chest) place(picture, 'base', chest)

  // The arms hang from the body's sides, and the hands from the arms.
  const left = body.leftmost('base') ?? 6
  const arms = library.robot.arms
  const hand = source('hands', parts.hands)
  const sides: Array<[boolean, CharacterLayer, CharacterLayer]> = [
    [false, 'arm-left-down', 'arm-left-up'],
    [true, 'arm-right-down', 'arm-right-up'],
  ]
  for (const [mirror, down, up] of sides) {
    const at = (p: [number, number]): [number, number] => [left + p[0], p[1]]
    picture.put(down, at(arms.down.at ?? [0, 0]), arms.down.rows ?? [], palette, mirror)
    picture.put(up, at(arms.up.at ?? [0, 0]), arms.up.rows ?? [], palette, mirror)
    if (hand && 'own' in hand) {
      picture.put(down, at(arms.hands.down), hand.own.down ?? [], palette, mirror)
      picture.put(up, at(arms.hands.up), hand.own.up ?? [], palette, mirror)
    } else if (hand) {
      const ink = inkOf(hand.made.colours)
      picture.put(down, at(arms.hands.down), hand.made.pixels, ink, mirror)
      picture.put(up, at(arms.hands.up), [...hand.made.pixels].reverse(), ink, mirror)
    }
  }
  const legs = source('legs', parts.legs)
  if (legs) place(picture, 'legs', legs)
  return picture
}

function characterPicture(c: DrawnCharacter): Picture {
  const picture = new Picture()
  const ink = inkOf(c.colours)
  for (const layer of LAYERS) {
    const rows = c.layers[layer]
    if (rows) picture.put(layer, [0, 0], rows, ink)
  }
  return picture
}

/** What the agent drew, by key, as a look reads them. */
export type Made = { parts: Record<string, DrawnPart>; characters: Record<string, DrawnCharacter> }
export const NOTHING_MADE: Made = { parts: {}, characters: {} }

function lookPicture(look: AgentLook, made: Made, move: CharacterMove): Picture {
  if (look.character !== 'robot') {
    const own = library.characters.find((c) => c.key === look.character)
    if (own) return characterPicture(own.character)
    const drawn = made.characters[look.character]
    if (drawn) return characterPicture(drawn)
  }
  return robotPicture(look.robot.parts, made.parts, move === 'happy')
}

/** One colour's runs in a layer: a path. */
export type Run = { d: string; fill: string; opacity?: number; flame: boolean }
/** A layer as drawn: its classes, and a path for each paint. */
export type Group = { classes: string; paths: Run[] }
/** A character, ready to draw: its legs, and the layers above them that bob. */
export type Drawn = { move: CharacterMove; label: string; legs: Group | null; up: Group[] }

function group(picture: Picture, layer: CharacterLayer, colours: CharacterColours): Group | null {
  const grid = picture.layers[LAYERS.indexOf(layer)]
  const paints: Array<[Paint, string]> = []
  for (let y = 0; y < HEIGHT; y++) {
    let x = 0
    while (x < WIDTH) {
      const paint = grid[y * WIDTH + x]
      if (!paint) {
        x++
        continue
      }
      let end = x
      while (end + 1 < WIDTH && grid[y * WIDTH + end + 1] && same(grid[y * WIDTH + end + 1] as Paint, paint)) end++
      const n = end - x + 1
      const run = `M${x} ${y}h${n}v1h-${n}z`
      const found = paints.find(([p]) => same(p, paint))
      if (found) found[1] += run
      else paints.push([paint, run])
      x = end + 1
    }
  }
  if (paints.length === 0) return null
  return {
    classes: CLASSES[layer],
    paths: paints.map(([p, d]) => ({
      d,
      fill: (ROLES as readonly string[]).includes(p.colour) ? colourFor(colours, p.colour as Role) : p.colour,
      opacity: p.opacity,
      flame: p.flicker,
    })),
  }
}

/** A look, drawn: what both the SVG's text and the React component are made from. */
export function draw(look: AgentLook, made: Made, move: CharacterMove, label: string): Drawn {
  const picture = lookPicture(look, made, move)
  const colours = look.robot.colours
  const up = (['base', 'eyes', 'shut', 'light', 'arm-left-down', 'arm-left-up', 'arm-right-down', 'arm-right-up'] as CharacterLayer[])
    .map((l) => group(picture, l, colours))
    .filter((g): g is Group => g !== null)
  return { move, label, legs: group(picture, 'legs', colours), up }
}

/** The floor's shadow and the sleeping z, as kelve draws them. */
export const SHADOW = 'M6 26h12v1h-12z'
export const ZZ = 'M19 1h3v1h-3zM21 2h1v1h-1zM20 3h1v1h-1zM19 4h3v1h-3z'

/** Text made safe inside an attribute. */
const escape = (text: string) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;')

/** A drawing as kelve writes its SVG, character for character, with `css` inside it when given. */
export function svgText(d: Drawn, css?: string): string {
  const g = (x: Group | null) =>
    x
      ? `<g class="${x.classes}">${x.paths.map((p) => `<path${p.flame ? ' class="flame"' : ''} d="${p.d}" fill="${p.fill}"${p.opacity === undefined ? '' : ` opacity="${p.opacity}"`}/>`).join('')}</g>`
      : ''
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" class="px a-${d.move}" viewBox="0 0 ${WIDTH} ${HEIGHT}" shape-rendering="crispEdges" role="img" aria-label="${escape(d.label)}">` +
    (css === undefined ? '' : `<style>${css}</style>`) +
    `<path d="${SHADOW}" fill="#1d1c1a" opacity=".1"/><g class="all">` +
    g(d.legs) +
    `<g class="up">${d.up.map(g).join('')}</g>` +
    `<path class="zz" d="${ZZ}" fill="#8a847b"/></g></svg>`
  )
}

/** The robot as kelve ships it. */
export function defaultLook(): AgentLook {
  return { character: 'robot', robot: { parts: { ...library.robot.parts }, colours: { ...library.robot.colours } }, background: library.robot.background }
}

/** What the agent made, as a look reads it, from what the API says it made. */
export function madeOf(made: Array<{ key: string; part: DrawnPart | null; character: DrawnCharacter | null }>): Made {
  const out: Made = { parts: {}, characters: {} }
  for (const m of made) {
    if (m.part) out.parts[m.key] = m.part
    if (m.character) out.characters[m.key] = m.character
  }
  return out
}
