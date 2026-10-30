// A file: line numbers, the gutter's squares, a change's + and − in a column of
// their own, then the code, read only, with each idea in the margin beside its
// first line and the overview strip at the right edge. A change's file shows
// what changed, with what's as before folded to a line; its added lines are
// tinted green, and its removed lines stay where they were, greyed and tinted.
// Markdown and plain text read as text (Prose). A code or text file reads
// plainly, unless you chose as code (Plainly). ] and [ step between ideas;
// Enter opens the one you're on.

import { useCallback, useEffect, useLayoutEffect, useMemo, useReducer, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Compartment, EditorState, type Extension, RangeSet, Text as Lines } from '@codemirror/state'
import { Decoration, EditorView, GutterMarker, gutterLineClass, lineNumbers, WidgetType } from '@codemirror/view'
import type { DiffView, FileView as FileShape, Marked, Marking, NoteRef, Opened, ReadAs, ServerEvent } from '../api/types'
import { Loader } from '../Loader'
import { MARKS, useMarks } from '../screens/Marks'
import { fetchAhead, fetchNow, isKept, useLoad } from '../useLoad'
import { asBefore, changedRuns, colours, diffMarks, folding, foldOf, foldRuns, grammar, lineClasses, parsed, place, type Placed, squares, unfoldAt } from './code'
import { ideaName, Margin, type Want } from './Margin'
import { NoteBox, NotesHint, NoteWell, useNotes } from './NoteParts'
import { opensAs, Plainly, ReadSwitch } from './Plainly'
import { Prose } from './Prose'
import { Ruler, type RulerRun } from './Ruler'
import { type Seen, squareLabel, useSeen } from './Square'
import { useSeenPlainly } from './seen'

/** Below this width, the margin's notes go under their lines. */
const MARGIN_FROM = 720

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
  marks: Marked[]
  marking: Marking
}

/** Whether two documents draw the same: the same text, lines and marks. */
function sameDoc(a: Doc, b: Doc): boolean {
  return a.path === b.path && a.lang === b.lang && a.text === b.text && a.marking === b.marking
    && (a.kinds ?? []).join() === (b.kinds ?? []).join() && (a.numbers ?? []).join() === (b.numbers ?? []).join()
    && JSON.stringify(a.marks) === JSON.stringify(b.marks)
}

/** Marks that came in for this file. */
const marksFor = (opened: number, path: string) => (e: ServerEvent) =>
  e.type !== 'marks' || (e.opened === opened && e.path === path)

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
  return { path: d.path, lang: d.lang, text: lines.join('\n'), kinds, numbers, lineOf: (n) => byNew.get(n), fileLine: (n) => byDoc.get(n), added, marks: d.marks, marking: d.marking }
}

function fromFile(f: FileShape): Doc {
  return { path: f.path, lang: f.lang, text: f.text ?? '', lineOf: (n) => n, fileLine: (n) => n, added: new Set(), marks: f.marks, marking: f.marking }
}

/** Read as text: markdown, and plain text. */
const isProse = (doc: Doc) => doc.lang === 'markdown' || /\.(txt|text)$/i.test(doc.path)

export type FileProps = {
  opened: Opened
  path: string
  /** Whether it's one of the change's files, shown with what changed. */
  inChange: boolean
  /** The idea whose entry is open: its lines are lit. */
  lit?: string
  /** An idea to bring into view. */
  focus?: string
  /** A line of the file to bring into view. */
  line?: number
  open: (key: string) => void
  /** Opens a lesson, beside the work. */
  lesson?: (id: string) => void
  /** The notes on lines of this file. */
  notes: NoteRef[]
  /** What to read next, when you're reading in order: at the file's end. */
  next?: React.ReactNode
  /** How a file reads this visit, as its switch says: none for as you chose in What kelve marks. */
  reads?: ReadAs | null
  /** Flips its switch, for this visit. */
  setReads?: (r: ReadAs) => void
  /** Opens a file, or a folder (its path ends in `/`): one that uses this one. */
  openPath?: (path: string) => void
}

