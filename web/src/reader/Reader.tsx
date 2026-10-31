// The reader (boards X-1 to X-6, M-2 to M-4, H-5 to H-7): one way to read
// anything opened. One bar: the title, what your agent said, what's ready for
// you, and your agent's line with how far reading has got; then the side panel,
// with what the squares mean and What kelve marks at its foot; the file, with
// each idea in the margin beside its lines, or a folder's page, or a change's
// own page, which a change opens on, each read plainly; and entries, opened as
// panes beside the work. The address holds what it shows, a change's page when
// it names no file or folder: #/o/<id>?f=<file>&l=<line>&m=<idea to
// show>&p=<idea>&p=~<idea followed>&t=<tab>&w=all &n=<note, or new>&na=<the
// idea a new note is about>&d=<folder whose page shows>

import { Component, type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { ChangeView, FolderView, ListItem, NoteRef, Opened, Paused, Place, ReadAs, ReadyItem, ServerEvent, Settings, WayPart } from '../api/types'
import { ListContext, OpenContext, useAgentList } from '../agentList'
import { Loader, Loading } from '../Loader'
import { rememberOpen } from '../lastOpen'
import { setTheme, shownTheme } from '../theme'
import { Top } from '../Top'
import { fetchAhead, fetchNow, isKept, useHeld, useLoad } from '../useLoad'
import { agentKindName, useNarrow } from '../words'
import { Hint, State } from '../ui/ui'
import { post } from '../api/client'
import { ChangePage } from './ChangePage'
import { fetchFile, fileAhead, FileView, fileIn } from './FileView'
import { FolderPage, type Part } from './FolderPage'
import { NotePage } from './NotePage'
import { NotesContext, type NotesWith, NotesTab } from './NoteParts'
import { isLesson, LESSON, useLessons } from './Lessons'
import { paneAddress, Panes } from './Panes'
import { type Command, Search, useSearchKey } from './Search'
import { type SideFile, Side, type Tab } from './Side'
import { type Seen, SeenContext, Square } from './Square'
import { AgentLine, ListPane } from './ListPane'
import { Grip, usePaneWidths, useWindowWidth, WORK_LEAST } from './widths'
import { MARKS, MarkedForYou } from '../screens/Marks'
import { SeenPlainly } from './seen'

/** How long a file shows before the next in order is looked at, so it doesn't hold up this one. */
const AHEAD_AFTER = 300

type Step = { key: string; followed: boolean }
type At = { f: string | null; l?: number; m: string | null; p: Step[]; t: Tab; w: 'some' | 'all'; n: string | null; na: string | null; np: Place | null; d: string | null; k: 'list' | 'marks' | null; r: ReadAs | null }

/** The address, read; the tab is `first` unless it says another. */
function read(first: Tab = 'files'): At {
  const q = new URLSearchParams(location.hash.split('?')[1] ?? '')
  const l = Number(q.get('l'))
  const t = q.get('t')
  return {
    f: q.get('f'),
    l: l > 0 ? l : undefined,
    m: q.get('m'),
    p: q.getAll('p').map((k) => (k.startsWith('~') ? { key: k.slice(1), followed: true } : { key: k, followed: false })),
    t: t === 'order' || t === 'files' || t === 'notes' ? t : first,
    w: q.get('w') === 'all' ? 'all' : 'some',
    n: q.get('n'),
    na: q.get('na'),
    np: q.get('np') === 'agent' ? 'agent' : null,
    k: q.get('k') === 'list' ? 'list' : q.get('k') === 'marks' ? 'marks' : null,
    d: q.has('d') ? (q.get('d') ?? '') : null,
    r: q.get('r') === 'plainly' ? 'plainly' : q.get('r') === 'code' ? 'code' : null,
  }
}

/** What the reader shows, kept in the address so it survives a reload, and Back goes back. */
function useAt(base: string, first: Tab): [At, (next: Partial<At>, replace?: boolean) => void] {
  const [at, setAt] = useState(() => read(first))
  useEffect(() => {
    const on = () => setAt(read(first))
    on()
    window.addEventListener('hashchange', on)
    return () => window.removeEventListener('hashchange', on)
  }, [first])
  const go = useCallback(
    (next: Partial<At>, replace = false) => {
      const was = read(first)
      const all = { ...was, ...next }
      // How a file reads by its switch is for that visit: another file reads as you chose.
      if (next.f !== undefined && next.f !== was.f && next.r === undefined) all.r = null
      const q = new URLSearchParams()
      if (all.f) q.set('f', all.f)
      if (all.f && all.r) q.set('r', all.r)
      if (all.f && all.l) q.set('l', String(all.l))
      if (all.d != null && !all.f) q.set('d', all.d)
      if (all.m) q.set('m', all.m)
      for (const s of all.p) q.append('p', s.followed ? `~${s.key}` : s.key)
      if (all.t !== first) q.set('t', all.t)
      if (all.w === 'all') q.set('w', 'all')
      if (all.n) q.set('n', all.n)
      if (all.n && all.na) q.set('na', all.na)
      if (all.n === 'new' && all.np) q.set('np', all.np)
      if (all.k && !all.n) q.set('k', all.k)
      const hash = `${base}${q.size ? `?${q}` : ''}`
      if (replace) {
        history.replaceState(null, '', hash)
        setAt(read(first))
      } else location.hash = hash
    },
    [base, first],
  )
  return [at, go]
}

/**
 * The steps of the way in you've read, as kelve keeps them with what's
 * open: a step is read once its plainly version is seen, with what it
 * says (`seen.ts`), never only opened. It shows as read once you leave
 * it, and is kept, unless it's the app's example. The second is what to
 * tell when something's seen plainly.
 */
function useStepsRead(opened: Opened, here: string | null, onPath: (path: string) => boolean, example?: boolean): [(path: string) => boolean, (path: string) => void] {
  const [now, setNow] = useState<Set<string>>(() => new Set())
  const kept = opened.way?.readSteps
  const seen = useCallback(
    (path: string) => {
      // The overview is the step "".
      if (!onPath(path) || now.has(path) || kept?.includes(path)) return
      setNow((n) => new Set(n).add(path))
      if (!example) void post(`/api/opened/${opened.id}/way/step`, { path }).catch(() => {})
    },
    [onPath, now, kept, opened.id, example],
  )
  const read = useCallback((path: string) => path !== here && (now.has(path) || !!kept?.includes(path)), [now, kept, here])
  return [read, seen]
}

/** At the first step of a part of the path, what the part is and how long: "How a file is read, 1 of 31". */
function partStarting(parts: WayPart[], path: string): string | undefined {
  const part = parts.find((p) => p.steps[0]?.path === path)
  return part && `${part.name}, 1 of ${part.steps.length}`
}

/** What's open while your agent finds a way in (Read-1): how much there is, and that it's being found. */
function Finding({ title, files }: { title: string; files?: Array<{ path: string }> }) {
  // Its top folders, each with how many files it holds, then its files at the top.
  const tops = useMemo(() => {
    const folders = new Map<string, number>()
    const loose: string[] = []
    for (const f of files ?? []) {
      const [top, ...rest] = f.path.split('/')
      if (rest.length) folders.set(top, (folders.get(top) ?? 0) + 1)
      else loose.push(top)
    }
    return { folders: [...folders].sort(([a], [b]) => a.localeCompare(b)), loose }
  }, [files])
  const all = files ? new Set(files.map((f) => f.path.split('/').slice(0, -1).join('/')).filter(Boolean)).size : 0
  const count = (n: number) => `${n} ${n === 1 ? 'file' : 'files'}`
  return (
    <div className="fv">
      <div className="fv-scroll" tabIndex={-1}>
        <div className="fv-in">
          <div className="pl">
            <p className="fnm2">{title}</p>
            {files && (
              <>
                <p className="lede">
                  {count(files.length)}
                  {all > 0 && <> in {all} {all === 1 ? 'folder' : 'folders'}</>}.
                </p>
                <ul className="tops" aria-label="What’s in it">
                  {tops.folders.map(([name, n]) => (
                    <li key={name}><code>{name}</code><span>{count(n)}</span></li>
                  ))}
                  {tops.loose.length > 0 && (
                    <li><code>{tops.loose[0]}</code><span>{tops.loose.length > 1 ? `and ${count(tops.loose.length - 1).replace('file', 'more file')} at the top` : 'at the top'}</span></li>
                  )}
                </ul>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}

/** If the reader can't be drawn, it says so, rather than the page going blank. */
class Fallback extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false }
  static getDerivedStateFromError() {
    return { failed: true }
  }
  componentDidCatch(error: unknown) {
    console.warn('kelve could not draw the reader', error)
  }
  render() {
    if (this.state.failed) return <div className="reader"><Top /><Loading error="Something went wrong drawing this. Reload the page to try again." /></div>
    return this.props.children
  }
}

export function Reader({ id, example }: {
  id: number
  /** The app's own example, on the first run: read only, at #/example. */
  example?: boolean
}) {
  // Asks too: the files your agent is working through move as it works.
  const opened = useLoad<Opened>(`/api/opened/${id}`, ['opened', 'asks'])
  if (!opened.data) return <div className="reader"><Top /><Loading error={opened.error} /></div>
  return (
    <Fallback>
      <Desk opened={opened.data} example={example} />
    </Fallback>
  )
}

/** Marks that came in for what's open. */
const marksFor = (opened: number) => (e: ServerEvent) => e.type !== 'marks' || e.opened === opened
/** What's open read further: a file's marks, or a file or folder read plainly, which the count waits for too. */
const readFor = (opened: number) => (e: ServerEvent) => (e.type !== 'read' && e.type !== 'plainly') || e.opened === opened

/** The folder's name, from where it is: `~/projects/ledger` is ledger. */
const project = (root: string) => root.replace(/\/+$/, '').split('/').pop() || root

/** A popover's opener: closed by a click elsewhere, or Escape. */
function useDismiss(open: boolean, here: React.RefObject<HTMLElement | null>, close: () => void) {
  useEffect(() => {
    if (!open) return
    const away = (e: MouseEvent) => {
      if (!here.current?.contains(e.target as Node)) close()
    }
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation()
        close()
      }
    }
    document.addEventListener('mousedown', away)
    window.addEventListener('keydown', key, true)
    return () => {
      document.removeEventListener('mousedown', away)
      window.removeEventListener('keydown', key, true)
    }
  }, [open, here, close])
}

