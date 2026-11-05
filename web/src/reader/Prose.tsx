// A text file read as text (board R-3): markdown parsed by remark's parser into
// its tree, whose every node knows the lines it came from, and drawn with
// kelve's own elements, a block a row: the gutter, the text, and notes under
// the lines they're on. Nothing from the file is markup: HTML in it shows
// as its text, and images as their words, never fetched.

import { useEffect, useMemo, useRef, useState } from 'react'
import type { Nodes, RootContent } from 'mdast'
import { block, parse } from '../ui/markdown'
import type { NoteRef } from '../api/types'
import { bringIntoView } from './code'
import type { Chosen, Doc } from './FileView'
import { NoteBox, NotesHint, NoteWell, useNotes } from './NoteParts'

type Row = { node: Nodes; from: number; to: number }

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

export function Prose({ doc, line, scroller, notes }: {
  doc: Doc
  line?: number
  scroller: React.RefObject<HTMLDivElement | null>
  notes: NoteRef[]
}) {
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
  const rows = useMemo<Row[]>(() => parse(text).map((n) => ({ node: n, from: n.position?.start.line ?? 0, to: n.position?.end.line ?? 0 })), [text])

  useEffect(() => {
    if (!line) return
    const i = rows.findIndex((r) => r.to >= line)
    const el = host.current?.querySelectorAll<HTMLElement>('.prow')[i]
    if (i >= 0 && el) bringIntoView(scroller.current, el, 0)
  }, [line, rows, scroller])

  const addedIn = (r: Row) => {
    for (let n = r.from; n <= r.to; n++) if (doc.added.has(n)) return true
    return false
  }

  return (
    <div className="pv" ref={host}>
      {rows.map((r, i) => {
        return (
          <div key={i} className="prow" data-from={r.from} data-to={r.to}>
            <div className="pg">
              {addedIn(r) && <span className="padd" aria-label="Added in this change">+</span>}
            </div>
            <div className="ptx">{block(r.node as RootContent, String(i))}</div>
            {shownNotes
              .filter((n) => n.on!.end >= r.from && n.on!.end <= r.to)
              .map((n) => <div key={n.id} className="pnote"><NoteWell note={n} /></div>)}
            {draft && draft.end >= r.from && draft.end <= r.to && (
              <div className="pnote"><NoteBox path={doc.path} start={draft.start} end={draft.end} close={() => setDraft(null)} /></div>
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