/** Images a browser draws, by name: shown as pictures, not read. */
const IMAGE = /\.(png|jpe?g|gif|webp|avif|bmp|ico|svg)$/i

/** What a file's view fetches as it opens: its text, or a change's file whole, and it read plainly. */
export function fileAddresses(opened: number, path: string, inChange: boolean) {
  const at = `path=${encodeURIComponent(path)}`
  return { text: `/api/opened/${opened}/${inChange ? 'diff' : 'file'}?${at}`, plainly: `/api/opened/${opened}/plainly?${at}` }
}

/** The same address, only looking: nothing is asked of your agent for it. */
const looking = (address: string) => `${address}&ahead=true`

/** Whether a file's view would show at once: what it fetches is in. */
export function fileIn(opened: number, path: string, inChange: boolean) {
  if (IMAGE.test(path)) return true
  const a = fileAddresses(opened, path, inChange)
  return isKept(a.text) && isKept(a.plainly) && isKept(MARKS)
}

/** Fetches a file the person opened: its text, and only looks at it read plainly, which it may not be. */
export function fetchFile(opened: number, path: string, inChange: boolean) {
  if (IMAGE.test(path)) return Promise.resolve()
  const a = fileAddresses(opened, path, inChange)
  return Promise.all([fetchNow(a.text), fetchNow(a.plainly, looking(a.plainly)), fetchNow(MARKS)])
}

