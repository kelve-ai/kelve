// The figures that move: a sequence, steps to walk through, a quadrant, a
// spectrum, a trend, a tree, a before and after, a number, an annotated
// excerpt and a funnel. Each is drawn by kelve from data, in the page's
// paper, ink and red, and plays once when you reach it: never again, and
// never for anyone who'd rather things stayed still.

import { useEffect, useRef, useState, type CSSProperties, type ReactNode, type RefObject } from 'react'
import type { Body, TreeBranch } from '../api/types'
import { Rich } from '../rich'
import { stillPlease } from '../words'
import { Excerpt } from './Blocks'

type Of<T extends Body['type']> = Extract<Body, { type: T }>

/** After a figure comes into view, a moment for the entry to write itself in. */
const SETTLE = 500

/**
 * Whether a figure has been reached: half of it on screen, a moment ago.
 * Straight away for an entry already read, or anyone who'd rather things
 * stayed still.
 */
export function useReached(ref: RefObject<HTMLElement | null>, still: boolean): boolean {
  const [reached, setReached] = useState(() => still || stillPlease())
  useEffect(() => {
    const el = ref.current
    if (reached || !el) return
    let timer: number | undefined
    const io = new IntersectionObserver((xs) => {
      if (!xs.some((x) => x.isIntersecting)) return
      io.disconnect()
      timer = window.setTimeout(() => setReached(true), SETTLE)
    }, { threshold: 0.5 })
    io.observe(el)
    return () => { io.disconnect(); window.clearTimeout(timer) }
  }, [reached, ref])
  return reached
}

/** How long after the first each of a figure's parts comes in. */
const turn = (i: number, every = 0.45): CSSProperties => ({ ['--t' as string]: `${i * every}s` })

/** Who says what to whom, in order: an arrow for each message, drawn in turn. */
export function Sequence({ id, body, still }: { id: string; body: Of<'sequence'>; still: boolean }) {
  const ref = useRef<HTMLDivElement>(null)
  const reached = useReached(ref, still)
  const n = body.actors.length
  const centre = (actor: string) => ((body.actors.indexOf(actor) + 0.5) / n) * 100
  return (
    <div ref={ref} className={`fig seq${reached ? ' on' : ''}`}>
      <div role="img" aria-labelledby={`${id}-describe`}>
        <div className="seq-actors" style={{ gridTemplateColumns: `repeat(${n}, minmax(0, 1fr))` }}>
          {body.actors.map((a) => <span key={a}><i />{a}</span>)}
        </div>
        <div className="seq-body">
          {body.actors.map((a) => <span key={a} className="seq-life" style={{ left: `${centre(a)}%` }} />)}
          {body.messages.map((m, i) => {
            const from = centre(m.from)
            const to = centre(m.to)
            const left = Math.min(from, to)
            return (
              <div key={i} className={`seq-msg${m.mark ? ' mark' : ''}${to < from ? ' back' : ''}`} style={turn(i, 0.7)}>
                <span className="seq-text" style={{ marginLeft: `${left}%`, width: `${Math.abs(to - from)}%` }}>{m.text}</span>
                <span className="seq-arrow" style={{ marginLeft: `${left}%`, width: `${Math.abs(to - from)}%` }} />
              </div>
            )
          })}
        </div>
      </div>
      <p id={`${id}-describe`} className="describe">{body.describe}</p>
    </div>
  )
}

/** A process, a stage at a time, at your own pace. */
export function Steps({ body }: { body: Of<'steps'> }) {
  const [at, setAt] = useState(0)
  const last = body.cards.length - 1
  const card = body.cards[at]
  return (
    <div className="fig steps">
      <div className="steps-dots" role="tablist" aria-label="Stages">
        {body.cards.map((c, i) => (
          <button key={i} role="tab" aria-selected={i === at} aria-label={`${i + 1}. ${c.title}`}
            className={i === at ? 'on' : i < at ? 'done' : undefined} onClick={() => setAt(i)} />
        ))}
        <span>{at + 1} of {body.cards.length}</span>
      </div>
      <div key={at} className="steps-card fade-in" aria-live="polite">
        <p className="steps-title">{card.title}</p>
        <p className="p"><Rich text={card.text} /></p>
      </div>
      <div className="steps-go">
        {at > 0 && <button className="word soft" onClick={() => setAt(at - 1)}>Back</button>}
        {at < last && <button className="word" onClick={() => setAt(at + 1)}>Next: {body.cards[at + 1].title}</button>}
        {at === last && last > 0 && <button className="word soft" onClick={() => setAt(0)}>From the start</button>}
      </div>
    </div>
  )
}

