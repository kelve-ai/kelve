// A file: line numbers, a change's + and − in a column of their own, then the
// code, read only. A change's file shows what changed, with what's as before
// folded to a line; its added lines are tinted green, and its removed lines
// stay where they were, greyed and tinted. Markdown and plain text read as
// text (Prose). Select lines to leave a note on them.

import { useCallback, useEffect, useLayoutEffect, useMemo, useReducer, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Compartment, EditorState, Text as Lines } from '@codemirror/state'
import { Decoration, EditorView, lineNumbers, WidgetType } from '@codemirror/view'
import type { DiffView, FileView as FileShape, NoteRef, Opened } from '../api/types'
import { Loader } from '../Loader'
import { fetchAhead, fetchNow, isKept, useLoad } from '../useLoad'
import { asBefore, colours, diffMarks, folding, foldRuns, grammar, lineClasses, parsed, unfoldAt } from './code'
import { NoteBox, NotesHint, NoteWell, useNotes } from './NoteParts'
import { Prose } from './Prose'

/** A file as the reader draws it: its lines, and what each one is. */
export type Doc = {
  path: string
  lang: string | null
  text: string
  /** For a change: `added`, `removed` or `context`, a line at a time. */
  kinds?: Array<string | undefined>
  /** The number beside each line. */
  numbers?: string[]
  /** A line of the file, as a line of the document. */
  lineOf: (n: number) => number | undefined
  /** A line of the document, as a line of the file: none for one taken out. */
  fileLine: (n: number) => number | undefined
  /** The file's lines a change added. */
  added: Set<number>
}

/** Whether two documents draw the same: the same text and lines. */
function sameDoc(a: Doc, b: Doc): boolean {
  return a.path === b.path && a.lang === b.lang && a.text === b.text
    && (a.kinds ?? []).join() === (b.kinds ?? []).join() && (a.numbers ?? []).join() === (b.numbers ?? []).join()
}

/** A change's file, whole, as one document: the lines it took out where they were. */
function fromDiff(d: DiffView): Doc {
  const lines: string[] = []
  const kinds: Array<string | undefined> = []
  const numbers: string[] = []
  const byNew = new Map<number, number>()
  const byDoc = new Map<number, number>()
  const added = new Set<number>()
  for (const l of d.lines) {
    lines.push(l.text)
    kinds.push(l.kind)
    numbers.push(l.kind === 'removed' ? '' : String(l.new ?? ''))
    if (l.new != null && l.kind !== 'removed') {
      byNew.set(l.new, lines.length)
      byDoc.set(lines.length, l.new)
    }
    if (l.kind === 'added' && l.new != null) added.add(l.new)
  }
  return { path: d.path, lang: d.lang, text: lines.join('\n'), kinds, numbers, lineOf: (n) => byNew.get(n), fileLine: (n) => byDoc.get(n), added }
}

function fromFile(f: FileShape): Doc {
  return { path: f.path, lang: f.lang, text: f.text ?? '', lineOf: (n) => n, fileLine: (n) => n, added: new Set() }
}

/** Read as text: markdown, and plain text in no language kelve knows (`core::code::prose`). */
const isProse = (doc: Doc) => doc.lang === 'markdown' || (doc.lang == null && /\.(txt|text)$/i.test(doc.path))

export type FileProps = {
  opened: Opened
  path: string
  /** Whether it's one of the change's files, shown with what changed. */
  inChange: boolean
  /** A line of the file to bring into view. */
  line?: number
  /** The notes on lines of this file. */
  notes: NoteRef[]
}

/** Images a browser draws, by name: shown as pictures, not read. */
const IMAGE = /\.(png|jpe?g|gif|webp|avif|bmp|ico|svg)$/i

/** What a file's view fetches as it opens: its text, or a change's file whole. */
export function fileAddress(opened: number, path: string, inChange: boolean) {
  return `/api/opened/${opened}/${inChange ? 'diff' : 'file'}?path=${encodeURIComponent(path)}`
}

/** Whether a file's view would show at once: what it fetches is in. */
export function fileIn(opened: number, path: string, inChange: boolean) {
  return IMAGE.test(path) || isKept(fileAddress(opened, path, inChange))
}

/** Fetches a file the person opened. */
export function fetchFile(opened: number, path: string, inChange: boolean) {
  if (IMAGE.test(path)) return Promise.resolve()
  return fetchNow(fileAddress(opened, path, inChange))
}

