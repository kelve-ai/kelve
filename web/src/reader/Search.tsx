// Search (boards X-6, L-5): what's newly written for you first, then ideas
// here, then those from the shared graph that aren't in your code, where
// opening one with no entry keeps it for your agent; then lessons about the
// work, files, lines that say the words, and notes; then what you can do.
// Before you type, this work's ideas, those in the most files first, and its
// lessons. ⌘K or Ctrl K opens it; the arrows choose, Enter opens, Escape
// closes. Each kind is a group, with how many; each result a line; the letters
// that matched in kelve's red.

import { Fragment, useEffect, useMemo, useRef, useState } from 'react'
import { get } from '../api/client'
import type { Found, FoundLine, LessonView, NoteRef, Opened, ReadyItem } from '../api/types'
import { LessonSquare } from './Lessons'
import { Square } from './Square'

/** Something the reader can do, by its name. */
export type Command = { name: string; run: () => void }

type Idea = { key: string; name: string; definition: string; ready: boolean; files: number | null }

type Hit =
  | { kind: 'written'; key: string; name: string }
  | ({ kind: 'here' } & Idea)
  | ({ kind: 'graph' } & Idea)
  | { kind: 'file'; path: string }
  | ({ kind: 'line' } & FoundLine)
  | { kind: 'note'; note: NoteRef }
  | { kind: 'lesson'; lesson: LessonView }
  | { kind: 'command'; command: Command }

const WHERE: Record<NoteRef['place'], string> = { you: 'Just you', work: 'With the work', agent: 'For your agent' }

/** Opens search on ⌘K or Ctrl K. */
export function useSearchKey(open: () => void) {
  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        open()
      }
    }
    window.addEventListener('keydown', on)
    return () => window.removeEventListener('keydown', on)
  }, [open])
}

/** Words, with what matched the search highlighted. */
function Lit({ text, q }: { text: string; q: string }) {
  const at = q ? text.toLowerCase().indexOf(q.toLowerCase()) : -1
  if (at < 0) return <>{text}</>
  return (
    <>
      {text.slice(0, at)}
      <u>{text.slice(at, at + q.length)}</u>
      {text.slice(at + q.length)}
    </>
  )
}

/** The most of this work's ideas shown before you type: typing finds the rest. */
const MOST_IDEAS = 8
/** The most lessons shown before you type. */
const MOST_LESSONS = 3