/** Things placed on two axes. */
export function Quadrant({ id, body, still }: { id: string; body: Of<'quadrant'>; still: boolean }) {
  const ref = useRef<HTMLDivElement>(null)
  const reached = useReached(ref, still)
  return (
    <div ref={ref} className={`fig quad${reached ? ' on' : ''}`}>
      <div role="img" aria-labelledby={`${id}-describe`}>
        <span className="quad-y">{body.y.high} ↑</span>
        <div className="quad-plot">
          <span className="quad-h" />
          <span className="quad-v" />
          {body.points.map((p, i) => (
            <span key={i} className={`quad-pt${p.mark ? ' mark' : ''}${p.x > 62 ? ' left' : ''}`}
              style={{ left: `${p.x}%`, bottom: `${p.y}%`, ...turn(i, 0.3) }}>
              <i />{p.label}
            </span>
          ))}
        </div>
        <div className="quad-x"><span>← {body.x.low}</span><span>{body.x.high} →</span></div>
        <span className="quad-y">{body.y.low} ↓</span>
      </div>
      <p id={`${id}-describe`} className="describe">{body.describe}</p>
    </div>
  )
}

/** Where things sit between two ends: each slides to its place. */
export function Spectrum({ id, body, still }: { id: string; body: Of<'spectrum'>; still: boolean }) {
  const ref = useRef<HTMLDivElement>(null)
  const reached = useReached(ref, still)
  const points = [...body.points].sort((a, b) => a.at - b.at)
  return (
    <div ref={ref} className={`fig spec${reached ? ' on' : ''}`}>
      <div role="img" aria-labelledby={`${id}-describe`}>
        <div className="spec-ends"><span>← {body.low}</span><span>{body.high} →</span></div>
        <div className="spec-line">
          {points.map((p, i) => (
            <span key={i} className={`spec-pt${p.mark ? ' mark' : ''}${i % 2 ? ' low' : ''}${p.at > 75 ? ' end' : p.at < 25 ? ' start' : ''}`}
              style={{ ['--at' as string]: `${p.at}%`, ...turn(i, 0.25) }}>
              <i />
              <span className="spec-label">{p.label}{p.note && <small>{p.note}</small>}</span>
            </span>
          ))}
        </div>
      </div>
      <p id={`${id}-describe`} className="describe">{body.describe}</p>
    </div>
  )
}

/** A number as people write it, with the unit after it. */
function shown(v: number, unit: string | null): string {
  const n = Math.abs(v) >= 100 ? Math.round(v).toLocaleString('en-GB') : String(Math.round(v * 10) / 10)
  if (!unit) return n
  return unit === '%' ? `${n}%` : /^[£$€]$/.test(unit) ? `${unit}${n}` : `${n} ${unit}`
}

