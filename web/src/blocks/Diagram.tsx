// A diagram, drawn by kelve from its nodes and edges, in one of four fixed
// layouts: a row, a cycle, a stack, a timeline. kelve's red square marks
// the point that matters, always beside a word. With steps, it plays through
// them once when you reach it, then rests on the last; Replay plays it again.
// It's described in words underneath, for everyone. Where its layout won't
// fit the width it has, it's drawn as a stack, so nothing leaves its box.

import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react'
import type { Body, DiagramEdge, DiagramNode } from '../api/types'
import { stillPlease, useNarrow } from '../words'

type Diagram = Extract<Body, { type: 'diagram' }>

/** How long each step shows. */
const STEP = 2800
/** After it comes into view, a moment for the entry to write itself in. */
const SETTLE = 900
/** The least width a node in a row or a timeline has, with the gap after it. */
const NODE = 96
/** A cycle's width and height. */
const W = 600
const H = 380

export function Diagram({ id, body, still }: { id: string; body: Diagram; still: boolean }) {
  const { steps, nodes, edges } = body
  const last = Math.max(0, steps.length - 1)
  const rest = still || stillPlease() || steps.length === 0
  const [frame, setFrame] = useState(rest ? last : 0)
  const [playing, setPlaying] = useState(false)
  const [started, setStarted] = useState(rest)
  const ref = useRef<HTMLDivElement>(null)
  const narrow = useNarrow()
  const [width, setWidth] = useState(0)

  // The width it has, before it's first drawn and whenever that changes.
  useLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    setWidth(el.clientWidth)
    const size = new ResizeObserver(() => setWidth(el.clientWidth))
    size.observe(el)
    return () => size.disconnect()
  }, [])

  // It plays once, when you first reach it.
  useEffect(() => {
    const el = ref.current
    if (started || !el) return
    let timer: number | undefined
    const io = new IntersectionObserver((xs) => {
      if (!xs.some((x) => x.isIntersecting)) return
      io.disconnect()
      timer = window.setTimeout(() => { setStarted(true); setPlaying(true) }, SETTLE)
    }, { threshold: 0.5 })
    io.observe(el)
    return () => { io.disconnect(); window.clearTimeout(timer) }
  }, [started])

  useEffect(() => {
    if (!playing) return
    if (frame >= last) {
      setPlaying(false)
      return
    }
    const t = window.setTimeout(() => setFrame((f) => f + 1), STEP)
    return () => window.clearTimeout(t)
  }, [playing, frame, last])

  const replay = () => {
    setFrame(0)
    setPlaying(true)
  }

  const atRest = steps.length === 0 || (!playing && frame === last)
  const marks = new Set(steps[frame]?.marks ?? [])
  if (atRest) for (const n of nodes) if (n.mark) marks.add(n.id)
  // A node a later step brings in waits, faint, until then.
  const waits = (n: DiagramNode) => {
    const at = steps.findIndex((s) => s.marks.includes(n.id))
    return at > frame
  }
  const between = (a: DiagramNode, b: DiagramNode): DiagramEdge | undefined =>
    edges.find((e) => (e.from === a.id && e.to === b.id) || (e.from === b.id && e.to === a.id))

  const cls = (n: DiagramNode) => `dg-node${marks.has(n.id) ? ' on' : ''}${waits(n) ? ' waits' : ''}`
  const text = (n: DiagramNode) => (
    <span className="dg-text">
      <span className="dg-label">{n.label}</span>
      {n.note && <span className="dg-note">{n.note}</span>}
    </span>
  )

  // A row or timeline with too many nodes for its width, or a cycle wider
  // than it, is a stack; on a phone, all of them are.
  const fits = body.layout === 'cycle' ? width >= W : nodes.length * NODE <= width
  const layout = (narrow || !fits) && body.layout !== 'stack' ? 'stack' : body.layout
  let picture
  if (layout === 'stack') {
    // Top to bottom; a cycle on a phone, too, coming back round at the end.
    const back = body.layout === 'cycle' && nodes.length > 1 ? between(nodes[nodes.length - 1], nodes[0]) : undefined
    picture = (
      <div className="dg-stack">
        {nodes.map((n, i) => {
          const next = nodes[i + 1]
          const label = next ? between(n, next)?.label : body.layout === 'cycle' && nodes.length > 1 ? (back?.label ?? `back to ${nodes[0].label}`) : undefined
          return (
            <div key={n.id} className={cls(n)}>
              <span className="dg-sq" />
              <span>
                {text(n)}
                {label && <span className="dg-edge">{label}</span>}
              </span>
              {next && <span className="dg-vseg" />}
            </div>
          )
        })}
      </div>
    )
  } else if (layout === 'cycle') {
    picture = <Cycle nodes={nodes} between={between} cls={cls} text={text} />
  } else {
    picture = (
      <div className={`dg-line${layout === 'row' ? ' arrows' : ''}`} style={{ gridTemplateColumns: `repeat(${nodes.length}, minmax(0, 1fr))` }}>
        {nodes.map((n, i) => {
          const next = nodes[i + 1]
          const label = next ? between(n, next)?.label : undefined
          return (
            <div key={n.id} className={cls(n)}>
              <div className="dg-pt">
                <span className="dg-sq" />
                {next && <span className="dg-seg">{label && <em>{label}</em>}</span>}
              </div>
              {text(n)}
            </div>
          )
        })}
      </div>
    )
  }

  const caption = steps[frame]?.caption
  return (
    <div className="dg" ref={ref}>
      <div role="img" aria-labelledby={`${id}-describe`}>
        {picture}
        {steps.length > 0 && <p key={frame} className="dg-cap fade-in" aria-hidden="true">{caption}</p>}
      </div>
      <p id={`${id}-describe`} className="describe">{body.describe}</p>
      {steps.length > 1 && started && !playing && <button className="word soft more" onClick={replay}>Replay</button>}
    </div>
  )
}

