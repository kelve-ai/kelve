// Your agent's character, standing on the page, never in a box. Every
// character is drawn on the same grid, 24 pixels wide and 27 tall, and
// moves the same way (pixels.ts, and gen/character.css, which kelve's own
// tests write). `move` is how it moves: waiting (`idle`), working, pleased
// (`happy`), hello (`wave`), asleep, or `still`; each stops for anyone whose
// computer asks for less motion. `lively` varies it over time (lively.ts), for
// where it's left on the page. With no `look`, it's the person's own agent,
// as they made it.

import { useMemo } from 'react'
import type { AgentLook, CharacterMove, CharacterView } from '../api/types'
import { EXAMPLE_CHARACTER } from '../example/agent'
import { useLoad } from '../useLoad'
import { useLively } from './lively'
import { defaultLook, draw, type Group, type Made, madeOf, NOTHING_MADE, SHADOW, ZZ } from './pixels'
import './gen/character.css'

export type Move = CharacterMove

/** Whether the page is the app's example, which answers with no server. */
const inExample = () => location.hash.startsWith('#/example')

/** The person's own agent: as they made it, or in the example, Sam's. */
export function useMyCharacter(skip = false): CharacterView | undefined {
  const example = inExample()
  const loaded = useLoad<CharacterView>(skip || example ? null : '/api/character', ['settings'])
  if (skip) return undefined
  return example ? EXAMPLE_CHARACTER : loaded.data
}

function Layer({ g }: { g: Group | null }) {
  if (!g) return null
  return (
    <g className={g.classes}>
      {g.paths.map((p) => <path key={p.d} className={p.flame ? 'flame' : undefined} d={p.d} fill={p.fill} opacity={p.opacity} />)}
    </g>
  )
}

export function Character({ size = 120, move = 'idle', lively = false, label, look, made }: {
  /** Its width in pixels on the page; its height follows the grid. */
  size?: number
  move?: Move
  /** Whether it varies how it moves as it's left on the page, rather than looping one move. */
  lively?: boolean
  /** What it's called, for people who can't see it: its name, unless said. */
  label?: string
  /** How it looks, when it isn't the person's own agent as kept: a choice being tried. */
  look?: AgentLook
  /** What the agent drew, that the look may wear. */
  made?: Made
}) {
  const mine = useMyCharacter(!!look && !!made && !!label)
  const l = look ?? mine?.look ?? defaultLook()
  const m = made ?? (mine ? madeOf(mine.made) : NOTHING_MADE)
  const name = label ?? mine?.name ?? 'Your agent'
  const now = useLively(move, lively)
  const d = useMemo(() => draw(l, m, now.move, name), [l, m, now.move, name])
  return (
    <span className="char" style={{ width: size }}>
      <svg className={`px a-${d.move}${now.beat ? ` b-${now.beat}` : ''}`} viewBox="0 0 24 27" width={size} height={(size * 27) / 24} shapeRendering="crispEdges" role="img" aria-label={d.label}>
        <path d={SHADOW} fill="#1d1c1a" opacity=".1" />
        <g className="all">
          <Layer g={d.legs} />
          <g className="up">
            {d.up.map((g) => <Layer key={g.classes} g={g} />)}
          </g>
          <path className="zz" d={ZZ} fill="#8a847b" />
        </g>
      </svg>
    </span>
  )
}