/** A line or two over time, drawn as you reach it. */
export function Trend({ id, body, still }: { id: string; body: Of<'trend'>; still: boolean }) {
  const ref = useRef<HTMLDivElement>(null)
  const reached = useReached(ref, still)
  const W = 600
  const H = 220
  const pad = { l: 8, r: 8, t: 18, b: 8 }
  const values = body.series.flatMap((s) => s.values)
  const hi = Math.max(...values)
  const lo0 = Math.min(...values)
  // From nought, unless the values all sit far above it.
  const lo = lo0 >= 0 && lo0 < hi * 0.6 ? 0 : lo0 - (hi - lo0) * 0.15
  const span = hi - lo || 1
  const x = (i: number) => pad.l + (i / Math.max(1, body.labels.length - 1)) * (W - pad.l - pad.r)
  const y = (v: number) => pad.t + (1 - (v - lo) / span) * (H - pad.t - pad.b)
  const every = Math.ceil(body.labels.length / 6)
  return (
    <div ref={ref} className={`fig trend${reached ? ' on' : ''}`}>
      <div role="img" aria-labelledby={`${id}-describe`}>
        <div className="trend-keys">
          {body.series.map((s, i) => (
            <span key={i} className={s.mark ? 'mark' : undefined}>
              <i />{s.label} <b>{shown(s.values[s.values.length - 1] ?? 0, body.unit)}</b>
            </span>
          ))}
        </div>
        <div className="trend-plot">
        <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" aria-hidden="true">
          <line className="trend-base" x1={0} x2={W} y1={y(lo)} y2={y(lo)} vectorEffect="non-scaling-stroke" />
          <line className="trend-top" x1={0} x2={W} y1={y(hi)} y2={y(hi)} vectorEffect="non-scaling-stroke" />
          {body.series.map((s, i) => (
            <polyline key={i} className={`trend-line${s.mark ? ' mark' : ''}`} vectorEffect="non-scaling-stroke"
              points={s.values.map((v, j) => `${x(j)},${y(v)}`).join(' ')} />
          ))}
        </svg>
        <span className="trend-y top">{shown(hi, body.unit)}</span>
        <span className="trend-y bottom">{shown(lo, body.unit)}</span>
        </div>
        <div className="trend-x">
          {body.labels.map((l, i) => (
            <span key={i} style={{ left: `${(x(i) / W) * 100}%` }} className={i === 0 ? 'first' : i === body.labels.length - 1 ? 'last' : undefined}>
              {i % every === 0 || i === body.labels.length - 1 ? l : ''}
            </span>
          ))}
        </div>
      </div>
      <p id={`${id}-describe`} className="describe">{body.describe}</p>
    </div>
  )
}

/** A decision that branches, from one question at the top, each branch growing in turn. */
export function Tree({ id, body, still }: { id: string; body: Of<'tree'>; still: boolean }) {
  const ref = useRef<HTMLDivElement>(null)
  const reached = useReached(ref, still)
  const top = body.branches.find((b) => !b.parent)
  const children = (b: TreeBranch) => body.branches.filter((c) => c.parent === b.id)
  let order = 0
  const grow = (b: TreeBranch, depth: number, seen: Set<string>): ReactNode => {
    if (seen.has(b.id)) return null
    seen.add(b.id)
    const kids = children(b)
    return (
      <li key={b.id} className={b.mark ? 'mark' : undefined} style={turn(order++, 0.3)}>
        <span className="tree-node">
          {b.edge && <em>{b.edge}</em>}
          <i />
          <span>{b.label}</span>
        </span>
        {kids.length > 0 && depth < 6 && <ul>{kids.map((k) => grow(k, depth + 1, seen))}</ul>}
      </li>
    )
  }
  return (
    <div ref={ref} className={`fig tree${reached ? ' on' : ''}`}>
      <div role="img" aria-labelledby={`${id}-describe`}>
        {top && <ul className="tree-top">{grow(top, 0, new Set())}</ul>}
      </div>
      <p id={`${id}-describe`} className="describe">{body.describe}</p>
    </div>
  )
}

/** Lines, coloured in their language once the highlighter's loaded. */
function useColoured(lines: string[], lang: string | null): string[] | null {
  const [coloured, setColoured] = useState<string[] | null>(null)
  const key = lines.join('\n')
  useEffect(() => {
    if (!lang) return
    let live = true
    import('./highlight').then(({ colours, highlight }) => {
      if (live && colours(lang)) setColoured(highlight(key.split('\n'), lang))
    })
    return () => { live = false }
  }, [key, lang])
  return coloured
}

function Pane({ v, hidden }: { v: Of<'beforeAfter'>['before']; hidden?: boolean }) {
  const coloured = useColoured(v.lines, v.lang)
  return (
    <div className="ba-pane" aria-hidden={hidden || undefined}>
      {v.lines.map((l, j) => (
        <div key={j}>{coloured ? <span dangerouslySetInnerHTML={{ __html: coloured[j] || ' ' }} /> : l || ' '}</div>
      ))}
    </div>
  )
}

/**
 * Two versions of a thing: once reached, the after sweeps in over the
 * before from the left; drag the red edge back to compare.
 */
