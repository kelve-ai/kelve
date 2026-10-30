// A file read plainly: what it's for, who uses it, and what the change open did
// to it, in a line each; then how it works, every part in the order things
// happen, each marked as the change left it, with the ideas it rests on woven
// into its sentence, what it did before, and its code, or a section's own
// words, a click away, in the lines around it. Set-up is one line at the end.
// Its rows are every plain page's (PlainRows). Your agent writes it as it
// reads; until then, where it is on your agent's list, with the code or text
// meanwhile.

import { useMemo, useRef, useState, type ReactNode } from 'react'
import { plainWhere, useList } from '../agentList'
import type { Marked, MarksSpec, NoteRef, Opened, PlainPart, PlainlyView, ReadAs } from '../api/types'
import { Loader } from '../Loader'
import { READ_AS } from '../screens/Marks'
import { Choice } from '../ui/ui'
import { useLoad } from '../useLoad'
import { Code, type Doc } from './FileView'
import { WaitingLine } from './ListPane'
import { useNotes } from './NoteParts'
import { useSeenPlainly } from './seen'
import { UsedLine, useLessons, usedOn } from './Lessons'
import { AlsoLine, GroupHead, KindLine, listed, PartRow, Woven } from './PlainRows'
import { Prose } from './Prose'

/** How a file opens, as you chose in What kelve marks: as its language's area says, else as files read overall. */
export function opensAs(spec: MarksSpec | undefined, lang: string | null): ReadAs {
  if (!spec) return 'plainly'
  return spec.areas.find((a) => a.area.trim().toLowerCase() === lang)?.read ?? spec.read
}

/** The switch on a file's name line: for this visit, whatever you chose. */
export function ReadSwitch({ value, set }: { value: ReadAs; set: (r: ReadAs) => void }) {
  return <Choice small label="Read it" value={value} onChange={set} options={[READ_AS[1], READ_AS[0]]} />
}

/** Lines of context either side of a part's own, when its code is shown. */
const AROUND = 4

/** The ideas marked on a part's lines, each once, in the order they come. */
function ideasIn(marks: Marked[], from: number, to: number): Marked[] {
  const by = new Map<string, Marked>()
  const first = (m: Marked) => m.mark.span?.line ?? m.mark.start
  const last = (m: Marked) => m.mark.span?.line ?? m.mark.end
  for (const m of [...marks].sort((a, b) => first(a) - first(b))) {
    if (first(m) <= to && last(m) >= from && !by.has(m.key)) by.set(m.key, m)
  }
  return [...by.values()]
}

export function Plainly({ opened, doc, name, lit, open, lesson, openPath, next, meanwhile, notes }: {
  opened: Opened
  doc: Doc
  /** The notes on lines of the file: shown with its parts' lines, as the file shows them. */
  notes: NoteRef[]
  /** The file's name line, with the switch on it. */
  name: ReactNode
  /** The idea whose entry is open: the part its lines are in is tinted. */
  lit?: string
  open: (key: string) => void
  /** Opens a lesson, beside the work. */
  lesson?: (id: string) => void
  /** Opens a file, or a folder (its path ends in `/`), beside it. */
  openPath?: (path: string) => void
  /** What to read next, when you're reading in order. */
  next?: ReactNode
  /** While it isn't written: the file as code or text, with this at its head. */
  meanwhile: (lead: ReactNode) => ReactNode
}) {
  const lessons = useLessons(opened.id, doc.path).data
  const view = useLoad<PlainlyView>(`/api/opened/${opened.id}/plainly?path=${encodeURIComponent(doc.path)}`, ['plainly', 'asks'], (e) => e.type !== 'plainly' || e.opened === opened.id)
  const list = useList()
  const agent = useNotes()?.agent ?? 'your agent'
  const Agent = agent.charAt(0).toUpperCase() + agent.slice(1)
  const v = view.data
  useSeenPlainly(doc.path, !!v?.what)
  // Kept from before as not written, it may be written now: nothing but its
  // name until it's asked again, so the code never shows only to give way.
  if (v && !v.parts && !view.fresh && !view.error) {
    return (
      <div className="fv">
        <div className="fv-scroll" tabIndex={-1}>
          <div className="fv-in">{name}</div>
        </div>
      </div>
    )
  }
  if (v && !v.parts) {
    return meanwhile(
      <div className="pl-waiting">
        <WaitingLine page={doc.path} where={plainWhere(list, v.waiting, opened.id)} working={!!list?.working} agent={Agent} writing={!!v.waiting?.writing} reading={!!v.waiting?.reading} />
      </div>,
    )
  }
  let body: ReactNode
  if (view.error) body = <p className="fv-say">{view.error}</p>
  else if (!v?.parts) body = <p className="fv-say"><Loader size="small" label="Loading it read plainly" /></p>
  else body = <Page v={v} parts={v.parts} doc={doc} notes={notes} lit={lit} open={open} openPath={openPath} used={lesson && <UsedLine lessons={usedOn(lessons, [doc.path])} open={lesson} />} />
  return (
    <div className="fv">
      <div className="fv-scroll" tabIndex={-1}>
        <div className="fv-in">
          {name}
          {body}
          {v?.parts && next}
        </div>
      </div>
    </div>
  )
}