/**
 * Whether the page beside the work (a note, the list, what kelve marks)
 * was opened after the entry last opened, so it sits at its right: each
 * side view opens at the right of those already open.
 */
function useNewestRight(page: string | null, entry: string | null, entries: number): boolean {
  const [was, setWas] = useState({ page, entry, entries, last: false })
  if (was.page !== page || was.entry !== entry || was.entries !== entries) {
    // A page opened now goes after the entries open; an entry opened now
    // goes after the page; one closed leaves the order as it was.
    const opened = !!entry && entry !== was.entry && entries >= was.entries
    const last = !page ? false : page !== was.page ? !!entry : opened ? false : was.last
    setWas({ page, entry, entries, last })
    return last
  }
  return was.last
}

/** Ready for you (board H-6): what you asked for by name, written, until you've seen it: a count in the bar that opens them. */
export function ReadyKey({ ready, entry }: {
  ready: ReadyItem[]
  entry: (key: string) => void
}) {
  const [open, setOpen] = useState(false)
  const here = useRef<HTMLDivElement>(null)
  const close = useCallback(() => setOpen(false), [])
  useDismiss(open, here, close)
  if (!ready.length) return null
  return (
    <div className="ready-key" ref={here}>
      <button type="button" aria-expanded={open} aria-label={`${ready.length} ready for you`} onClick={() => setOpen((o) => !o)}>
        <State state="ready" />
        {ready.length}<span className="long"> ready</span>
      </button>
      {open && (
        <section className="drop" aria-label="Ready for you">
          <p className="lgroup"><b>Ready for you</b><span>{ready.length}</span></p>
          {ready.map((r) => (
            <button key={`e:${r.key}`} type="button" className="lrow" onClick={() => { close(); entry(r.key) }}>
              <Square ideaKey={r.key} name={r.name} />
              <span className="lt">{r.name}</span>
            </button>
          ))}
        </section>
      )}
    </div>
  )
}