export function BeforeAfter({ id, body, still }: { id: string; body: Of<'beforeAfter'>; still: boolean }) {
  const ref = useRef<HTMLDivElement>(null)
  const reached = useReached(ref, still)
  // How much of the after shows: none until it's reached, then all of it.
  const [shownAfter, setShownAfter] = useState<number | null>(null)
  const at = shownAfter ?? (reached ? 100 : 0)
  return (
    <div ref={ref} className={`fig ba${shownAfter === null ? ' sweep' : ''}`}>
      <div className="ba-labels">
        <span>{body.before.label}{body.before.from && <small>{body.before.from}</small>}</span>
        <span>{body.after.label}{body.after.from && <small>{body.after.from}</small>}</span>
      </div>
      <div className="ba-stack code">
        <Pane v={body.before} />
        <div className="ba-after" style={{ clipPath: `inset(0 ${100 - at}% 0 0)` }}>
          <Pane v={body.after} />
        </div>
        <span className="ba-edge" style={{ left: `${at}%` }} aria-hidden="true" />
      </div>
      <label htmlFor={`${id}-drag`} className="sr-only">How much of {body.after.label} to show</label>
      <input id={`${id}-drag`} type="range" className="range" min={0} max={100} step={1} value={at}
        style={{ ['--p' as string]: `${at}%` }} onChange={(e) => setShownAfter(Number(e.target.value))} />
      <p className="describe">{body.describe}</p>
    </div>
  )
}

/** One number, what it means, and how much of the whole it is. */
export function NumberFigure({ body, still }: { body: Of<'number'>; still: boolean }) {
  const ref = useRef<HTMLDivElement>(null)
  const reached = useReached(ref, still)
  return (
    <div ref={ref} className={`fig num${reached ? ' on' : ''}`}>
      <p className="num-line"><b>{body.figure}</b> <span><Rich text={body.text} /></span></p>
      {body.share !== null && (
        <div className="num-bar" aria-hidden="true"><i style={{ width: `${Math.min(100, Math.max(0, body.share))}%` }} /></div>
      )}
    </div>
  )
}

/** Lines of the real thing, with numbered notes on the lines that matter. */
export function Annotated({ body, still }: { body: Of<'annotated'>; still: boolean }) {
  const ref = useRef<HTMLDivElement>(null)
  const reached = useReached(ref, still)
  const [lit, setLit] = useState<number | null>(null)
  const pins = new Map(body.notes.map((n, i) => [n.line, i + 1]))
  return (
    <div ref={ref} className={`fig ann${reached ? ' on' : ''}`}>
      <Excerpt body={{ type: 'excerpt', from: body.from, lang: body.lang, lines: body.lines }} pins={pins} lit={lit} />
      <ol className="ann-notes">
        {body.notes.map((n, i) => (
          <li key={i} style={turn(i, 0.5)} className={lit === n.line ? 'lit' : undefined}
            onMouseEnter={() => setLit(n.line)} onMouseLeave={() => setLit(null)}
            onFocus={() => setLit(n.line)} onBlur={() => setLit(null)} tabIndex={0}>
            <b>{i + 1}</b><span><Rich text={n.text} /></span>
          </li>
        ))}
      </ol>
    </div>
  )
}

/** How many make it through each stage, from the first. */
export function Funnel({ body, still }: { body: Of<'funnel'>; still: boolean }) {
  const ref = useRef<HTMLDivElement>(null)
  const reached = useReached(ref, still)
  const first = body.stages[0]?.value || 1
  return (
    <div ref={ref} className={`fig funnel${reached ? ' on' : ''}`}>
      <div className="funnel-rows" role="img" aria-label={body.stages.map((s) => `${s.label}: ${s.display}`).join('; ')}>
        {body.stages.map((s, i) => {
          const before = body.stages[i - 1]
          const drop = before && before.value > 0 ? Math.round((1 - s.value / before.value) * 100) : null
          return [
            <span key={`l${i}`} className={s.mark ? 'mark' : undefined}>{s.label}</span>,
            <div key={`b${i}`} className="funnel-track">
              <i className={s.mark ? 'mark' : undefined} style={{ width: `${(s.value / first) * 100}%`, ...turn(i, 0.35) }} />
            </div>,
            <span key={`v${i}`} className="funnel-n">{s.display}{drop !== null && drop > 0 && <small>−{drop}%</small>}</span>,
          ]
        })}
      </div>
      {body.caption && <p className="describe">{body.caption}</p>}
    </div>
  )
}