function Page({ v, parts, doc, notes, lit, open, openPath, used }: {
  v: PlainlyView
  notes: NoteRef[]
  /** The lessons from the shared graph used on it. */
  used?: ReactNode
  parts: PlainPart[]
  doc: Doc
  lit?: string
  open: (key: string) => void
  openPath?: (path: string) => void
}) {
  const rows = parts.filter((p) => !p.also)
  const also = parts.filter((p) => p.also)
  const change = v.change
  const n = rows.length
  const at = (p: string) =>
    openPath ? <button key={p} type="button" className="plink mono" onClick={() => openPath(p)}>{p}</button> : <span key={p} className="mono">{p}</span>
  return (
    <div className={`pl${change ? '' : ' unmarked'}`}>
      {v.what && <p className="lede"><Woven text={v.what} ideas={v.ideas} idea={open} /></p>}
      {v.usedBy.length > 0 && <p className="used">Used by {listed(v.usedBy.map(at))}.</p>}
      {change && (change.says || change.movedFrom) && (
        <KindLine kind={change.kind}>
          {change.says && <Woven text={change.says} ideas={v.ideas} idea={open} />}
          {change.movedFrom && <>{change.says && ' '}Moved from <span className="mono">{change.movedFrom}</span>.</>}
        </KindLine>
      )}
      {used}
      <section aria-label={v.code ? 'How it works' : 'What it says'}>
        <GroupHead
          name={v.code ? 'How it works' : 'What it says'}
          count={v.code ? `${n} ${n === 1 ? 'part' : 'parts'}${n > 1 ? ', in the order they happen' : ''}` : `${n} ${n === 1 ? 'section' : 'sections'}`}
        />
        {rows.map((p, i) => (
          <Part key={`${p.from}-${p.to}`} doc={doc} notes={notes} part={p} at={i + 1} code={v.code} marked={!!change} lit={lit} open={open} />
        ))}
        {v.out.map((o, i) => (
          <PartRow key={`out-${i}`} mark="takenOut" goal={o.goal} says={o.says} ideas={o.ideas} idea={open} />
        ))}
      </section>
      {also.length > 0 && (
        <AlsoLine name="Also here">
          {also.map((p, i) => (
            <span key={`${p.from}-${p.to}`}>
              {i > 0 && ' '}
              <Woven text={p.goal} ideas={p.ideas} idea={open} />: <Woven text={p.says} ideas={p.ideas} idea={open} />
            </span>
          ))}
        </AlsoLine>
      )}
    </div>
  )
}

function Part({ doc, notes, part, at, code, marked, lit, open }: {
  doc: Doc
  notes: NoteRef[]
  part: PlainPart
  /** Its place among the parts: a text's sections are numbered. */
  at: number
  code: boolean
  /** In a change: each part is marked as the change left it. */
  marked: boolean
  lit?: string
  open: (key: string) => void
}) {
  const [shown, setShown] = useState(false)
  const ideas = useMemo(() => ideasIn(doc.marks, part.from, part.to), [doc.marks, part.from, part.to])
  const lines = part.from === part.to ? `${part.from}` : `${part.from}–${part.to}`
  const what = code ? 'code' : 'text'
  return (
    <PartRow
      mark={marked ? part.mark : null}
      step={marked ? undefined : at}
      goal={part.goal}
      says={part.says}
      ideas={part.ideas}
      before={part.before}
      idea={open}
      at={code || !marked ? lines : `§${at}`}
      atTitle={code || !marked ? undefined : `Lines ${lines}`}
      lit={!!lit && ideas.some((m) => m.key === lit)}
    >
      {shown && (code ? <PartCode doc={doc} notes={notes} from={part.from} to={part.to} lit={lit} open={open} /> : <PartText doc={doc} notes={notes} from={part.from} to={part.to} lit={lit} open={open} />)}
      <button type="button" className="quiet-link showc" aria-expanded={shown} onClick={() => setShown(!shown)}>
        {shown ? `Hide the ${what}` : `Show the ${what}`}
      </button>
    </PartRow>
  )
}

