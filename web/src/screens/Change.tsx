// A debrief's change, file by file (board ChangeView), or work handed over
// that has no debrief, the same way, from what was kept of it: its files by folder
// down the side, to find and step through; the file, whole, through the
// reader's CodeMirror (reader/code.ts), what it left as it was folded away;
// and each decision in the margin beside its first line, with where it
// stands. Lines the person changed after handover are theirs.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { EditorState, RangeSetBuilder, Text as Lines } from '@codemirror/state'
import { Decoration, EditorView, lineNumbers, WidgetType } from '@codemirror/view'
import type { DebriefChangeView, DebriefDiffView, DecisionStatus, DiffNote } from '../api/types'
import { exampleChange, exampleDiff } from '../example/debriefs'
import { asBefore, colours, diffMarks, folding, foldOf, foldRuns, grammar, lineClasses, parsed, unfoldAt } from '../reader/code'
import { hrefOf } from '../routes'
import { useLoad } from '../useLoad'

const STATUS: Record<DecisionStatus, string> = {
  yourCall: 'Your call',
  agreed: 'Agreed',
  redoing: 'Redoing',
  researching: 'Researching',
  researchReady: 'Research ready',
}

const fileParam = () => new URLSearchParams(location.hash.split('?')[1] ?? '').get('file')
const plus = (n: number) => (n ? `+${n.toLocaleString('en-GB')}` : '')
const minus = (n: number) => (n ? `−${n.toLocaleString('en-GB')}` : '')

export function Change({ id, example, handover }: {
  id: string
  example?: boolean
  /** Work handed over with no debrief: `id` is the handover's, and there are no decisions. */
  handover?: boolean
}) {
  const local = example || id === 'example'
  const from = handover ? `/api/handovers/${id}` : `/api/debriefs/${id}`
  const loaded = useLoad<DebriefChangeView>(local ? null : `${from}/change`, ['debriefs'])
  const change = local ? exampleChange(id) : loaded.data
  const files = useMemo(() => change?.folders.flatMap((f) => f.files) ?? [], [change])
  const [find, setFind] = useState('')
  const [chosen, setChosen] = useState<string | null>(fileParam)
  const path = chosen ?? files.find((f) => f.decisions.length)?.path ?? files[0]?.path ?? null
  const diff = useLoad<DebriefDiffView>(local || !path ? null : `${from}/diff?path=${encodeURIComponent(path)}`, ['debriefs'])
  const file = local && path ? exampleDiff(path) : diff.data?.path === path ? diff.data : undefined

  const pick = (p: string) => {
    setChosen(p)
    const base = location.hash.split('?')[0]
    history.replaceState(null, '', `${base}?file=${encodeURIComponent(p)}`)
  }
  const at = files.findIndex((f) => f.path === path)
  const step = (by: number) => files.length && pick(files[(at + by + files.length) % files.length].path)

  if (!change) {
    if (loaded.error) return <p className="cv-error" role="alert">{loaded.error}</p>
    return <div className="cv" aria-busy="true" />
  }
  const words = find.trim().toLowerCase()
  return (
    <div className="cv">
      <div className="cv-head">
        <div className="cv-title">
          {handover ? <a className="go" href={hrefOf('', example)}>← Back to home</a> : <a className="go" href={hrefOf(`debrief/${id}`, example)}>← Back to the debrief</a>}
          <h1>{change.title}</h1>
        </div>
        <span className="cv-counts">{change.files === 1 ? '1 file' : `${change.files} files`} <span className="add">{plus(change.added)}</span> <span className="del">{minus(change.removed)}</span></span>
      </div>
      <div className="cv-body">
        <nav className="cv-nav" aria-label="Files">
          <label className="sr-only" htmlFor="cv-find">Find a file</label>
          <input id="cv-find" className="field cv-find" placeholder="Find a file" value={find} onChange={(e) => setFind(e.target.value)} />
          {change.folders.map((g) => {
            const shown = g.files.filter((f) => !words || f.path.toLowerCase().includes(words))
            if (!shown.length) return null
            const prefix = g.name === 'the top folder' ? '' : g.name
            return (
              <div className="cv-group" key={g.name}>
                <span className="cv-folder">{g.name}</span>
                {shown.map((f) => (
                  <button key={f.path} type="button" className="cv-file" aria-current={f.path === path} onClick={() => pick(f.path)}>
                    <span className={`cv-file-n${f.yours ? ' yours' : ''}`}>{f.path.startsWith(prefix) ? f.path.slice(prefix.length) : f.path}</span>
                    <span className="cv-file-c"><span className="add">{plus(f.added)}</span> <span className="del">{minus(f.removed)}</span></span>
                  </button>
                ))}
              </div>
            )
          })}
        </nav>
        <section className="cv-file-view" aria-label={path ?? 'The file'}>
          <div className="cv-file-head">
            <div className="cv-file-name">
              <span className="mono">{path}</span>
              {file && <span className={`cv-kind ${file.kind.replace(' ', '-')}`}>{file.kind}</span>}
            </div>
            <div className="cv-steps">
              <button type="button" className="btn btn-s btn-sm" onClick={() => step(-1)}>Previous file</button>
              <button type="button" className="btn btn-s btn-sm" onClick={() => step(1)}>Next file</button>
            </div>
          </div>
          {file && !file.shown && <p className="cv-quiet">kelve doesn’t show this file: it isn’t text, or it holds a secret.</p>}
          {file && file.shown && <DiffCode key={file.path} file={file} id={id} example={example} />}
          {diff.error && <p className="cv-quiet" role="alert">{diff.error}</p>}
        </section>
      </div>
    </div>
  )
}

