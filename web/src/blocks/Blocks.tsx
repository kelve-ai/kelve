// The still blocks an entry is built from: excerpts of the real thing,
// bars, tables, two sides compared, squares, a timeline, a conversation and
// the answer card. Each is drawn by kelve from data; none holds a link.

import type { Body } from '../api/types'
import { useEffect, useState } from 'react'
import { Rich } from '../rich'

type Of<T extends Body['type']> = Extract<Body, { type: T }>

/**
 * Lines of the real thing, numbered as they are there, the one that matters
 * in red: code coloured in its language, what someone said as prose, and
 * anything else plain.
 */
export function Excerpt({ body, pins, lit }: {
  body: Of<'excerpt'>
  /** Numbered notes pinned to lines, by where each line is. */
  pins?: Map<number, number>
  /** The line whose note is being read. */
  lit?: number | null
}) {
  const prose = body.lang === 'prose'
  const lang = body.lang
  // The highlighter loads the first time an entry quotes code; until then,
  // and for anything that isn't code, the lines are plain.
  const [coloured, setColoured] = useState<string[] | null>(null)
  useEffect(() => {
    if (!lang || prose) return
    let live = true
    import('./highlight').then(({ colours, highlight }) => {
      if (live && colours(lang)) setColoured(highlight(body.lines.map((l) => l.text), lang))
    })
    return () => { live = false }
  }, [body.lines, lang, prose])
  return (
    <div className={`code${prose ? ' prose' : ''}`} role="figure" aria-label={body.from}>
      {body.from && <p className="file">{body.from}</p>}
      {body.lines.map((l, j) => {
        const text = coloured
          ? <span dangerouslySetInnerHTML={{ __html: coloured[j] || ' ' }} />
          : l.text || ' '
        return (
          <div key={j} className={`ln${l.n === null ? ' plain' : ''}${lit === j ? ' lit' : ''}`}>
            {l.n !== null && <span className="n" style={l.mark ? { color: 'var(--red)' } : undefined}>{l.n}</span>}
            <span className={l.dim ? 'dim' : undefined}>
              {l.mark ? <span className="mark">{text}</span> : text}
              {pins?.has(j) && <b className="pin" aria-label={`note ${pins.get(j)}`}>{pins.get(j)}</b>}
            </span>
          </div>
        )
      })}
    </div>
  )
}

export function Bars({ body }: { body: Of<'bars'> }) {
  return (
    <div>
      <div className="bars" role="img" aria-label={body.rows.map((r) => `${r.label}: ${r.display}`).join('; ')}>
        {body.rows.flatMap((r, j) => [
          <span key={`l${j}`} style={{ color: r.mark || !r.quiet ? 'var(--ink)' : 'var(--soft)' }}>{r.label}</span>,
          <div key={`b${j}`}><div className="bar" style={{ width: `${Math.min(100, Math.max(0, r.value))}%`, background: r.mark ? 'var(--red)' : r.quiet ? 'var(--mute)' : 'var(--ink-2)' }} /></div>,
          <span key={`v${j}`} style={{ color: 'var(--soft)', textAlign: 'right', fontVariantNumeric: 'tabular-nums' }}>{r.display}</span>,
        ])}
      </div>
      {body.caption && <p className="small" style={{ margin: '0 0 22px', color: 'var(--soft)', fontSize: 14 }}>{body.caption}</p>}
    </div>
  )
}

export function Table({ body }: { body: Of<'table'> }) {
  const cols = body.head.length
  const wide = cols > 3
  return (
    <div className="table" role="table" style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}>
      {body.head.map((h, j) => <div key={`h${j}`} role="columnheader" className="th">{h}</div>)}
      {body.rows.flatMap((r, ri) => r.map((c, ci) => (
        <div key={`${ri}-${ci}`} role="cell" className={`td${body.marks.some(([mr, mc]) => mr === ri && mc === ci) ? ' red' : ''}`} style={ci === 0 && wide ? { color: 'var(--ink)' } : undefined}>{c}</div>
      )))}
    </div>
  )
}

/** Two approaches, or two options, side by side. */
export function Compare({ body }: { body: Of<'compare'> }) {
  return (
    <div className="compare">
      {[body.left, body.right].map((side, j) => (
        <div key={j}>
          <p className="side">{side.title}</p>
          <ul className="sub">
            {side.points.map((p, k) => <li key={k}><Rich text={p} /></li>)}
          </ul>
        </div>
      ))}
    </div>
  )
}

export function Squares({ body }: { body: Of<'squares'> }) {
  return (
    <div className="squares">
      {body.rows.flatMap((r, j) => [
        <i key={`i${j}`} aria-hidden="true" style={{ background: r.red ? 'var(--red)' : 'var(--ink-2)' }} />,
        <span key={`s${j}`} style={{ color: 'var(--text)' }}><Rich text={r.text} /></span>,
      ])}
    </div>
  )
}

export function Timeline({ body }: { body: Of<'timeline'> }) {
  return (
    <div className="timeline" role="img" aria-label={body.rows.map((r) => `${r.when}: ${r.text}`).join(' ')}>
      {body.rows.map((r, j) => (
        <div key={j} className="t">
          <span>{r.when}</span>
          <i style={r.mark !== 'none' ? { background: r.mark === 'red' ? 'var(--red)' : 'var(--ink-2)' } : undefined} />
          <span>{r.text}</span>
        </div>
      ))}
    </div>
  )
}

/** Whoever you'd have to convince, and what they'd say. */
export function Conversation({ body }: { body: Of<'conversation'> }) {
  return (
    <div className="talk">
      {body.lines.map((l, j) => (
        <div key={j} className="said">
          <span className="who">{l.who}</span>
          <span><Rich text={l.text} /></span>
        </div>
      ))}
    </div>
  )
}

/** "So if anyone asks…": one sentence you can use. */
export function AnswerCard({ body }: { body: Of<'answer'> }) {
  return <p className="asked">{body.lead} <span className="key"><Rich text={body.text} /></span></p>
}