/** Round and back to the start, on a circle, clockwise from the top. */
function Cycle({ nodes, between, cls, text }: {
  nodes: DiagramNode[]
  between: (a: DiagramNode, b: DiagramNode) => DiagramEdge | undefined
  cls: (n: DiagramNode) => string
  text: (n: DiagramNode) => ReactNode
}) {
  const cx = W / 2
  const cy = H / 2
  const r = 125
  const angle = (k: number) => -Math.PI / 2 + (k * 2 * Math.PI) / nodes.length
  const point = (a: number, radius = r) => [cx + radius * Math.cos(a), cy + radius * Math.sin(a)]
  // Room above and below for labels and notes that reach past the picture.
  const ref = useRef<HTMLDivElement>(null)
  const [room, setRoom] = useState({ above: 0, below: 0 })
  useLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    const measure = () => {
      const box = el.getBoundingClientRect()
      let top = box.top
      let bottom = box.top + H
      for (const x of el.querySelectorAll('.dg-place, .dg-clabel')) {
        const at = x.getBoundingClientRect()
        top = Math.min(top, at.top)
        bottom = Math.max(bottom, at.bottom)
      }
      const above = Math.ceil(box.top - top)
      const below = Math.ceil(bottom - box.top - H)
      setRoom((was) => (was.above === above && was.below === below ? was : { above, below }))
    }
    measure()
    const size = new ResizeObserver(measure)
    for (const x of el.querySelectorAll('.dg-place')) size.observe(x)
    return () => size.disconnect()
  })
  return (
    <div style={{ paddingTop: room.above, paddingBottom: room.below }}>
      <div ref={ref} className="dg-cycle" style={{ height: H }}>
        <svg viewBox={`0 0 ${W} ${H}`} width={W} height={H} aria-hidden="true">
          <circle className="dg-ring" cx={cx} cy={cy} r={r} />
          {nodes.length > 1 && nodes.map((_, k) => {
            const mid = angle(k + 0.5)
            const [x, y] = point(mid)
            const deg = (mid * 180) / Math.PI + 90
            return <path key={k} className="dg-arrow" d="M -3 -4 L 2 0 L -3 4" transform={`translate(${x} ${y}) rotate(${deg})`} />
          })}
        </svg>
        {nodes.map((n, k) => {
          const a = angle(k)
          const [x, y] = point(a)
          const c = Math.cos(a)
          const s = Math.sin(a)
          const place: CSSProperties =
            c > 0.35 ? { left: x + 16, top: y - 11, maxWidth: W - x - 16 }
            : c < -0.35 ? { right: W - x + 16, top: y - 11, maxWidth: x - 16, textAlign: 'right' }
            : s < 0 ? { left: x, top: y - 14, transform: 'translate(-50%, -100%)', textAlign: 'center' }
            : { left: x, top: y + 14, transform: 'translateX(-50%)', textAlign: 'center' }
          return (
            <div key={n.id} className={cls(n)}>
              <span className="dg-sq" style={{ position: 'absolute', left: x - 4.5, top: y - 4.5 }} />
              <div className="dg-place" style={place}>{text(n)}</div>
            </div>
          )
        })}
        {nodes.map((n, k) => {
          const next = nodes[(k + 1) % nodes.length]
          const label = nodes.length > 1 ? between(n, next)?.label : undefined
          if (!label) return null
          const [x, y] = point(angle(k + 0.5), r - 38)
          return <span key={`e${k}`} className="dg-clabel" style={{ left: x, top: y }}>{label}</span>
        })}
      </div>
    </div>
  )
}
