// Sam's agent, as the example shows it with no server: its character, a
// party hat it drew, what it knows, Fits you, and its settings (boards
// YourAgent, Customise and Settings).

import type { AgentSettings, CharacterView, DrawingView, DrawnPart, YourAgentView } from '../api/types'

const AT = '2026-11-02T10:00:00Z'

export const PARTY_HAT: DrawnPart = {
  name: 'Party hat',
  slot: 'hat',
  at: [9, 0],
  colours: { a: 'light', g: '#f4c84a', w: '#ffffff' },
  pixels: ['..ww..', '..gg..', '.aaaa.', 'agggga'],
}

export const EXAMPLE_CHARACTER: CharacterView = {
  name: 'Sam’s agent',
  named: false,
  look: {
    character: 'robot',
    robot: {
      parts: { head: 'round', antenna: 'ball', hat: 'none', eyes: 'big', mouth: 'smile', ears: 'bolts', body: 'box', chest: 'panel', hands: 'hands', legs: 'legs' },
      colours: { body: '#8ecae6', light: '#ff7a59', screen: '#22304a', eyes: '#9ff5d6' },
    },
    background: '#e3f2f9',
  },
  svg: '',
  made: [{ key: 'made-party', name: 'Party hat', slot: 'hat', svg: '', at: AT, part: PARTY_HAT, character: null }],
}

/** A square for each part, by letter: w works, f failed, c checking, z no difference, n not checked. */
function parts(squares: string) {
  const of = { w: 'works', f: 'failed', c: 'checking', z: 'noDifference', n: 'notChecked' } as const
  return squares.split('').map((x) => of[x as keyof typeof of])
}

export const EXAMPLE_AGENT: YourAgentView = {
  character: EXAMPLE_CHARACTER,
  knows: [
    { name: 'everywhere', title: 'Everywhere', kind: 'like', parts: parts('wwwwwzn') },
    { name: 'rust', title: 'Rust', kind: 'like', parts: parts('wcwnwf') },
    { name: 'words', title: 'Words', kind: 'like', parts: parts('wwwwwwnnn') },
    { name: 'adding-a-cli-flag', title: 'Adding a CLI flag', kind: 'do', parts: parts('wwn') },
    { name: 'fetch', title: 'fetch', kind: 'project', parts: parts('wwwnn') },
  ],
  fits: { picked: 9, of: 11, tests: [true, true, true, true, true, true, true, true, true, false, false] },
  fitsReady: 0,
  fitsOpen: true,
  scores: [],
  page: null,
}

export const EXAMPLE_SETTINGS: AgentSettings = {
  connected: [
    { agent: 'claude', name: 'Claude Code' },
    { agent: 'codex', name: 'Codex' },
  ],
  debriefs: 'worthOne',
  skillsFolder: true,
  checks: 'whenFree',
  checksAWeek: 10,
  checksThisWeek: 4,
}

export const EXAMPLE_DRAWINGS: DrawingView[] = [
  { id: 'd2', asked: 'a cat with a bell', state: 'waiting', made: null, why: null, at: AT },
  { id: 'd1', asked: 'a party hat', state: 'done', made: 'made-party', why: null, at: AT },
]
