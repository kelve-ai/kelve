// The margin: beside each idea's first line, its square, its name, and one line
// on how this work uses it, as your agent said it. Notes that would overlap
// move down; they never cover each other. Clicking one opens its entry beside
// the work.

import { type ReactNode, useLayoutEffect, useRef } from 'react'
import type { Marked } from '../api/types'
import { Square } from './Square'

/** The margin's line: plain text, with `code` allowed and nothing else. */
export function UseLine({ text }: { text: string }) {
  const parts = text.split(/(`[^`]+`)/g)
  return (
    <>
      {parts.map((p, i): ReactNode =>
        p.startsWith('`') && p.endsWith('`') && p.length > 1 ? <code key={i}>{p.slice(1, -1)}</code> : p,
      )}
    </>
  )
}

export const ideaName = (m: Marked) => m.mark.idea?.name ?? m.mark.name

/** One idea in the margin: a button that opens its entry. */
export function MarginNote({ mark, on, open, style }: {
  mark: Marked
  /** Its entry is open: its name goes ink, its lines are lit. */
  on: boolean
  open: (m: Marked) => void
  style?: React.CSSProperties
}) {
  const name = ideaName(mark)
  return (
    <button type="button" className={`mnote${on ? ' on' : ''}`} style={style} data-key={mark.key} onClick={() => open(mark)}>
      <b><Square ideaKey={mark.key} name={name} />{name}</b>
      {mark.mark.use ? <span><UseLine text={mark.mark.use} /></span> : <span className="def">{mark.mark.definition}</span>}
    </button>
  )
}

/** Where each idea wants to sit, from the top of the file. */
export type Want = { mark: Marked; top: number }

/**
 * The margin beside a file: each note at its line, or below the one above
 * it, when they'd overlap.
 */
export function Margin({ wants, lit, open, drop = 0 }: {
  wants: Want[]
  lit?: string
  open: (m: Marked) => void
  /** How far below its line each note sits: a line's height when they float over the code. */
  drop?: number
}) {
  const host = useRef<HTMLDivElement>(null)
  // Placed after they're drawn, by their heights.
  useLayoutEffect(() => {
    const el = host.current
    if (!el) return
    let floor = -Infinity
    const notes = [...el.querySelectorAll<HTMLElement>('.mnote')]
    notes.forEach((n, i) => {
      const top = Math.max((wants[i]?.top ?? 0) + drop, floor)
      n.style.top = `${top}px`
      floor = top + n.offsetHeight + 10
    })
  })
  return (
    <div className="margin" ref={host}>
      {wants.map((w) => <MarginNote key={w.mark.key} mark={w.mark} on={w.mark.key === lit} open={open} style={{ top: w.top }} />)}
    </div>
  )
}