/**
 * The side's key, at its foot (boards Plainly-1, Marks-1 to Marks-3): only
 * the marks the tab shows, on one line. In order, the steps you've read and
 * your agent at work; Files and Notes, the ideas' squares, and what kelve
 * marks for you a click away.
 */
function SideKey({ change, marks, order, agent }: {
  change: boolean
  /** Opens what kelve marks for you, beside the work. */
  marks: () => void
  /** The In order tab is showing. */
  order: boolean
  /** Your agent, as people say it: "Claude Code". */
  agent: string
}) {
  return (
    <div className="side-foot">
      {order ? (
        <ul className="sq-legend" aria-label="What the marks mean">
          <li><i className="lsq ready" aria-hidden="true" />To read</li>
          <li><i className="lsq" aria-hidden="true" />Read</li>
          <li><i className="wq" style={{ '--f': '66%' } as React.CSSProperties} aria-hidden="true" />{agent}</li>
        </ul>
      ) : (
        <>
          <ul className="sq-legend" aria-label="What the squares mean">
            {change && <li><i className="sq new" aria-hidden="true" />New</li>}
            <li><i className="sq" aria-hidden="true" />Known</li>
            <li><i className="sq read" aria-hidden="true" />Read</li>
          </ul>
          <button type="button" className="marked-for" onClick={marks}>What kelve marks</button>
        </>
      )}
    </div>
  )
}

/** What your agent said about what's open, a click away, beside its title in the bar. */
function Said({ said }: { said: string }) {
  const [open, setOpen] = useState(false)
  const here = useRef<HTMLDivElement>(null)
  const close = useCallback(() => setOpen(false), [])
  useDismiss(open, here, close)
  return (
    <div className="said-key" ref={here}>
      <button type="button" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        What your agent said
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" style={{ transform: open ? 'rotate(180deg)' : undefined }}>
          <path d="M2 3.5 5 6.5 8 3.5" fill="none" stroke="currentColor" strokeWidth="1.4" />
        </svg>
      </button>
      {open && <p className="lede">{said}</p>}
    </div>
  )
}

