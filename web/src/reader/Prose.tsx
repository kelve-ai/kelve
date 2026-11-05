// A text file read as text (board R-3): markdown parsed by remark's parser into
// its tree, whose every node knows the lines it came from, and drawn with
// kelve's own elements, a block a row: the gutter and the text. Nothing from
// the file is markup: HTML in it shows as its text, and images as their words,
// never fetched.

import { useEffect, useMemo, useRef } from 'react'
import type { Nodes, RootContent } from 'mdast'
import { block, parse } from '../ui/markdown'
import { bringIntoView } from './code'
import type { Doc } from './FileView'

type Row = { node: Nodes; from: number; to: number }

export function Prose({ doc, line, scroller }: {
  doc: Doc
  line?: number
  scroller: React.RefObject<HTMLDivElement | null>
}) {
  const host = useRef<HTMLDivElement>(null)
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
          <div key={i} className="prow">
            <div className="pg">
              {addedIn(r) && <span className="padd" aria-label="Added in this change">+</span>}
            </div>
            <div className="ptx">{block(r.node as RootContent, String(i))}</div>
          </div>
        )
      })}
    </div>
  )
}
