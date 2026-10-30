// A text file read as text (board R-3): markdown parsed by remark's parser into
// its tree, whose every node knows the lines it came from, and drawn with
// kelve's own elements, a block a row: the gutter's squares, the text, and the
// margin's notes beside it. Nothing from the file is markup: HTML in it shows
// as its text, and images as their words, never fetched.

import { Fragment, type ReactNode, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { fromMarkdown } from 'mdast-util-from-markdown'
import { gfmFromMarkdown } from 'mdast-util-gfm'
import { gfm } from 'micromark-extension-gfm'
import type { Nodes, Parents, RootContent } from 'mdast'
import type { Marked, NoteRef } from '../api/types'
import { bringIntoView } from './code'
import { type Chosen, type Doc, ideasOn } from './FileView'
import { MarginNote } from './Margin'
import { NoteBox, NotesHint, NoteWell, useNotes } from './NoteParts'
import { useSeen } from './Square'

/** Only links people expect to follow leave the page. */
const safeHref = (url: string) => (/^(https?:|mailto:)/i.test(url) ? url : undefined)

function inline(nodes: RootContent[], key: string): ReactNode[] {
  return nodes.map((node, i) => {
    const k = `${key}.${i}`
    switch (node.type) {
      case 'text':
        return <Fragment key={k}>{node.value}</Fragment>
      case 'inlineCode':
        return <code key={k}>{node.value}</code>
      case 'emphasis':
        return <em key={k}>{inline(node.children, k)}</em>
      case 'strong':
        return <strong key={k}>{inline(node.children, k)}</strong>
      case 'delete':
        return <del key={k}>{inline(node.children, k)}</del>
      case 'break':
        return <br key={k} />
      case 'link': {
        const href = safeHref(node.url)
        return href ? <a key={k} href={href} target="_blank" rel="noreferrer noopener">{inline(node.children, k)}</a> : <span key={k}>{inline(node.children, k)}</span>
      }
      case 'image':
      case 'imageReference':
        // Never fetched: its words stand in for it.
        return node.alt ? <span key={k} className="alt">[{node.alt}]</span> : null
      case 'html':
        return <code key={k}>{node.value}</code>
      case 'footnoteReference':
        return <sup key={k}>{node.label ?? node.identifier}</sup>
      default:
        return 'children' in node ? <Fragment key={k}>{inline((node as Parents).children as RootContent[], k)}</Fragment> : null
    }
  })
}

function block(node: RootContent, key: string): ReactNode {
  switch (node.type) {
    case 'heading': {
      const H = `h${Math.min(node.depth + 1, 6)}` as 'h2'
      return <H key={key}>{inline(node.children, key)}</H>
    }
    case 'paragraph':
      return <p key={key}>{inline(node.children, key)}</p>
    case 'blockquote':
      return <blockquote key={key}>{node.children.map((n, i) => block(n, `${key}.${i}`))}</blockquote>
    case 'list': {
      const L = node.ordered ? 'ol' : 'ul'
      return (
        <L key={key} start={node.ordered ? (node.start ?? undefined) : undefined}>
          {node.children.map((item, i) => (
            <li key={i}>
              {item.checked != null && <span className="tick" aria-label={item.checked ? 'Done' : 'Not done'}>{item.checked ? '☑' : '☐'} </span>}
              {item.children.map((n, j) => (n.type === 'paragraph' && !node.spread ? <Fragment key={j}>{inline(n.children, `${key}.${i}.${j}`)}</Fragment> : block(n, `${key}.${i}.${j}`)))}
            </li>
          ))}
        </L>
      )
    }
    case 'code':
      return <pre key={key}><code>{node.value}</code></pre>
    case 'thematicBreak':
      return <hr key={key} />
    case 'table':
      return (
        <div key={key} className="ptable">
          <table>
            <tbody>
              {node.children.map((row, i) => (
                <tr key={i}>
                  {row.children.map((cell, j) => {
                    const Cell = i === 0 ? 'th' : 'td'
                    return <Cell key={j}>{inline(cell.children, `${key}.${i}.${j}`)}</Cell>
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )
    case 'html':
      return <pre key={key}><code>{node.value}</code></pre>
    case 'definition':
    case 'footnoteDefinition':
      return null
    default:
      return 'children' in node ? <div key={key}>{((node as Parents).children as RootContent[]).map((n, i) => block(n, `${key}.${i}`))}</div> : null
  }
}

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
    const tree = fromMarkdown(text, { extensions: [gfm()], mdastExtensions: [gfmFromMarkdown()] })
    const rows: Row[] = tree.children
      .filter((n) => n.type !== 'definition' && n.type !== 'footnoteDefinition')
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
