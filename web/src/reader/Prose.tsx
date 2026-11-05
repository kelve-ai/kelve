// A text file read as text (board R-3): markdown parsed by remark's parser into
// its tree, whose every node knows the lines it came from, and drawn with
// kelve's own elements, a block a row: the gutter's squares, the text, and the
// margin's notes beside it. Nothing from the file is markup: HTML in it shows
// as its text, and images as their words, never fetched.

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { Nodes, RootContent } from 'mdast'
import { block, parse } from '../ui/markdown'
import type { Marked, NoteRef } from '../api/types'
import { bringIntoView } from './code'
import { type Chosen, type Doc, ideasOn } from './FileView'
import { MarginNote } from './Margin'
import { NoteBox, NotesHint, NoteWell, useNotes } from './NoteParts'
import { useSeen } from './Square'

type Row = { node: Nodes; from: number; to: number; starts: Marked[] }

/** The lines a selection in the text covers, from the rows it starts and ends in. */
function selectedLines(host: HTMLElement): [number, number] | null {
  const s = window.getSelection()
  if (!s || s.isCollapsed || !s.anchorNode || !s.focusNode) return null
  if (!host.contains(s.anchorNode) || !host.contains(s.focusNode)) return null
  const row = (n: Node) => ((n.nodeType === Node.ELEMENT_NODE ? n : n.parentElement) as HTMLElement | null)?.closest<HTMLElement>('.prow')
  const a = row(s.anchorNode)
  const b = row(s.focusNode)
  if (!a || !b) return null
  const from = [Number(a.dataset.from), Number(b.dataset.from)]
  const to = [Number(a.dataset.to), Number(b.dataset.to)]
  return [Math.min(...from), Math.max(...to)]
}