/** A lesson's Do, as plain words. */
const said = (l: LessonView) => l.do.replace(/`/g, '')

export function Search({ initial = '', opened, commands, close, onIdea, ideas = [], written = [], lessons = [], onLesson, onFile, onLine, onNote }: {
  /** Words to start with. */
  initial?: string
  opened: Opened
  commands: Command[]
  close: () => void
  onIdea: (key: string) => void
  /** This work's ideas, those in the most files first, with how many files each is in. */
  ideas?: Array<{ key: string; name: string; files: number }>
  /** Entries you asked for in this lately, written now: newly written, first. */
  written?: ReadyItem[]
  /** The lessons about the ideas in the work. */
  lessons?: LessonView[]
  onLesson?: (id: string) => void
  onFile: (path: string) => void
  onLine: (path: string, line: number) => void
  onNote: (id: string) => void
}) {
  const [q, setQ] = useState(initial)
  const [found, setFound] = useState<Found | null>(null)
  const [on, setOn] = useState(0)
  const latest = useRef(0)

  useEffect(() => {
    const words = q.trim()
    if (words.length < 2) {
      setFound(null)
      return
    }
    const n = ++latest.current
    const timer = window.setTimeout(() => {
      get<Found>(`/api/search?q=${encodeURIComponent(words)}&opened=${opened.id}`).then(
        (f) => {
          if (n !== latest.current) return
          setFound(f)
          setOn(0)
        },
        () => {},
      )
    }, 120)
    return () => window.clearTimeout(timer)
  }, [q, opened.id])

  const words = q.trim()
  const hits = useMemo<Hit[]>(() => {
    const lower = words.toLowerCase()
    const doing = commands.filter((c) => !lower || c.name.toLowerCase().includes(lower)).map((command) => ({ kind: 'command' as const, command }))
    const teaching = (onLesson ? lessons : [])
      .filter((l) => !lower || said(l).toLowerCase().includes(lower) || l.says.toLowerCase().includes(lower) || l.about.some((a) => a.name.toLowerCase().includes(lower)))
      .map((lesson) => ({ kind: 'lesson' as const, lesson }))
    const fresh = written.filter((w) => !lower || w.name.toLowerCase().includes(lower)).map((w) => ({ kind: 'written' as const, key: w.key, name: w.name }))
    // Before you type: what's newly written for you, then what this work
    // rests on, to look around in.
    if (words.length < 2) {
      if (lower) return doing
      return [...fresh, ...ideas.slice(0, MOST_IDEAS).map((i) => ({ kind: 'here' as const, ...i, definition: '', ready: true })), ...teaching.slice(0, MOST_LESSONS), ...doing]
    }
    if (!found) return [...fresh, ...teaching, ...doing]
    return [
      ...fresh,
      ...found.ideas.filter((i) => i.files != null).map((i) => ({ kind: 'here' as const, ...i })),
      ...found.ideas.filter((i) => i.files == null).map((i) => ({ kind: 'graph' as const, ...i })),
      ...teaching,
      ...found.files.map((path) => ({ kind: 'file' as const, path })),
      ...found.lines.map((l) => ({ kind: 'line' as const, ...l })),
      ...found.notes.map((note) => ({ kind: 'note' as const, note })),
      ...doing,
    ]
  }, [found, words, commands, ideas, written, lessons, onLesson])

  const go = (h: Hit) => {
    close()
    if (h.kind === 'here' || h.kind === 'graph' || h.kind === 'written') onIdea(h.key)
    else if (h.kind === 'file') onFile(h.path)
    else if (h.kind === 'line') onLine(h.path, h.line)
    else if (h.kind === 'note') onNote(h.note.id)
    else if (h.kind === 'lesson') onLesson?.(h.lesson.id)
    else h.command.run()
  }

  const key = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') close()
    else if (e.key === 'ArrowDown') {
      e.preventDefault()
      setOn((o) => Math.min(o + 1, hits.length - 1))
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      setOn((o) => Math.max(o - 1, 0))
    } else if (e.key === 'Enter' && hits[on]) go(hits[on])
  }

  const row = (h: Hit, i: number) => {
    let mark: React.ReactNode = <span />
    let main: React.ReactNode
    let right: React.ReactNode = null
    if (h.kind === 'written') {
      mark = <Square ideaKey={h.key} name={h.name} />
      main = <Lit text={h.name} q={words} />
      right = 'Written for you'
    } else if (h.kind === 'here' || h.kind === 'graph') {
      mark = <Square ideaKey={h.key} name={h.name} />
      main = <><Lit text={h.name} q={words} />{h.definition && <i>{h.definition}</i>}</>
      right = h.kind === 'here' ? (h.files === 1 ? '1 file' : h.files ? `${h.files} files` : null) : h.ready ? null : 'Not written yet'
    } else if (h.kind === 'file') {
      const at = h.path.lastIndexOf('/')
      main = <span className="mono"><Lit text={h.path.slice(at + 1)} q={words} /></span>
      right = at > 0 ? <span className="mono"><Lit text={h.path.slice(0, at)} q={words} /></span> : null
    } else if (h.kind === 'line') {
      main = <span className="mono"><Lit text={h.text.trim()} q={words} /></span>
      right = <span className="mono">{h.path}:{h.line}</span>
    } else if (h.kind === 'lesson') {
      mark = <LessonSquare />
      main = <Lit text={said(h.lesson)} q={words} />
      right = h.lesson.about.map((a) => a.name).join(', ')
    } else if (h.kind === 'note') {
      main = <span title={h.note.file}><Lit text={h.note.title} q={words} /></span>
      right = WHERE[h.note.place]
    } else {
      mark = <span className="glyph" aria-hidden="true">+</span>
      main = <Lit text={h.command.name} q={words} />
    }
    return (
      <li key={i} className={i === on ? 'on' : undefined} onMouseEnter={() => setOn(i)}>
        <button type="button" id={`hit-${i}`} onClick={() => go(h)}>
          {mark}
          <span className="main">{main}</span>
          <span className="r">{right}</span>
        </button>
      </li>
    )
  }

  const group = (title: string, kinds: Hit['kind'][]) => {
    const these = hits.map((h, i) => [h, i] as const).filter(([h]) => kinds.includes(h.kind))
    if (!these.length) return null
    return (
      <section className="grp" aria-label={title}>
        <h3 className="lgroup"><b>{title}</b><span>{these.length}</span></h3>
        <ul>{these.map(([h, i]) => <Fragment key={i}>{row(h, i)}</Fragment>)}</ul>
      </section>
    )
  }

  return (
    <div className="veil" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className="pal" role="dialog" aria-modal="true" aria-label="Search">
        <div className="in">
          <label htmlFor="search-q" className="sr-only">Search ideas, lessons, files, what they say, and notes</label>
          <input
            id="search-q"
            type="search"
            autoFocus
            autoComplete="off"
            spellCheck={false}
            placeholder="Search ideas, lessons, files, what they say, and notes"
            value={q}
            aria-activedescendant={hits[on] ? `hit-${on}` : undefined}
            onChange={(e) => { setQ(e.target.value); setOn(0) }}
            onKeyDown={key}
          />
        </div>
        {group('Newly written', ['written'])}
        {group('Ideas here', ['here'])}
        {group('From the shared graph', ['graph'])}
        {group('Lessons', ['lesson'])}
        {group('Files', ['file'])}
        {group('In the files', ['line'])}
        {group('Notes', ['note'])}
        {group('Do', ['command'])}
        {words.length >= 2 && found && !hits.length && <p className="none">Nothing here says that.</p>}
        <p className="ft">
          <span>↑↓ to choose</span>
          <span>Enter to open</span>
          <span>Esc to close</span>
        </p>
      </div>
    </div>
  )
}