/** Fetches a file likely opened next, only looking: it shows at once if it is. */
export function fileAhead(opened: number, path: string, inChange: boolean) {
  if (IMAGE.test(path)) return
  const a = fileAddress(opened, path, inChange)
  fetchAhead(a, `${a}&ahead=true`)
}

export function FileView(props: FileProps) {
  const [source, setSource] = useState(false)
  // An SVG is text too: shown as a picture, with its source a click away.
  if (IMAGE.test(props.path) && !source) {
    const svg = /\.svg$/i.test(props.path)
    return <Picture {...props} source={svg ? () => setSource(true) : undefined} />
  }
  return <Text {...props} source={/\.svg$/i.test(props.path) ? () => setSource(false) : undefined} />
}

/** An image, drawn: served sandboxed, so an SVG's script never runs. */
function Picture({ opened, path, source }: FileProps & { source?: () => void }) {
  const [broken, setBroken] = useState(false)
  const name = path.slice(path.lastIndexOf('/') + 1)
  return (
    <div className="fv">
      <div className="fv-scroll">
        <div className="fv-in">
          <FileName path={path}>{source && <button type="button" className="quiet-link" onClick={source}>Show its source</button>}</FileName>
          {broken ? (
            <p className="fv-say">kelve can’t show this image. It may be too large, or not there any more.</p>
          ) : (
            <div className="fv-image">
              <img src={`/api/opened/${opened.id}/image?path=${encodeURIComponent(path)}`} alt={name} onError={() => setBroken(true)} />
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

/** A file read as text: code, or prose. */
function Text(props: FileProps & { source?: () => void }) {
  const { opened, path, inChange } = props
  const address = fileAddress(opened.id, path, inChange)
  const diff = useLoad<DiffView>(inChange ? address : null, [])
  const file = useLoad<FileShape>(inChange ? null : address, [])
  const loaded = inChange ? diff : file
  const skip = inChange ? diff.data?.skip : file.data?.skip
  const fresh = useMemo(() => (diff.data ? fromDiff(diff.data) : file.data?.text != null ? fromFile(file.data) : null), [diff.data, file.data])
  // Fetched again with nothing changed, it's the same document: nothing is
  // drawn again, so the code never flashes.
  const kept = useRef<Doc | null>(null)
  const doc = useMemo(() => {
    if (fresh && kept.current && sameDoc(kept.current, fresh)) return kept.current
    kept.current = fresh
    return fresh
  }, [fresh])
  const [source, setSource] = useState(false)
  if (loaded.error) return <div className="fv"><p className="fv-say">{loaded.error}</p></div>
  if (!loaded.data) return <div className="fv"><p className="fv-say"><Loader size="small" /></p></div>
  if (skip || !doc) return <div className="fv"><FileName path={path} /><p className="fv-say">{skip ?? 'kelve can’t show this file.'}</p></div>
  const prose = isProse(doc) && !source
  const toggle = props.source ? { set: props.source, label: 'Show it as a picture' } : isProse(doc) ? { set: () => setSource(!source), label: source ? 'Show it as text' : 'Show its markdown' } : undefined
  return <Scroller key={`${path}:${prose}`} doc={doc} prose={prose} {...props} source={toggle} />
}

function FileName({ path, children }: { path: string; children?: React.ReactNode }) {
  const slash = path.lastIndexOf('/')
  return (
    <div className="fv-name">
      <span className="mono"><i>{path.slice(0, slash + 1)}</i><b>{path.slice(slash + 1)}</b></span>
      {children}
    </div>
  )
}

/** The file in its own scroller. */
function Scroller({ doc, prose, source, line, notes }: FileProps & {
  doc: Doc
  prose: boolean
  /** A switch between ways of showing it: markdown as text or source, an SVG as a picture. */
  source?: { label: string; set: () => void }
}) {
  const scroller = useRef<HTMLDivElement>(null)
  return (
    <div className="fv">
      <div className="fv-scroll" ref={scroller} tabIndex={-1}>
        <div className={`fv-in${prose ? ' prose' : ''}`}>
          <FileName path={doc.path}>
            {source && (
              <button type="button" className="quiet-link" onClick={source.set}>
                {source.label}
              </button>
            )}
          </FileName>
          {prose ? (
            <Prose doc={doc} line={line} scroller={scroller} notes={notes} />
          ) : (
            <Code doc={doc} line={line} scroller={scroller} notes={notes} />
          )}
        </div>
      </div>
    </div>
  )
}

/** Lines chosen for a note, and where its button sits. */
export type Chosen = { start: number; end: number; top: number; left: number }

/** Elements CodeMirror draws for notes under lines, for React to fill. */
class Slots {
  map = new Map<string, HTMLElement>()
  private fns = new Set<() => void>()
  set(key: string, el: HTMLElement) {
    this.map.set(key, el)
    queueMicrotask(() => this.fns.forEach((f) => f()))
  }
  unset(key: string, el: HTMLElement) {
    if (this.map.get(key) !== el) return
    this.map.delete(key)
    queueMicrotask(() => this.fns.forEach((f) => f()))
  }
  listen(fn: () => void) {
    this.fns.add(fn)
    return () => void this.fns.delete(fn)
  }
}

class Slot extends WidgetType {
  constructor(readonly key: string, readonly slots: Slots) {
    super()
  }
  eq(o: Slot) {
    return o.key === this.key
  }
  toDOM() {
    const el = document.createElement('div')
    el.className = 'cm-slot'
    this.slots.set(this.key, el)
    return el
  }
  destroy(el: HTMLElement) {
    this.slots.unset(this.key, el)
  }
  ignoreEvent() {
    return true
  }
}

/**
 * A file's code, read only, with notes under the lines they're on, and a
 * Note button on lines chosen.
 */
export function Code({ doc, line, scroller, notes }: {
  doc: Doc
  line?: number
  scroller: React.RefObject<HTMLDivElement | null>
  /** The notes on lines of this file. */
  notes: NoteRef[]
}) {
  const noted = useNotes()
  const slots = useMemo(() => new Slots(), [])
  const [, redraw] = useReducer((x: number) => x + 1, 0)
  useEffect(() => slots.listen(redraw), [slots])
  const noting = useRef(new Compartment())
  const [chosen, setChosen] = useState<Chosen | null>(null)
  const [draft, setDraft] = useState<{ start: number; end: number } | null>(null)
  const choose = useRef<(v: EditorView) => void>(() => {})
  const host = useRef<HTMLDivElement>(null)
  const view = useRef<EditorView | null>(null)

  const numbersKey = useMemo(() => doc.numbers?.join('\n'), [doc.numbers])
  const kindsKey = useMemo(() => doc.kinds?.join(), [doc.kinds])
  // A new document is a new editor.
  useEffect(() => {
    if (!host.current) return
    const lines = Lines.of(doc.text.split('\n'))
    const classes = new Map<number, string>()
    doc.kinds?.forEach((k, i) => (k === 'removed' ? classes.set(i + 1, 'cm-gone') : k === 'added' && classes.set(i + 1, 'cm-added')))
    const v = new EditorView({
      parent: host.current,
      state: parsed(EditorState.create({
        doc: lines,
        extensions: [
          EditorState.readOnly.of(true),
          EditorView.editable.of(false),
          EditorView.contentAttributes.of({ 'aria-label': `${doc.path}, read only`, tabindex: '0' }),
          lineNumbers(doc.numbers ? { formatNumber: (n) => doc.numbers![n - 1] ?? '' } : {}),
          folding,
          lineClasses(lines, classes),
          doc.kinds ? diffMarks(doc.kinds) : [],
          grammar(doc.lang),
          colours,
          noting.current.of([]),
          EditorView.updateListener.of((u) => {
            if (u.selectionSet) choose.current(u.view)
          }),
        ],
      })),
    })
    view.current = v
    // What changed, alone: what's as before folded away, each run a line that opens it.
    if (doc.kinds) v.dispatch({ effects: foldRuns.of(asBefore(doc.kinds)) })
    // Once the fonts are in, lines have their heights.
    void document.fonts?.ready.then(() => v.requestMeasure())
    return () => {
      v.destroy()
      view.current = null
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doc.text, doc.lang, doc.path, numbersKey, kindsKey])

  // Lines chosen: a Note button at the end of them.
  choose.current = (v) => {
    const s = v.state.selection.main
    if (s.empty || !noted) return setChosen(null)
    const a = v.state.doc.lineAt(s.from).number
    // A selection that ends at the start of a line doesn't take that line.
    const b = v.state.doc.lineAt(s.to > s.from && v.state.doc.lineAt(s.to).from === s.to ? s.to - 1 : s.to).number
    const lines = Array.from({ length: b - a + 1 }, (_, i) => doc.fileLine(a + i)).filter((n): n is number => n !== undefined)
    const at = v.coordsAtPos(s.head)
    const box = host.current?.getBoundingClientRect()
    if (!lines.length || !at || !box) return setChosen(null)
    setChosen({ start: Math.min(...lines), end: Math.max(...lines), top: at.bottom - box.top + 6, left: Math.max(60, Math.min(at.left - box.left, box.width - 90)) })
  }

  // Notes under the lines they're on, and the box for a new one.
  const shownNotes = useMemo(() => notes.filter((n) => n.on?.found), [notes])
  useEffect(() => {
    const v = view.current
    if (!v) return
    const at = (fileLine: number) => {
      const n = doc.lineOf(fileLine)
      return n && n <= v.state.doc.lines ? v.state.doc.line(n).to : undefined
    }
    const widgets = []
    for (const n of shownNotes) {
      const pos = at(n.on!.end)
      if (pos !== undefined) widgets.push(Decoration.widget({ widget: new Slot(`n:${n.id}`, slots), block: true, side: 2 }).range(pos))
    }
    const d = draft ? at(draft.end) : undefined
    if (d !== undefined) widgets.push(Decoration.widget({ widget: new Slot('draft', slots), block: true, side: 3 }).range(d))
    // A note's lines are never folded away: their folds open.
    const opening = shownNotes.flatMap((n) => {
      const a = doc.lineOf(n.on!.start)
      const b = doc.lineOf(n.on!.end)
      return [...(a ? unfoldAt(v.state, a) : []), ...(b ? unfoldAt(v.state, b) : [])]
    })
    v.dispatch({ effects: [...opening, noting.current.reconfigure(EditorView.decorations.of(Decoration.set(widgets, true)))] })
  }, [shownNotes, draft, doc, slots])
  // What's drawn into them changes their height.
  const filled = [...slots.map.keys()].join(' ')
  useLayoutEffect(() => view.current?.requestMeasure(), [filled, draft, shownNotes])

  const startNote = useCallback(() => {
    if (!chosen) return
    setDraft({ start: chosen.start, end: chosen.end })
    setChosen(null)
    const v = view.current
    if (v) v.dispatch({ selection: { anchor: v.state.selection.main.head } })
  }, [chosen])

  // Bring a line into view, a third of the way down. CodeMirror does it,
  // once it has drawn the lines on the way and knows their heights. A line
  // folded away is opened first.
  const scrollTo = useCallback((fileLine: number) => {
    const v = view.current
    const n = doc.lineOf(fileLine)
    if (!v || !n || n > v.state.doc.lines) return
    const third = Math.round((scroller.current?.clientHeight ?? 600) / 3)
    v.dispatch({ effects: [...unfoldAt(v.state, n), EditorView.scrollIntoView(v.state.doc.line(n).from, { y: 'start', yMargin: third })] })
  }, [doc.lineOf, scroller])
  useEffect(() => {
    if (line) scrollTo(line)
  }, [line, scrollTo])

  // N leaves a note on the lines chosen.
  useEffect(() => {
    const el = scroller.current
    if (!el) return
    const key = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return
      if ((e.target as HTMLElement).closest('textarea, input, .nbox')) return
      if ((e.key === 'n' || e.key === 'N') && chosen) {
        e.preventDefault()
        startNote()
      }
    }
    el.addEventListener('keydown', key)
    return () => el.removeEventListener('keydown', key)
  }, [scroller, chosen, startNote])

  return (
    <div className="fv-body">
      <div className="fv-code" ref={host}>
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
      {[...slots.map.entries()].map(([key, el]) => {
        if (key === 'draft') {
          return draft ? createPortal(<NoteBox path={doc.path} start={draft.start} end={draft.end} close={() => setDraft(null)} />, el, key) : null
        }
        const note = shownNotes.find((n) => `n:${n.id}` === key)
        return note ? createPortal(<NoteWell note={note} />, el, key) : null
      })}
    </div>
  )
}