export function Prose({ doc, wide, lit, focus, line, open, scroller, placed, notes }: {
  doc: Doc
  wide: boolean
  lit?: string
  focus?: string
  line?: number
  open: (m: Marked) => void
  scroller: React.RefObject<HTMLDivElement | null>
  placed: (where: Array<{ key: string; top: number }>) => void
  notes: NoteRef[]
}) {
  const seen = useSeen()
  const noted = useNotes()
  const host = useRef<HTMLDivElement>(null)
  const [chosen, setChosen] = useState<Chosen | null>(null)
  const [draft, setDraft] = useState<{ start: number; end: number } | null>(null)

  // Lines chosen: a Note button at the end of them.
  useEffect(() => {
    const el = host.current
    if (!el || !noted) return
    const up = () => {
      const lines = selectedLines(el)
      const range = window.getSelection()?.rangeCount ? window.getSelection()!.getRangeAt(0).getBoundingClientRect() : null
      if (!lines || !range) return setChosen(null)
      const box = el.getBoundingClientRect()
      setChosen({ start: lines[0], end: lines[1], top: range.bottom - box.top + 6, left: Math.max(56, range.right - box.left - 40) })
    }
    const key = (e: KeyboardEvent) => {
      if ((e.key === 'n' || e.key === 'N') && !(e.target as HTMLElement).closest('textarea, input')) {
        const lines = selectedLines(el)
        if (lines) {
          e.preventDefault()
          setDraft({ start: lines[0], end: lines[1] })
          setChosen(null)
        }
      }
    }
    el.addEventListener('mouseup', up)
    el.addEventListener('keyup', up)
    window.addEventListener('keydown', key)
    return () => {
      el.removeEventListener('mouseup', up)
      el.removeEventListener('keyup', up)
      window.removeEventListener('keydown', key)
    }
  }, [noted])
  const startNote = () => {
    if (!chosen) return
    setDraft({ start: chosen.start, end: chosen.end })
    setChosen(null)
    window.getSelection()?.removeAllRanges()
  }
  const shownNotes = notes.filter((n) => n.on?.found)
  // A change's file reads as it is after the change.
  const text = useMemo(() => (doc.kinds ? doc.text.split('\n').filter((_, i) => doc.kinds![i] !== 'removed').join('\n') : doc.text), [doc])
  const rows = useMemo<Row[]>(() => {
    const rows: Row[] = parse(text)
      .map((n) => ({ node: n, from: n.position?.start.line ?? 0, to: n.position?.end.line ?? 0, starts: [] }))
    const placedKeys = new Set<string>()
    const ordered = [...doc.marks].sort((a, b) => (a.mark.span?.line ?? a.mark.start) - (b.mark.span?.line ?? b.mark.start))
    for (const m of ordered) {
      if (placedKeys.has(m.key)) continue
      const start = m.mark.span?.line ?? m.mark.start
      const row = rows.find((r) => r.to >= start)
      if (!row) continue
      placedKeys.add(m.key)
      row.starts.push(m)
    }
    return rows
  }, [text, doc.marks])

  // For each row: whether a stem runs through it, from an idea above that spans lines.
  const stems = useMemo(() => {
    return rows.map((r) => {
      const through = doc.marks.find((m) => !m.mark.span && m.mark.start < r.from && m.mark.end >= r.from)
      return through ? { key: through.key, end: through.mark.end <= r.to } : null
    })
  }, [rows, doc.marks])

  // Where each idea sits, for the ruler: once the scroller is there (on the
  // first draw it isn't yet), and again whenever the text changes height.
  useLayoutEffect(() => {
    const el = host.current
    if (!el) return
    const tell = () => {
      const sc = scroller.current
      if (!sc) return
      const base = sc.getBoundingClientRect().top - sc.scrollTop
      placed(
        [...el.querySelectorAll<HTMLElement>('.prow')].flatMap((r, i) =>
          rows[i]?.starts.map((m) => ({ key: m.key, top: r.getBoundingClientRect().top - base })) ?? [],
        ),
      )
    }
    tell()
    const size = new ResizeObserver(tell)
    size.observe(el)
    return () => size.disconnect()
  }, [rows, scroller, placed, wide])

  const target = focus ?? lit
  useEffect(() => {
    const i = rows.findIndex((r) => r.starts.some((m) => m.key === target))
    const el = host.current?.querySelectorAll<HTMLElement>('.prow')[i]
    if (i >= 0 && el) bringIntoView(scroller.current, el, 0, true)
  }, [target, rows, scroller])
  useEffect(() => {
    if (!line) return
    const i = rows.findIndex((r) => r.to >= line)
    const el = host.current?.querySelectorAll<HTMLElement>('.prow')[i]
    if (i >= 0 && el) bringIntoView(scroller.current, el, 0)
  }, [line, rows, scroller])

  const isLit = (r: Row) => !!lit && doc.marks.some((m) => m.key === lit && (m.mark.span?.line ?? m.mark.start) <= r.to && (m.mark.span?.line ?? m.mark.end) >= r.from)
  const addedIn = (r: Row) => {
    for (let n = r.from; n <= r.to; n++) if (doc.added.has(n)) return true
    return false
  }

  return (
    <div className={`pv${wide ? ' wide' : ''}`} ref={host}>
      {rows.map((r, i) => {
        const first = r.starts[0]
        const spans = first && !first.mark.span && first.mark.end > r.to
        const stem = stems[i]
        const red = (key?: string) => (key && seen.isNew(key) ? ' new' : '')
        return (
          <div key={i} className="prow" data-from={r.from} data-to={r.to}>
            <div className="pg">
              {addedIn(r) && <span className="padd" aria-label="Added in this change">+</span>}
              {stem && <i className={`stem${stem.end ? ' end' : ''}${red(stem.key)}`} />}
              {first && (
                <button type="button" className="gsq" onClick={() => open(first)} aria-label={first.mark.idea?.name ?? first.mark.name}>
                  <i className={`sq${seen.isNew(first.key) ? ' new' : ''}${seen.isRead(first.key) ? ' read' : ''}`} />
                </button>
              )}
              {spans && <i className={`stem down${red(first.key)}`} />}
            </div>
            <div className={`ptx${isLit(r) ? ' hot' : ''}`}>{block(r.node as RootContent, String(i))}</div>
            {r.starts.length > 0 && (
              <div className="pm">
                {r.starts.map((m) => <MarginNote key={m.key} mark={m} on={m.key === lit} open={open} />)}
              </div>
            )}
            {shownNotes
              .filter((n) => n.on!.end >= r.from && n.on!.end <= r.to)
              .map((n) => <div key={n.id} className="pnote"><NoteWell note={n} /></div>)}
            {draft && draft.end >= r.from && draft.end <= r.to && (
              <div className="pnote"><NoteBox path={doc.path} start={draft.start} end={draft.end} ideas={ideasOn(doc.marks, draft.start, draft.end)} close={() => setDraft(null)} /></div>
            )}
          </div>
        )
      })}
      {chosen && !draft && (
        <>
          <span className="note-key sel-keys" style={{ top: chosen.top, left: chosen.left }}>
            <button type="button" className="raised small" onMouseDown={(e) => e.preventDefault()} onClick={startNote}>
              Note
            </button>
          </span>
          <NotesHint top={chosen.top} left={chosen.left} />
        </>
      )}
    </div>
  )
}
