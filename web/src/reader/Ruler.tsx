// The overview strip at the work's right edge: where ideas sit in the whole
// file, red or grey; in a change, where it added lines and took them out,
// thinner, under the ideas; and the part on screen. Clicking it goes there.

import { useEffect, useState } from 'react'
import { useSeen } from './Square'

/** A run of lines a change added or took out, by how far down the scroller's content it sits, and how tall. */
export type RulerRun = { kind: 'added' | 'removed'; top: number; height: number }

export function Ruler({ scroller, marks, runs = [] }: {
  scroller: React.RefObject<HTMLElement | null>
  /** Each idea, by how far down the scroller's content it sits. */
  marks: Array<{ key: string; top: number }>
  runs?: RulerRun[]
}) {
  const seen = useSeen()
  const [view, setView] = useState({ top: 0, height: 1, total: 1 })
  useEffect(() => {
    const el = scroller.current
    if (!el) return
    const on = () => setView({ top: el.scrollTop, height: el.clientHeight, total: Math.max(el.scrollHeight, 1) })
    on()
    el.addEventListener('scroll', on, { passive: true })
    const size = new ResizeObserver(on)
    size.observe(el)
    if (el.firstElementChild) size.observe(el.firstElementChild)
    return () => {
      el.removeEventListener('scroll', on)
      size.disconnect()
    }
  }, [scroller])
  // A file with ideas always has its strip, even one that fits on screen.
  if (view.total <= view.height + 2 && !marks.length && !runs.length) return null
  const go = (e: React.MouseEvent<HTMLDivElement>) => {
    const el = scroller.current
    if (!el) return
    const box = e.currentTarget.getBoundingClientRect()
    const at = (e.clientY - box.top) / box.height
    el.scrollTo({ top: at * view.total - view.height / 2 })
  }
  return (
    <div className="ruler" onClick={go} aria-hidden="true">
      <span className="vis" style={{ top: `${(view.top / view.total) * 100}%`, height: `${(view.height / view.total) * 100}%` }} />
      {runs.map((r) => (
        <b key={`${r.kind}:${r.top}`} className={r.kind} style={{ top: `${(r.top / view.total) * 100}%`, height: `${(r.height / view.total) * 100}%` }} />
      ))}
      {/* New ones last, so where two sit together, the red one shows. */}
      {[...marks].sort((a, b) => Number(seen.isNew(a.key)) - Number(seen.isNew(b.key))).map((m) => (
        <i key={m.key} className={seen.isNew(m.key) ? 'new' : undefined} style={{ top: `${(m.top / view.total) * 100}%` }} />
      ))}
    </div>
  )
}