/**
 * A part's lines of the document, with the lines around them: up to
 * `AROUND` of the file's lines either side, and whatever a change took out
 * among them, where it was. The lines a change took out just before the
 * part's first are the part's own.
 */
function sliceOf(doc: Doc, from: number, to: number) {
  const all = doc.text.split('\n')
  const gone = (n: number) => doc.kinds?.[n - 1] === 'removed'
  const clamp = (n: number) => Math.max(1, Math.min(n, all.length))
  let first = clamp(doc.lineOf(from) ?? from)
  const last = clamp(doc.lineOf(to) ?? to)
  while (first > 1 && gone(first - 1)) first -= 1
  let a = first
  for (let kept = 0; a > 1 && (kept < AROUND || gone(a - 1)); a--) if (!gone(a - 1)) kept++
  let b = last
  for (let kept = 0; b < all.length && kept < AROUND; b++) if (!gone(b + 1)) kept++
  b = Math.max(a, b)
  return {
    a,
    b,
    first,
    last,
    text: all.slice(a - 1, b).join('\n'),
    kinds: doc.kinds?.slice(a - 1, b),
    numbers: doc.numbers ? doc.numbers.slice(a - 1, b) : Array.from({ length: b - a + 1 }, (_, i) => String(doc.fileLine(a + i) ?? '')),
  }
}

/** What the file view needs to be told that a part's lines don't use: where ideas sit, for a ruler they have none of. */
const unplaced = () => {}

/**
 * A part's code, as the file view draws code: read only, numbered, quietly
 * coloured, with the ideas' squares and each idea's name and line under
 * its lines, a change's + and − and the lines it took out, the notes on
 * its lines, and lines chosen to note; the lines around it quieter.
 */
function PartCode({ doc, notes, from, to, lit, open }: { doc: Doc; notes: NoteRef[]; from: number; to: number; lit?: string; open: (key: string) => void }) {
  const none = useRef<HTMLDivElement>(null)
  const { part, quiet } = useMemo(() => {
    const slice = sliceOf(doc, from, to)
    const part: Doc = {
      ...doc,
      text: slice.text,
      kinds: slice.kinds,
      numbers: slice.numbers,
      lineOf: (n) => {
        const d = doc.lineOf(n)
        return d !== undefined && d >= slice.a && d <= slice.b ? d - slice.a + 1 : undefined
      },
      fileLine: (n) => doc.fileLine(slice.a + n - 1),
    }
    const quiet: number[] = []
    for (let d = slice.a; d <= slice.b; d++) if (d < slice.first || d > slice.last) quiet.push(d - slice.a + 1)
    return { part, quiet }
  }, [doc, from, to])
  const opening = (m: Marked) => open(m.key)
  return (
    <div className="pcode">
      <Code doc={part} wide={false} width={0} lit={lit} open={opening} scroller={none} placed={unplaced} notes={notes} whole quiet={quiet} />
    </div>
  )
}

/**
 * A section's own words, as the reader draws text: the ideas' squares and
 * each idea's name and line under its block, the notes on its lines, and
 * lines chosen to note.
 */
function PartText({ doc, notes, from, to, lit, open }: { doc: Doc; notes: NoteRef[]; from: number; to: number; lit?: string; open: (key: string) => void }) {
  const none = useRef<HTMLDivElement>(null)
  const part = useMemo<Doc>(() => {
    // The file as the change left it: its lines, without those taken out,
    // with blank lines before the section's own, so each of its blocks
    // keeps the line it has in the file, as its ideas and notes do.
    const now = doc.text.split('\n').filter((_, i) => doc.kinds?.[i] !== 'removed')
    const text = '\n'.repeat(Math.max(0, from - 1)) + now.slice(Math.max(0, from - 1), to).join('\n')
    const on = (m: Marked) => (m.mark.span?.line ?? m.mark.start) >= from && (m.mark.span?.line ?? m.mark.start) <= to
    return { ...doc, text, kinds: undefined, numbers: undefined, marks: doc.marks.filter(on) }
  }, [doc, from, to])
  const opening = (m: Marked) => open(m.key)
  return (
    <div className="ptext pv-part">
      <Prose doc={part} wide={false} lit={lit} open={opening} scroller={none} placed={unplaced} notes={notes} />
    </div>
  )
}