function Desk({ opened, example }: { opened: Opened; example?: boolean }) {
  // The whole path: the way in, then each part of the rest, in order.
  const steps = useMemo(() => [...(opened.way?.order ?? []), ...(opened.way?.parts ?? []).flatMap((p) => p.steps)], [opened.way])
  const finding = !!opened.way?.finding && !steps.length
  const [at, go] = useAt(example ? '#/example' : `#/o/${opened.id}`, steps.length || finding ? 'order' : 'files')
  const narrow = useNarrow()
  const isChange = opened.kind === 'change'
  // The folder whose page shows: the one asked for; else, with a way in,
  // the top's, the overview, first on the way in, as a change opens on its
  // own page.
  const dir = at.d ?? (!isChange && !at.f && steps.length > 0 ? '' : null)
  // Read plainly too: how far reading has got counts each file once it's read plainly, and each page once it's written.
  // Asked again as files are read, and as the way in comes (`opened`), so
  // how far reading has got counts what it reads.
  const change = useLoad<ChangeView>(isChange ? `/api/opened/${opened.id}/change` : null, ['marks', 'read', 'plainly', 'spec', 'opened'], (e) => marksFor(opened.id)(e) && readFor(opened.id)(e))
  const folder = useLoad<FolderView>(isChange ? null : `/api/opened/${opened.id}/folder`, ['read', 'plainly', 'spec', 'opened'], readFor(opened.id))
  const list = useLoad<FolderView>(isChange && at.w === 'all' ? `/api/opened/${opened.id}/folder?list=true` : null, ['read', 'marks', 'spec'], (e) => readFor(opened.id)(e) && marksFor(opened.id)(e))
  const kept = useLoad<string[]>('/api/read', ['idea', 'read'])
  const ready = useLoad<ReadyItem[]>(`/api/opened/${opened.id}/ready`, ['idea', 'asks'])
  const notes = useLoad<NoteRef[]>(`/api/opened/${opened.id}/notes`, ['notes'])
  const settings = useLoad<Settings>('/api/settings', ['settings'])
  const agentList = useAgentList()
  // An address with `q` opens search on its words.
  const [searching, setSearching] = useState(() => new URLSearchParams(location.hash.split('?')[1] ?? '').has('q'))
  // Entries asked for here lately and written: newly written, in search.
  const written = useLoad<ReadyItem[]>(searching ? `/api/opened/${opened.id}/written` : null, ['idea', 'asks'])
  const [sideOpen, setSideOpen] = useState(false)
  const [widths, setWidth] = usePaneWidths()
  const windowWidth = useWindowWidth()
  useSearchKey(useCallback(() => setSearching(true), []))

  // Where you are in it, for the way back from settings and the start page.
  useEffect(() => {
    if (!example) rememberOpen({ hash: location.hash || `#/o/${opened.id}`, title: opened.title })
  }, [at, opened.id, opened.title, example])

  // Squares: new in this change, and read.
  const builtOn = useMemo(() => (isChange ? change.data?.builtOn : folder.data?.builtOn) ?? [], [isChange, change.data, folder.data])
  const fresh = useMemo(() => new Set(isChange ? builtOn.filter((b) => b.new).map((b) => b.key) : []), [isChange, builtOn])
  const [readNow, setReadNow] = useState<Set<string>>(new Set())
  const readSet = useMemo(() => new Set([...(kept.data ?? []), ...readNow]), [kept.data, readNow])
  const markRead = useCallback((key: string) => setReadNow((s) => (s.has(key) ? s : new Set(s).add(key))), [])
  const seen: Seen = useMemo(() => ({ isNew: (k) => fresh.has(k), isRead: (k) => readSet.has(k), read: markRead }), [fresh, readSet, markRead])

  // The files the side panel lists: the changed files, or every file.
  const changedFiles: SideFile[] = useMemo(
    () => (change.data?.files ?? []).map((f) => ({ path: f.path, ideas: f.ideas, skip: f.skip, gone: f.status === 'deleted', added: f.added, removed: f.removed })),
    [change.data],
  )
  const folderFiles: SideFile[] = useMemo(() => (folder.data?.files ?? []).map((f) => ({ path: f.path, ideas: f.ideas, skip: f.skip })), [folder.data])
  const everyFile: SideFile[] = useMemo(() => {
    const ideas = new Map(changedFiles.map((f) => [f.path, f]))
    return (list.data?.files ?? []).map((f) => ideas.get(f.path) ?? { path: f.path, ideas: f.ideas, skip: f.skip })
  }, [list.data, changedFiles])
  const sideFiles = isChange ? (at.w === 'all' ? everyFile : changedFiles) : folderFiles
  // The ideas the files choice covers: in a change, its own, or every file's read so far.
  // This work's ideas, for search before you type: those in the most files first.
  const workIdeas = useMemo(() => {
    const by = new Map<string, { key: string; name: string; files: number }>()
    for (const f of isChange ? changedFiles : folderFiles) for (const t of f.ideas ?? []) {
      const had = by.get(t.key)
      if (had) had.files += 1
      else by.set(t.key, { key: t.key, name: t.name, files: 1 })
    }
    return [...by.values()].sort((a, b) => b.files - a.files || a.name.localeCompare(b.name))
  }, [isChange, changedFiles, folderFiles])
  // Every lesson about the ideas in the work, for search: those used here first.
  const lessonsHere = useLessons(opened.id).data
  const workLessons = useMemo(() => {
    const seen = new Set<string>()
    return [...(lessonsHere?.used ?? []), ...(lessonsHere?.here ?? [])].filter((l) => !seen.has(l.id) && (seen.add(l.id), true))
  }, [lessonsHere])

  // The file shown: the one asked for, else the one opened, else the first
  // step on the way in, else the first that matters. None while a folder's
  // page shows, or a change's: a change opens on its own page.
  const path = useMemo(() => {
    if (at.f) return at.f
    if (dir != null || isChange) return null
    if (steps.length) return steps[0].path
    if (finding) return null
    if (opened.kind === 'file' && opened.path) return opened.path
    const files = folder.data?.files ?? []
    const readme = files.find((f) => /^readme(\.md|\.txt)?$/i.test(f.path))
    return (readme ?? files.find((f) => f.ideas?.length) ?? files.find((f) => !f.skip))?.path ?? null
  }, [at.f, dir, steps, finding, opened, isChange, folder.data])
  // The overview read as a step, by the path "", once you've had it open.
  const overviewOn = steps.length > 0 && !at.f && (isChange ? at.d == null : dir === '')
  const onPath = useCallback((p: string) => (p === '' ? steps.length > 0 : steps.some((s) => s.path === p)), [steps])
  const [stepRead, seenPlainly] = useStepsRead(opened, overviewOn ? '' : path, onPath, example)
  // The files your agent has read: a file is read once its ideas are marked.
  const agentRead = useMemo(() => {
    const files = isChange ? change.data?.files : folder.data?.files
    if (!files) return undefined
    const read = new Set(files.filter((f) => f.ideas != null || f.skip != null).map((f) => f.path))
    return (p: string) => read.has(p)
  }, [isChange, change.data, folder.data])
  const stepAt = path ? steps.findIndex((s) => s.path === path) : -1
  const nextStep = stepAt >= 0 ? steps[stepAt + 1] : undefined
  const changed = useMemo(() => new Set(change.data?.files.map((f) => f.path) ?? []), [change.data])
  // What kelve marks for you decides how a file reads: in before any file opens.
  useEffect(() => fetchAhead(MARKS), [])
  // A file likely opened next is looked at once this one's shown, so it opens at once:
  // the next in order, or one in the side you've rested on.
  const ahead = useCallback((f: string) => fileAhead(opened.id, f, changed.has(f)), [opened.id, changed])
  useEffect(() => {
    if (!nextStep) return
    const t = window.setTimeout(() => ahead(nextStep.path), AHEAD_AFTER)
    return () => window.clearTimeout(t)
  }, [nextStep, ahead])
  const openFile = useCallback((f: string, l?: number) => go({ f, l, m: null, d: null }), [go])
  // A part a folder's page or a change's page names: a folder's page, or a file, plainly.
  const openPart = useCallback(
    (p: Part) => (p.folder ? go({ d: p.path, f: null, l: undefined, m: null }) : go({ f: p.path, r: 'plainly', l: undefined, m: null, d: null })),
    [go],
  )
  const changePage = isChange && !at.f && at.d == null
  const inChange = isChange && !!path && changed.has(path)

  // The path through entries, and their names for the top bar.
  const [names, setNames] = useState<Record<string, string>>({})
  const named = useCallback((key: string, name: string) => setNames((n) => (n[key] === name ? n : { ...n, [key]: name })), [])
  // An idea opens beside a note that's open, rather than closing it: you
  // can go on writing with the file and the idea in view.
  const openIdea = useCallback((key: string) => go({ p: [{ key, followed: false }] }), [go])
  // A lesson opens where an entry does, beside the work (Hive-4).
  const openLesson = useCallback((id: string) => go({ p: [{ key: LESSON + id, followed: false }] }), [go])
  const lessonOn = at.p[0] && isLesson(at.p[0].key) ? at.p[0].key.slice(LESSON.length) : null
  const agentName = agentKindName(settings.data?.agent)
  // Something on the agent's list: here, or in what else it's for.
  const openItem = useCallback(
    (item: ListItem) => {
      if (item.opened != null && item.opened !== opened.id && !example) {
        const q = new URLSearchParams()
        if (item.idea) q.set('p', item.idea)
        if (item.note) q.set('n', item.note)
        if (item.kind === 'plainly') {
          q.set('f', item.title)
          q.set('r', 'plainly')
        }
        location.hash = `#/o/${item.opened}${q.size ? `?${q}` : ''}`
        return
      }
      if (item.idea) go({ p: [{ key: item.idea, followed: false }], k: null })
      else if (item.note) go({ n: item.note, na: null, k: null })
      else if (item.kind === 'plainly') go({ f: item.title, r: 'plainly', l: undefined, m: null, d: null, k: null })
      else go({ k: null })
    },
    [go, opened.id, example],
  )
  // What the tab shows is what's open: going to something opened before,
  // from the list, Start or a link, opens it again, and its reading carries on.
  useEffect(() => {
    if (!example) void post(`/api/opened/${opened.id}/again`).catch(() => {})
  }, [opened.id, example])
  // Something paused, opened again: the tab moves to it.
  const openPaused = useCallback(
    async (p: Paused) => {
      if (example) return go({ k: null })
      location.hash = `#/o/${p.opened}`
    },
    [go, example],
  )
  const openFrom = useCallback((key: string, from: number) => go({ p: [...read().p.slice(0, from + 1), { key, followed: true }] }), [go])

  // Notes: what they need from the reader, and the page one opens as.
  const [pageKey, setPageKey] = useState(0)
  const noting: NotesWith = useMemo(
    () => ({
      opened: opened.id,
      agent: agentKindName(settings.data?.agent),
      place: settings.data?.newNote ?? 'you',
      notesDir: settings.data?.notesDir ?? 'your notes folder',
      page: (id, ideas) => {
        setPageKey((k) => k + 1)
        // A note on an idea opens beside its entry, so you can read it as you write.
        go({ n: id ?? 'new', na: ideas?.[0] ?? null, np: null, p: ideas?.length ? read().p : [], k: null })
      },
      goTo: (f, l) => go({ f, l, m: null }),
      hint: !example && !!settings.data && !settings.data.hints.includes('notes'),
      hinted: () => void post('/api/settings', { hintSeen: 'notes' }).then(settings.reload),
    }),
    [opened.id, settings, go, example],
  )
  // The first result: one hint, beside the first new square (O-3).
  const squaresHint = !example && !!settings.data && !settings.data.hints.includes('squares') && builtOn.length > 0
  const squaresSeen = () => void post('/api/settings', { hintSeen: 'squares' }).then(settings.reload)

  // A side view opens at the right of those open, always (Nathan, 30
  // October): a note, the list or what kelve marks, opened beside an entry,
  // goes after it; an entry opened beside one of those goes after that.
  const pageLast = useNewestRight(at.n ?? at.k ?? null, at.p.at(-1)?.key ?? null, at.p.length)
  // Escape closes the entry on top; with nothing open beside the work, it
  // goes to the start page (board Home-1). Never when something else has
  // it: search, a popover, a field, an editor, a key with others held.
  const home = example ? '#/example' : '#/'
  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey || searching) return
      const t = e.target as HTMLElement
      if (t.closest('input, textarea, select, [contenteditable="true"], [role="dialog"], [role="menu"], [role="listbox"]')) return
      if (!at.p.length && !at.n && !at.k) {
        location.hash = home
        return
      }
      // The rightmost goes first: the entry or the page opened last.
      const page = at.n ? { n: null } : at.k ? { k: null } : null
      go(page && (pageLast || !at.p.length) ? page : { p: at.p.slice(0, -1) })
    }
    window.addEventListener('keydown', on)
    return () => window.removeEventListener('keydown', on)
  }, [at.p, at.n, at.k, searching, go, home, pageLast])

  const title = opened.title
  const reading = isChange ? change.data?.reading : folder.data?.reading
  // How far reading has got, said only of what you have open.
  const readingOpen = reading?.marking === 'marking' && (agentList.data?.open ?? opened.id) === opened.id ? reading : undefined
  const loaded = isChange ? change : folder

  const commands: Command[] = [
    { name: 'Show the files', run: () => go({ t: 'files' }) },
    ...(at.p.length ? [{ name: 'Close the entries', run: () => go({ p: [] }) }] : []),
    { name: shownTheme() === 'dark' ? 'Switch to the light theme' : 'Switch to the dark theme', run: () => setTheme(shownTheme() === 'dark' ? 'light' : 'dark') },
    ...(example
      ? []
      : [
          { name: 'Open something else', run: () => { location.hash = '#/' } },
          { name: 'Settings', run: () => { location.hash = '#/you' } },
        ]),
  ]


  const page = !!at.n || !!at.k
  // The entries beside the work: one opened stays as it was until the next is in, for a moment.
  const shownP = useHeld(
    at.p.map((s) => `${s.key}:${s.followed}`).join(' '),
    at.p,
    at.p.every((s) => isKept(paneAddress(s))),
    () => Promise.all(at.p.map((s) => fetchNow(paneAddress(s)))),
  )
  const panes = shownP.length > 0 || page
  // How much the panes take, and whether the side panel still fits beside the work.
  // The list is narrower than a note: a line each.
  const pageWidth = at.n ? widths.note : at.k === 'list' ? widths.list : widths.note
  const panesFor = (p: number, n: boolean) =>
    n ? pageWidth + (p ? widths.back : 0) : p >= 2 ? widths.entry + widths.back : p === 1 ? widths.entry : 0
  const sideFits = (p: number, n: boolean) => windowWidth >= widths.side + WORK_LEAST + panesFor(p, n)
  const showSide = narrow ? sideOpen : sideFits(shownP.length, page)
  const fileNotes = (notes.data ?? []).filter((n) => n.on?.path === path)
  // What the work shows: a folder's page, the change's, or a file. Going to
  // another, this one stays until it's in, for a moment, rather than blinking.
  const workId = loaded.error ? 'error' : dir != null && !at.f ? `d:${dir}` : changePage ? 'change' : path ? `f:${path}:${inChange}` : 'none'
  const pageAddress = dir != null && !at.f ? `/api/opened/${opened.id}/plainly/folder?path=${encodeURIComponent(dir)}` : changePage ? `/api/opened/${opened.id}/plainly/change` : null
  const work = (
    loaded.error ? (
      <p className="fv-say">{loaded.error}</p>
    ) : dir != null && !at.f ? (
      <FolderPage key={dir} opened={opened} path={dir} agent={agentName} open={openPart} idea={openIdea} />
    ) : changePage ? (
      <ChangePage opened={opened} files={changedFiles} agent={agentName} open={(f) => openPart({ path: f, folder: false })} idea={openIdea} lesson={openLesson} lessonOn={lessonOn} />
    ) : path ? (
      <FileView
        key={`${path}:${inChange}`}
        opened={opened}
        path={path}
        inChange={inChange}
        lit={at.p[0]?.key}
        focus={at.m ?? undefined}
        line={at.l}
        open={openIdea}
        lesson={openLesson}
        notes={fileNotes}
        reads={at.r}
        setReads={(r) => go({ f: path, r }, true)}
        openPath={(p) => (p.endsWith('/') ? go({ d: p.slice(0, -1), f: null, m: null }) : openFile(p))}
        next={
          nextStep && (
            <button type="button" className="nextf" onClick={() => openFile(nextStep.path)}>
              <span className="lb">Next</span>
              <span className="nm">{stepAt + 2}</span>
              <span>
                <b className="mono">{nextStep.path}</b>
                <span>{partStarting(opened.way?.parts ?? [], nextStep.path) ?? nextStep.why}</span>
              </span>
            </button>
          )
        }
      />
    ) : finding ? (
      <Finding title={opened.title} files={isChange ? change.data?.files : folder.data?.files} />
    ) : (
      <p className="fv-say">{loaded.data ? 'There’s no file here to read.' : <Loader size="small" />}</p>
    )
  )
  const held = useHeld(
    workId,
    work,
    pageAddress ? isKept(pageAddress) : path && !loaded.error ? fileIn(opened.id, path, inChange) : true,
    () => (pageAddress ? fetchNow(pageAddress) : path ? fetchFile(opened.id, path, inChange) : Promise.resolve()),
  )
  // What's open in this tab, for what to say to your agent: this, by its number.
  const forThis = useMemo(() => ({ id: opened.id, title }), [opened.id, title])
  // A note, the list or what kelve marks, beside the work.
  const pageView = at.n ? (
    <section className="pane page" style={narrow ? undefined : { width: widths.note }} aria-label="The note">
      {!narrow && <Grip width={widths.note} set={(w) => setWidth('note', w)} label="The width of the note" />}
      <div className="pane-in">
        <NotePage
          key={pageKey}
          id={at.n === 'new' ? null : at.n}
          ideas={at.na ? [at.na] : undefined}
          place={at.np ?? undefined}
          opened={opened.id}
          kept={(n) => go({ n, na: null }, true)}
          close={() => go({ n: null, na: null })}
          onIdea={openIdea}
        />
      </div>
    </section>
  ) : at.k === 'list' ? (
    <section className="pane page" style={narrow ? undefined : { width: widths.list }} aria-label={`${agentName}’s list`}>
      {!narrow && <Grip width={widths.list} set={(w) => setWidth('list', w)} label="The width of the list" />}
      <div className="pane-in">
        <ListPane list={agentList.data} agent={agentName} onOpen={openItem} onOpenPaused={openPaused} reading={reading && (agentList.data?.open ?? opened.id) === opened.id ? { ...reading, opened: opened.id } : undefined} newNote={() => go({ n: 'new', np: 'agent', k: null })} close={() => go({ k: null })} />
      </div>
    </section>
  ) : at.k === 'marks' ? (
    <section className="pane page" style={narrow ? undefined : { width: widths.note }} aria-label="What kelve marks">
      {!narrow && <Grip width={widths.note} set={(w) => setWidth('note', w)} label="The width of what kelve marks" />}
      <div className="pane-in">
        <MarkedForYou agent={agentName} note={() => go({ n: 'new', np: 'agent', k: null })} close={() => go({ k: null })} />
      </div>
    </section>
  ) : null
  // Entries at the right: of the work, or of a page that's open.
  const entryPanes = shownP.length > 0 && <Panes path={shownP} open={openFrom} close={(n) => go({ p: read().p.filter((_, i) => i !== n) })} named={named} widths={narrow ? undefined : widths} setWidth={setWidth} beside={page} opened={example ? undefined : opened.id} />
  return (
    <SeenContext.Provider value={seen}>
    <SeenPlainly.Provider value={seenPlainly}>
    <ListContext.Provider value={agentList.data}>
    <OpenContext.Provider value={forThis}>
    <NotesContext.Provider value={noting}>
      <div className={`reader${panes ? ' with-panes' : ''}${(shownP.length > 1 && !page) || (page && shownP.length) ? ' two' : ''}`}>
        <Top
          search={() => setSearching(true)}
          home={home}
          esc
          settings={example ? '#/example/you/agent' : '#/you'}
          right={
            <>
              <ReadyKey ready={ready.data ?? []} entry={openIdea} />
              <AgentLine
                list={agentList.data}
                agent={agentName}
                open={at.k === 'list'}
                onClick={() => go({ k: at.k === 'list' ? null : 'list', n: null })}
                reading={readingOpen}
              />
            </>
          }
        >
          {narrow && !panes && (
            <button type="button" className="side-key" aria-expanded={sideOpen} onClick={() => setSideOpen((o) => !o)}>Files</button>
          )}
          {/* The project, unless the title already says it. */}
          {project(opened.root).toLowerCase() !== title.toLowerCase() && <span className="proj" title={opened.root}>{project(opened.root)}</span>}
          {/* The title, then the path through entries: one bar, so the files have the height (board H-5). */}
          <nav className="path" aria-label="Where you are">
            <h1 className="title" title={title}>
              {at.p.length ? <button type="button" onClick={() => go({ p: [] })}>{title}</button> : <b>{title}</b>}
            </h1>
            {at.p.map((s, i) => (
              <span key={`${i}:${s.key}`}>
                <i aria-hidden="true">/</i>
                {i === at.p.length - 1 ? <b aria-current="page">{names[s.key] ?? '…'}</b> : <button type="button" onClick={() => go({ p: at.p.slice(0, i + 1) })}>{names[s.key] ?? '…'}</button>}
              </span>
            ))}
          </nav>
          {opened.said && !at.p.length && <Said said={opened.said} />}
        </Top>
        <div className="frame">
          <main className="work" aria-label={title}>
            <div className="view">
              {showSide && (
                <Side
                  foot={
                    <SideKey
                      change={isChange}
                      order={at.t === 'order' && (steps.length > 0 || finding)}
                      agent={agentName}
                      marks={() => go({ k: 'marks', n: null })}
                    />
                  }
                  filesHint={
                    squaresHint && (
                      <Hint side="up" style={{ top: 'calc(100% + 12px)', left: 'calc(50% - 30px)' }} onDone={squaresSeen}>
                        <b>Each square is an idea this work rests on.</b> {isChange ? 'Red ones are new in this change. ' : ''}Click one for what it is, how it works and why, then what it rests on. A filled square is still to read; it empties once you have.
                      </Hint>
                    )
                  }
                  width={narrow ? undefined : widths.side}
                  grip={narrow ? undefined : <Grip right width={widths.side} set={(w) => setWidth('side', w)} label="The width of the side" />}
                  tab={at.t}
                  setTab={(t) => go({ t }, true)}
                  files={sideFiles}
                  loading={!loaded.data && !loaded.error}
                  which={at.w}
                  setWhich={(w) => go({ w }, true)}
                  change={isChange}
                  here={path ?? undefined}
                  look={ahead}
                  go={(f) => {
                    setSideOpen(false)
                    go({ f, l: undefined, m: null, d: null })
                  }}
                  notes={(filter) => <NotesTab notes={notes.data ?? []} filter={filter} />}
                  order={opened.way?.order}
                  parts={opened.way?.parts}
                  finding={finding}
                  reading={opened.way?.reading}
                  leftOff={opened.way?.leftOff}
                  readPart={(name) => { if (!example) void post(`/api/opened/${opened.id}/way/part`, { name }).catch(() => {}) }}
                  agentRead={agentRead}
                  agent={agentName}
                  read={stepRead}
                  folders={opened.way?.folders}
                  folder={at.d ?? undefined}
                  goFolder={(d) => {
                    setSideOpen(false)
                    go({ d, f: null, l: undefined, m: null })
                  }}
                  changePage={changePage}
                  goChange={() => {
                    setSideOpen(false)
                    go({ f: null, d: null, l: undefined, m: null })
                  }}
                  overview={{
                    title: isChange ? 'What this change does' : 'What this is',
                    why: 'The whole of it, before its files.',
                    on: overviewOn,
                    go: () => {
                      setSideOpen(false)
                      go({ f: null, d: null, l: undefined, m: null })
                    },
                  }}
                />
              )}
              {held}
            </div>
          </main>
          {/* The side views, the one opened last at the right. */}
          {pageLast ? <>{entryPanes}{pageView}</> : <>{pageView}{entryPanes}</>}
        </div>
        {searching && (
          <Search
            initial={new URLSearchParams(location.hash.split('?')[1] ?? '').get('q') ?? ''}
            opened={opened}
            commands={commands}
            close={() => setSearching(false)}
            onIdea={openIdea}
            ideas={workIdeas}
            written={written.data}
            lessons={workLessons}
            onLesson={openLesson}
            onFile={(f) => go({ f, l: undefined, m: null })}
            onLine={(f, l) => go({ f, l, m: null })}
            onNote={(n) => noting.page(n)}
          />
        )}
      </div>
    </NotesContext.Provider>
    </OpenContext.Provider>
    </ListContext.Provider>
    </SeenPlainly.Provider>
    </SeenContext.Provider>
  )
}