/** Fetches a file likely opened next, only looking: it shows at once if it is. */
export function fileAhead(opened: number, path: string, inChange: boolean) {
  if (IMAGE.test(path)) return
  const a = fileAddresses(opened, path, inChange)
  fetchAhead(a.text, looking(a.text))
  fetchAhead(a.plainly, looking(a.plainly))
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
  const addresses = fileAddresses(opened.id, path, inChange)
  const diff = useLoad<DiffView>(inChange ? addresses.text : null, ['marks', 'spec'], marksFor(opened.id, path))
  const file = useLoad<FileShape>(inChange ? null : addresses.text, ['marks', 'spec'], marksFor(opened.id, path))
  // Read plainly, it's on its way as the text is, not after.
  useEffect(() => fetchAhead(addresses.plainly, looking(addresses.plainly)), [addresses.plainly])
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
  // A code or text file reads plainly or as code: as you chose, or as its
  // switch says.
  const marks = useMarks()
  const switches = !!doc && (isProse(doc) || doc.lang != null) && !props.source && !!props.setReads
  const reads = doc && switches ? (props.reads ?? opensAs(marks.data?.spec, doc.lang)) : 'code'
  // Read as code, a step of the way is read once its marks are done: seen
  // as your agent left it, never before (Nathan, 29 October). Read
  // plainly, `Plainly` says so once it shows what it says.
  useSeenPlainly(doc && reads === 'code' ? doc.path : null, loaded.data?.marking === 'done')
  if (loaded.error) return <div className="fv"><p className="fv-say">{loaded.error}</p></div>
  if (!loaded.data) return <div className="fv"><p className="fv-say"><Loader size="small" /></p></div>
  if (skip || !doc) return <div className="fv"><FileName path={path} /><p className="fv-say">{skip ?? 'kelve can’t show this file.'}</p></div>
  const prose = isProse(doc) && !source
  const toggle = props.source ? { set: props.source, label: 'Show it as a picture' } : isProse(doc) ? { set: () => setSource(!source), label: source ? 'Show it as text' : 'Show its markdown' } : undefined
  const readSwitch = switches && props.setReads ? <ReadSwitch value={reads} set={props.setReads} /> : undefined
  const scroller = (lead?: React.ReactNode) => <Scroller key={`${path}:${prose}`} doc={doc} prose={prose} {...props} source={toggle} readSwitch={readSwitch} lead={lead} />
  if (reads === 'plainly') {
    const name = <FileName path={doc.path}>{readSwitch}</FileName>
    return <Plainly opened={props.opened} doc={doc} notes={props.notes} name={name} lit={props.lit} open={props.open} lesson={props.lesson} openPath={props.openPath} next={props.next} meanwhile={scroller} />
  }
  return scroller()
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

function MarkingLine({ marking }: { marking: Marking }) {
  if (marking === 'done') return null
  return (
    <p className="fv-marking">
      <Loader size="small" label="Being read" />
      Your agent is reading this file. Its ideas show here as it marks them.
    </p>
  )
}

/** The file in its own scroller, with the ruler beside it. */
function Scroller({ doc, prose, source, readSwitch, lead, lit, focus, line, open, notes, next }: FileProps & {
  doc: Doc
  prose: boolean
  /** A switch between ways of showing it: markdown as text or source, an SVG as a picture. */
  source?: { label: string; set: () => void }
  /** Plainly or As code. */
  readSwitch?: React.ReactNode
  /** Under its name: where its plainly is, while it isn't written. */
  lead?: React.ReactNode
}) {
  const scroller = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(1200)
  const [where, setWhere] = useState<Array<{ key: string; top: number }>>([])
  const [runs, setRuns] = useState<RulerRun[]>([])
  const placed = useCallback((w: Array<{ key: string; top: number }>, r: RulerRun[] = []) => {
    setWhere(w)
    setRuns(r)
  }, [])
  useLayoutEffect(() => {
    const el = scroller.current
    if (!el) return
    const size = new ResizeObserver(() => setWidth(el.clientWidth))
    size.observe(el)
    setWidth(el.clientWidth)
    return () => size.disconnect()
  }, [])
  const wide = width >= MARGIN_FROM
  const opening = useCallback((m: Marked) => open(m.key), [open])
  return (
    <div className="fv">
      <div className="fv-scroll" ref={scroller} tabIndex={-1}>
        <div className={`fv-in${prose ? ' prose' : ''}`}>
          <FileName path={doc.path}>
            {readSwitch}
            {source && (
              <button type="button" className="quiet-link" onClick={source.set}>
                {source.label}
              </button>
            )}
          </FileName>
          {lead}
          {/* The line at its head says where its reading is, when there is one. */}
          {!lead && <MarkingLine marking={doc.marking} />}
          {prose ? (
            <Prose doc={doc} wide={wide} lit={lit} focus={focus} line={line} open={opening} scroller={scroller} placed={placed} notes={notes} />
          ) : (
            <Code doc={doc} wide={wide} width={width} lit={lit} focus={focus} line={line} open={opening} scroller={scroller} placed={placed} notes={notes} />
          )}
          {next}
        </div>
      </div>
      <Ruler scroller={scroller} marks={where} runs={runs} />
    </div>
  )
}

/** A note under its line, where there's no room for the margin. */
class Under extends WidgetType {
  constructor(readonly mark: Marked, readonly label: string, readonly classes: string, readonly open: (m: Marked) => void) {
    super()
  }
  eq(o: Under) {
    return o.mark.key === this.mark.key && o.label === this.label && o.classes === this.classes && o.mark.mark.use === this.mark.mark.use
  }
  toDOM() {
    const b = document.createElement('button')
    b.type = 'button'
    b.className = 'mnote under'
    const name = document.createElement('b')
    const sq = document.createElement('i')
    sq.className = this.classes
    sq.setAttribute('role', 'img')
    sq.setAttribute('aria-label', this.label)
    name.append(sq, ideaName(this.mark))
    const use = document.createElement('span')
    for (const part of (this.mark.mark.use || this.mark.mark.definition).split(/(`[^`]+`)/g)) {
      if (part.startsWith('`') && part.endsWith('`') && part.length > 1) {
        const c = document.createElement('code')
        c.textContent = part.slice(1, -1)
        use.append(c)
      } else use.append(part)
    }
    b.append(name, use)
    b.addEventListener('click', () => this.open(this.mark))
    return b
  }
  ignoreEvent() {
    return true
  }
}

function underNotes(view: EditorView, placed: Placed[], seen: Seen, open: (m: Marked) => void): Extension {
  const seenKeys = new Set<string>()
  const widgets = placed
    .filter((p) => !seenKeys.has(p.mark.key) && seenKeys.add(p.mark.key))
    .map((p) => {
      const isNew = seen.isNew(p.mark.key)
      const read = seen.isRead(p.mark.key)
      const classes = `sq${isNew ? ' new' : ''}${read ? ' read' : ''}`
      const at = view.state.doc.line(p.a).to
      return Decoration.widget({ widget: new Under(p.mark, squareLabel(ideaName(p.mark), isNew, read), classes, open), block: true, side: 1 }).range(at)
    })
  return EditorView.decorations.of(Decoration.set(widgets, true))
}

type ViewProps = {
  doc: Doc
  wide: boolean
  lit?: string
  focus?: string
  line?: number
  open: (m: Marked) => void
  scroller: React.RefObject<HTMLDivElement | null>
  /** Hears where each idea sits, for the ruler, and each run a change added or took out. */
  placed: (where: Array<{ key: string; top: number }>, runs?: RulerRun[]) => void
  /** The notes on lines of this file. */
  notes: NoteRef[]
}

/** The ideas marked on lines of the file. */
export const ideasOn = (marks: Marked[], start: number, end: number) =>
  [...new Set(marks.filter((m) => (m.mark.span?.line ?? m.mark.start) <= end && (m.mark.span?.line ?? m.mark.end) >= start).map((m) => m.key))]

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

/** A line around a part's own, in the gutter: quieter. */
class Around extends GutterMarker {
  elementClass = 'cm-around'
}
const around = new Around()

/**
 * A file's code, read only: the gutter's squares, the margin's notes beside
 * it, or under their lines where there's no room, and notes under the lines
 * they're on, with a Note button on lines chosen. A part of a file read
 * plainly shows its lines the same way: `whole`, with nothing folded, and
 * the lines around the part's own `quiet`.
 */
export function Code({ doc, wide, width, lit, focus, line, open, scroller, placed, notes, whole, quiet }: ViewProps & {
  width: number
  /** Every line shown: nothing a change left as it was is folded away. */
  whole?: boolean
  /** Lines of the document drawn quieter: those around a part's own. */
  quiet?: number[]
}) {
  const seen = useSeen()
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
  const marking = useRef(new Compartment())
  const notesUnder = useRef(new Compartment())
  const [wants, setWants] = useState<Want[]>([])
  const told = useRef<{ where: Array<{ key: string; top: number }>; runs: RulerRun[] }>({ where: [], runs: [] })
  const [cursor, setCursor] = useState<string | null>(null)
  const openRef = useRef(open)
  openRef.current = open

  const marks = useMemo(() => {
    // Each idea once, at its first place in the file.
    const by = new Map<string, Marked>()
    for (const m of doc.marks) {
      const had = by.get(m.key)
      if (!had || (m.mark.span?.line ?? m.mark.start) < (had.mark.span?.line ?? had.mark.start)) by.set(m.key, m)
    }
    return [...by.values()]
  }, [doc.marks])

  const changed = useMemo(() => (doc.kinds ? changedRuns(doc.kinds) : []), [doc.kinds])

  // Where each idea's note wants to sit: beside its first line. An idea on
  // lines folded away has none, until they're opened.
  const measure = useCallback(() => {
    const v = view.current
    if (!v) return
    const ps = place(v.state.doc, marks, doc.lineOf).filter((p) => !foldOf(v.state, p.a))
    const next = ps.map((p) => ({ mark: p.mark, top: v.lineBlockAt(v.state.doc.line(p.a).from).top }))
    setWants((was) => (was.length === next.length && was.every((w, i) => w.mark === next[i].mark && w.top === next[i].top) ? was : next))
    const offset = (host.current?.getBoundingClientRect().top ?? 0) - (scroller.current?.getBoundingClientRect().top ?? 0) + (scroller.current?.scrollTop ?? 0)
    // Only when something moved: telling the ruler draws the file again.
    const where = next.map((w) => ({ key: w.mark.key, top: offset + w.top }))
    const runs = changed.filter((r) => r.b <= v.state.doc.lines).map((r) => {
      const top = v.lineBlockAt(v.state.doc.line(r.a).from).top
      return { kind: r.kind, top: offset + top, height: v.lineBlockAt(v.state.doc.line(r.b).from).bottom - top }
    })
    const was = told.current
    const same = <T extends { top: number }>(a: T[], b: T[], eq: (x: T, y: T) => boolean) => a.length === b.length && a.every((x, i) => eq(x, b[i]) && Math.abs(x.top - b[i].top) < 1)
    if (same(was.where, where, (x, y) => x.key === y.key) && same(was.runs, runs, (x, y) => x.kind === y.kind && Math.abs(x.height - y.height) < 1)) return
    told.current = { where, runs }
    placed(where, runs)
  }, [marks, doc.lineOf, scroller, placed, changed])
  const measureRef = useRef(measure)
  measureRef.current = measure

  // What the editor is drawn from, by content: a document fetched again
  // with new marks is drawn again by marks alone, never a new editor.
  const numbersKey = useMemo(() => doc.numbers?.join('\n'), [doc.numbers])
  const kindsKey = useMemo(() => doc.kinds?.join(), [doc.kinds])
  const quietKey = quiet?.join()
  // A new document is a new editor.
  useEffect(() => {
    if (!host.current) return
    const lines = Lines.of(doc.text.split('\n'))
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
          quiet?.length ? gutterLineClass.of(RangeSet.of(quiet.filter((n) => n >= 1 && n <= lines.lines).map((n) => around.range(lines.line(n).from)))) : [],
          marking.current.of([]),
          doc.kinds ? diffMarks(doc.kinds) : [],
          grammar(doc.lang),
          colours,
          notesUnder.current.of([]),
          noting.current.of([]),
          EditorView.updateListener.of((u) => {
            if (u.geometryChanged || u.heightChanged) measureRef.current()
            if (u.selectionSet) choose.current(u.view)
          }),
        ],
      })),
    })
    view.current = v
    // Once the fonts are in, lines have their heights.
    void document.fonts?.ready.then(() => v.requestMeasure())
    return () => {
      v.destroy()
      view.current = null
    }
  }, [doc.text, doc.lang, doc.path, numbersKey, kindsKey, quietKey])

  // What changed, alone: what's as before folded away, each run a line
  // that opens it.
  useEffect(() => {
    view.current?.dispatch({ effects: foldRuns.of(doc.kinds && !whole ? asBefore(doc.kinds) : []) })
  }, [kindsKey, whole])

  // Marks, and what's lit, without drawing the file again.
  useEffect(() => {
    const v = view.current
    if (!v) return
    const ps = place(v.state.doc, marks, doc.lineOf)
    const classes = new Map<number, string>()
    doc.kinds?.forEach((k, i) => (k === 'removed' ? classes.set(i + 1, 'cm-gone') : k === 'added' && classes.set(i + 1, 'cm-added')))
    for (const n of quiet ?? []) classes.set(n, `${classes.get(n) ?? ''} cm-around`.trim())
    for (const p of ps) {
      if (p.mark.key !== lit && p.mark.key !== cursor) continue
      for (let n = p.a; n <= p.b; n++) classes.set(n, `${classes.get(n) ?? ''} cm-lit`.trim())
    }
    v.dispatch({ effects: marking.current.reconfigure([squares(ps, seen, (m) => openRef.current(m)), lineClasses(v.state.doc, classes)]) })
    v.dispatch({ effects: notesUnder.current.reconfigure(wide ? [] : underNotes(v, ps, seen, (m) => openRef.current(m))) })
    measure()
  }, [marks, doc.lineOf, doc.kinds, lit, cursor, seen, wide, measure, quietKey])

  // Where a margin beside the code would cut its longest line short, as on
  // a laptop, the notes float over the code instead, each under its line.
  const longest = useMemo(() => doc.text.split('\n').reduce((n, l) => Math.max(n, l.replace(/\t/g, '    ').length), 0), [doc.text])
  const [over, setOver] = useState(false)
  useLayoutEffect(() => {
    const v = view.current
    if (!v || !wide) return setOver(false)
    const gutters = v.dom.querySelector('.cm-gutters')?.getBoundingClientRect().width ?? 60
    const code = gutters + longest * v.defaultCharacterWidth + 24
    const body = width - 40
    setOver(code > body - Math.min(320, Math.max(220, body * 0.28)))
  }, [wide, width, longest, doc.text])

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

  // Bring an idea, or a line, into view, a third of the way down. CodeMirror
  // does it, once it has drawn the lines on the way and knows their heights:
  // lines not yet drawn are only guessed at, so a scroll worked out here
  // would stop short in a long file just opened. A line folded away is
  // opened first.
  const scrollTo = useCallback((fileLine: number) => {
    const v = view.current
    const n = doc.lineOf(fileLine)
    if (!v || !n || n > v.state.doc.lines) return
    const third = Math.round((scroller.current?.clientHeight ?? 600) / 3)
    v.dispatch({ effects: [...unfoldAt(v.state, n), EditorView.scrollIntoView(v.state.doc.line(n).from, { y: 'start', yMargin: third })] })
  }, [doc.lineOf, scroller])
  const target = focus ?? lit
  const targetLine = marks.find((m) => m.key === target)?.mark
  useEffect(() => {
    if (targetLine) scrollTo(targetLine.span?.line ?? targetLine.start)
    // Only when what's asked for changes, not as marks come in.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target, !!targetLine])
  useEffect(() => {
    if (line) scrollTo(line)
  }, [line, scrollTo])

  // ] and [ step between ideas; Enter opens the one you're on.
  useEffect(() => {
    const el = scroller.current
    if (!el) return
    const key = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return
      if ((e.target as HTMLElement).closest('textarea, input, .nbox')) return
      if ((e.key === 'n' || e.key === 'N') && chosen) {
        e.preventDefault()
        startNote()
        return
      }
      const order = wants.map((w) => w.mark)
      if (!order.length) return
      const at = order.findIndex((m) => m.key === cursor)
      if (e.key === ']' || e.key === '[') {
        e.preventDefault()
        const next = order[e.key === ']' ? Math.min(at + 1, order.length - 1) : Math.max(at - 1, 0)]
        setCursor(next.key)
        scrollTo(next.mark.span?.line ?? next.mark.start)
      } else if (e.key === 'Enter' && at >= 0) {
        e.preventDefault()
        open(order[at])
      }
    }
    el.addEventListener('keydown', key)
    return () => el.removeEventListener('keydown', key)
  }, [scroller, wants, cursor, open, scrollTo, chosen, startNote])

  return (
    <div className={`fv-body${wide ? (over ? ' over' : ' wide') : ''}`}>
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
      {wide && <Margin wants={wants} lit={lit ?? cursor ?? undefined} open={open} drop={over ? (view.current?.defaultLineHeight ?? 24) : 0} />}
      {[...slots.map.entries()].map(([key, el]) => {
        if (key === 'draft') {
          return draft ? createPortal(<NoteBox path={doc.path} start={draft.start} end={draft.end} ideas={ideasOn(doc.marks, draft.start, draft.end)} close={() => setDraft(null)} />, el, key) : null
        }
        const note = shownNotes.find((n) => `n:${n.id}` === key)
        return note ? createPortal(<NoteWell note={note} />, el, key) : null
      })}
    </div>
  )
}