/** The note for a decision, in the margin or over the code. */
function Note({ note, id, example, top }: { note: DiffNote; id: string; example?: boolean; top?: number }) {
  return (
    <a className="cv-note" href={`${hrefOf(`debrief/${id}`, example)}`} style={top === undefined ? undefined : { top }}>
      <span className="cv-note-t">{note.n}. {note.title}</span>
      <span className={`db-status ${note.status}`}>{STATUS[note.status]}</span>
    </a>
  )
}

/** Lines kelve didn't keep, as one quiet line in their place. */
class Kept extends WidgetType {
  constructor(readonly n: number) {
    super()
  }
  eq(o: Kept) {
    return o.n === this.n
  }
  toDOM() {
    const el = document.createElement('span')
    el.className = 'cv-fold'
    const count = document.createElement('b')
    count.textContent = `${this.n} lines`
    el.append(count, ' as before')
    return el
  }
}

/** Each fold's line, drawn as how many lines it stands for. */
function folds(doc: Lines, lines: DebriefDiffView['lines']) {
  const b = new RangeSetBuilder<Decoration>()
  lines.forEach((l, i) => {
    if (l.kind !== 'fold') return
    const line = doc.line(i + 1)
    b.add(line.from, line.to, Decoration.replace({ widget: new Kept(l.folded) }))
  })
  return EditorView.decorations.of(b.finish())
}

/** Below this width, the margin's notes go above the code. */
const MARGIN_FROM = 900

/** A file, whole, as one document: the lines taken out where they were, what's as before folded. */
function DiffCode({ file, id, example }: { file: DebriefDiffView; id: string; example?: boolean }) {
  const host = useRef<HTMLDivElement>(null)
  const view = useRef<EditorView | null>(null)
  const [tops, setTops] = useState<number[]>([])
  const [wide, setWide] = useState(() => window.innerWidth >= MARGIN_FROM)
  useEffect(() => {
    const on = () => setWide(window.innerWidth >= MARGIN_FROM)
    window.addEventListener('resize', on)
    return () => window.removeEventListener('resize', on)
  }, [])

  const doc = useMemo(() => {
    const kinds = file.lines.map((l) => (l.kind === 'added' || l.kind === 'yours' ? 'added' : l.kind === 'removed' ? 'removed' : undefined))
    const text = file.lines.map((l) => (l.kind === 'fold' ? '…' : l.text)).join('\n')
    const numbers = file.lines.map((l) => (l.n == null ? '' : String(l.n)))
    const classes = new Map<number, string>()
    file.lines.forEach((l, i) => {
      if (l.kind === 'yours') classes.set(i + 1, 'cm-added cm-yours')
      else if (l.kind === 'added') classes.set(i + 1, 'cm-added')
      else if (l.kind === 'removed') classes.set(i + 1, 'cm-gone')
      else if (l.kind === 'fold') classes.set(i + 1, 'cm-cut')
    })
    return { kinds, text, numbers, classes }
  }, [file])

  const measure = useCallback(() => {
    const v = view.current
    if (!v) return
    let last = -Infinity
    const next = file.notes.map((n) => {
      const line = Math.min(Math.max(n.at, 1), v.state.doc.lines)
      const top = Math.max(v.lineBlockAt(v.state.doc.line(line).from).top, last + 8)
      last = top + 44
      return top
    })
    setTops((was) => (was.length === next.length && was.every((t, i) => Math.abs(t - next[i]) < 1) ? was : next))
  }, [file.notes])
  const measureRef = useRef(measure)
  measureRef.current = measure

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
          EditorView.contentAttributes.of({ 'aria-label': `${file.path}, read only`, tabindex: '0' }),
          lineNumbers({ formatNumber: (n) => doc.numbers[n - 1] ?? '' }),
          folding,
          diffMarks(doc.kinds),
          lineClasses(lines, doc.classes),
          folds(lines, file.lines),
          grammar(file.lang),
          colours,
          EditorView.updateListener.of((u) => {
            if (u.geometryChanged || u.heightChanged) measureRef.current()
          }),
        ],
      })),
    })
    view.current = v
    // What's as before, folded; but never a decision's own lines.
    const runs = asBefore(doc.kinds.map((k, i) => (file.lines[i].kind === 'fold' ? 'removed' : k)))
    v.dispatch({ effects: foldRuns.of(runs) })
    const opening = file.notes.flatMap((n) => (foldOf(v.state, n.at) ? unfoldAt(v.state, n.at) : []))
    if (opening.length) v.dispatch({ effects: opening })
    void document.fonts?.ready.then(() => v.requestMeasure())
    measureRef.current()
    return () => {
      v.destroy()
      view.current = null
    }
  }, [doc, file])

  useLayoutEffect(() => measure(), [measure, wide])

  return (
    <div className={`fv cv-code${wide ? ' wide' : ''}`}>
      {!wide && file.notes.length > 0 && (
        <div className="cv-notes-above">{file.notes.map((n) => <Note key={n.n} note={n} id={id} example={example} />)}</div>
      )}
      <div className="cv-lines" ref={host} />
      {wide && (
        <div className="cv-margin" aria-label="Decisions in this file">
          {file.notes.map((n, i) => <Note key={n.n} note={n} id={id} example={example} top={tops[i] ?? 0} />)}
        </div>
      )}
    </div>
  )
}
