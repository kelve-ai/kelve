// The side panel (boards X-1 to X-3, T-1 to T-3, H-6): In order, when your
// agent gave a way in, then Files and Notes, with a filter for the tab you're
// on; above the way in and the files, what the work rests on. In order is the
// way in, each step a file and why to read it now: your agent's dial until it's
// read it, filled once it's ready for you, hollow once you've read it plainly.
// Files is a tree, each file with at most one square, each folder with its line
// when your agent gave one, wrapped onto as many lines as it takes; a folder's
// name opens its page. For a change, Files says how much changed, and opens the
// change's own page, and each file how many lines it added and took out. The
// tree scrolls on its own, and is drawn as it scrolls, so 20,000 files feel the
// same as 20.

import { type ReactNode, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { FolderLine, IdeaTag, StepAt, WayPart, WayStep, WorkStage } from '../api/types'
import { Loader } from '../Loader'
import { Choice, State } from '../ui/ui'
import { useSeen } from './Square'

/** A file as the side panel lists it; in a change, with the lines it added and took out. */
export type SideFile = { path: string; ideas: IdeaTag[] | null; skip?: string | null; gone?: boolean; added?: number; removed?: number }

export type Tab = 'order' | 'files' | 'notes'

/** How long a file's row is rested on before the file is looked at, so passing over rows looks at none. */
const LOOK_AFTER = 80

type Row =
  | { kind: 'dir'; path: string; name: string; depth: number; open: boolean }
  | { kind: 'file'; file: SideFile; name: string; depth: number }

type Dir = { name: string; path: string; dirs: Map<string, Dir>; files: SideFile[] }

/** The folders and files as a tree, folders first, each by name; tooling's dot-folders last. */
function tree(files: SideFile[]): Dir {
  const root: Dir = { name: '', path: '', dirs: new Map(), files: [] }
  for (const f of files) {
    const parts = f.path.split('/')
    let at = root
    for (let i = 0; i < parts.length - 1; i++) {
      const path = parts.slice(0, i + 1).join('/')
      let next = at.dirs.get(parts[i])
      if (!next) at.dirs.set(parts[i], (next = { name: parts[i], path, dirs: new Map(), files: [] }))
      at = next
    }
    at.files.push(f)
  }
  return root
}

const byName = (a: string, b: string) => Number(a.startsWith('.')) - Number(b.startsWith('.')) || a.localeCompare(b)

/** The rows on show: open folders' contents, a chain of single folders as one row. */
function rows(root: Dir, open: (path: string) => boolean): Row[] {
  const out: Row[] = []
  const walk = (dir: Dir, depth: number) => {
    for (const d of [...dir.dirs.values()].sort((a, b) => byName(a.name, b.name))) {
      let at = d
      let name = d.name
      while (at.files.length === 0 && at.dirs.size === 1) {
        at = [...at.dirs.values()][0]
        name = `${name}/${at.name}`
      }
      const isOpen = open(at.path)
      out.push({ kind: 'dir', path: at.path, name, depth, open: isOpen })
      if (isOpen) walk(at, depth + 1)
    }
    for (const f of [...dir.files].sort((a, b) => byName(a.path, b.path))) out.push({ kind: 'file', file: f, name: f.path.split('/').pop() ?? f.path, depth })
  }
  walk(root, 0)
  return out
}

/** A file's one square: red if anything in it is new, grey if it has ideas; hollow once every one is read. */
function FileSquare({ ideas }: { ideas: IdeaTag[] | null }) {
  const seen = useSeen()
  if (!ideas?.length) return null
  const isNew = ideas.some((i) => seen.isNew(i.key))
  const read = ideas.every((i) => seen.isRead(i.key))
  const label = `${ideas.length === 1 ? 'One idea' : `${ideas.length} ideas`}${isNew ? ', something new in this change' : ''}${read ? ', all read' : ''}`
  return <i className={`sq small${isNew ? ' new' : ''}${read ? ' read' : ''}`} role="img" aria-label={label} />
}

/** How many lines a change added and took out, as the side says it: `+9 −3`. */
function Counts({ added = 0, removed = 0 }: { added?: number; removed?: number }) {
  if (!added && !removed) return null
  const label = [added ? `${added} added` : '', removed ? `${removed} taken out` : ''].filter(Boolean).join(', ')
  return (
    <span className="cnt" role="img" aria-label={label}>
      {added > 0 && <span className="a">+{added}</span>}
      {added > 0 && removed > 0 && ' '}
      {removed > 0 && <span className="r">{'\u2212'}{removed}</span>}
    </span>
  )
}

/** A change's files, all told: how many, and the lines they added and took out; it opens the change's own page. */
function Changed({ files, on, go }: { files: SideFile[]; on?: boolean; go?: () => void }) {
  const added = files.reduce((n, f) => n + (f.added ?? 0), 0)
  const removed = files.reduce((n, f) => n + (f.removed ?? 0), 0)
  const inner = (
    <>
      <b>Changed</b>
      <span>{files.length === 1 ? '1 file' : `${files.length} files`}</span>
      <Counts added={added} removed={removed} />
    </>
  )
  if (!go) return <p className="lgroup changed">{inner}</p>
  return (
    <button type="button" className={`lgroup changed${on ? ' on' : ''}`} aria-current={on ? 'page' : undefined} title="The change, plainly" onClick={go}>
      {inner}
    </button>
  )
}

const ROW = 28
/** A line of a folder's line, under its name: it wraps onto as many as it takes. */
const LINE = 17

/** Images kelve shows as pictures, though it doesn't read them: not dimmed. */
const PICTURE = /\.(png|jpe?g|gif|webp|avif|bmp|ico|svg)$/i

/** Draws only the rows in view. */
function Virtual<T>({ items, render, keyOf, at, here, heightOf, settled = true, ruler }: {
  items: T[]
  render: (item: T) => ReactNode
  keyOf: (item: T) => string
  /** A row's height, when not all are one. */
  heightOf?: (item: T) => number
  /** The row you're on, and what it is: it's brought into view when what it is changes, not when rows above it open or close. */
  at?: number
  here?: string
  /** Whether the rows' heights are measured: until they are, the row you're on is brought into view again once they are. */
  settled?: boolean
  /** Hidden, as wide as a row: for measuring rows. */
  ruler?: ReactNode
}) {
  const host = useRef<HTMLDivElement>(null)
  const [view, setView] = useState({ top: 0, height: 800 })
  useEffect(() => {
    const el = host.current
    if (!el) return
    const on = () => setView({ top: el.scrollTop, height: el.clientHeight })
    on()
    el.addEventListener('scroll', on, { passive: true })
    const size = new ResizeObserver(on)
    size.observe(el)
    return () => {
      el.removeEventListener('scroll', on)
      size.disconnect()
    }
  }, [])
  // Where each row starts.
  const tops = useMemo(() => {
    const out = new Array<number>(items.length + 1)
    out[0] = 0
    for (let i = 0; i < items.length; i++) out[i + 1] = out[i] + (heightOf ? heightOf(items[i]) : ROW)
    return out
  }, [items, heightOf])
  // The row you're on, in view: once for each, and again once heights are
  // measured, if they weren't yet.
  const brought = useRef<{ here?: string; settled: boolean } | null>(null)
  useEffect(() => {
    const el = host.current
    if (!el || at == null || at < 0) return
    if (brought.current && brought.current.here === here && (brought.current.settled || !settled)) return
    brought.current = { here, settled }
    const top = tops[at]
    if (top < el.scrollTop || tops[at + 1] > el.scrollTop + el.clientHeight) el.scrollTop = top - el.clientHeight / 3
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [here, settled])
  const rowAt = (y: number) => {
    let lo = 0
    let hi = items.length
    while (lo < hi) {
      const mid = (lo + hi) >> 1
      if (tops[mid + 1] <= y) lo = mid + 1
      else hi = mid
    }
    return lo
  }
  const first = Math.max(0, rowAt(view.top) - 10)
  const last = Math.min(items.length, rowAt(view.top + view.height) + 10)
  return (
    <div className="side-list" ref={host} role="tree">
      <div style={{ height: tops[items.length], position: 'relative' }}>
        {ruler}
        {items.slice(first, last).map((item, i) => (
          <div key={keyOf(item)} style={{ position: 'absolute', top: tops[first + i], left: 0, right: 0 }}>
            {render(item)}
          </div>
        ))}
      </div>
    </div>
  )
}

const CHEVRON = (open: boolean) => (
  <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" style={{ transform: open ? undefined : 'rotate(-90deg)' }}>
    <path d="M2 3.5 5 6.5 8 3.5" fill="none" stroke="currentColor" strokeWidth="1.4" />
  </svg>
)

function Files({ files, loading, here, go, goFolder, folder, lines, filter, kinds, changed, changePage, goChange }: {
  files: SideFile[]
  /** Still on its way. */
  loading?: boolean
  here?: string
  go: (path: string) => void
  /** Opens a folder's page. */
  goFolder?: (path: string) => void
  /** The folder whose page is open. */
  folder?: string
  /** What each folder is for, by its path. */
  lines: Map<string, string>
  filter: string
  kinds?: ReactNode
  /** A change's own files: headed with how much changed. */
  changed?: boolean
  /** Whether the change's own page shows. */
  changePage?: boolean
  /** Opens the change's own page. */
  goChange?: () => void
}) {
  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase()
    return q ? files.filter((f) => f.path.toLowerCase().includes(q)) : files
  }, [files, filter])
  const root = useMemo(() => tree(shown), [shown])
  // Folders opened or shut here; until then, a change's every folder, so
  // all it changed shows, or else the first two levels and the way to the
  // file you're on.
  const [toggled, setToggled] = useState<Map<string, boolean>>(new Map())
  const many = files.length > 60
  const open = (path: string) => {
    const t = toggled.get(path)
    if (t !== undefined) return t
    if (filter.trim() || !many || changed) return true
    return path.split('/').length <= 1 || !!here?.startsWith(`${path}/`)
  }
  const list = rows(root, open)
  const at = list.findIndex((r) => (r.kind === 'file' && r.file.path === here) || (r.kind === 'dir' && r.path === folder))
  // Folders' lines wrap, so each is measured, at the tree's width, as a
  // hidden row: all those not yet measured at once, before they're drawn.
  // Kept by depth and line until the width changes, or the fonts load.
  const ruler = useRef<HTMLDivElement>(null)
  const [, setWide] = useState(0)
  const [fonts, setFonts] = useState(0)
  const [tall, setTall] = useState<{ wide: number; fonts: number; by: Map<string, number> }>({ wide: 0, fonts: 0, by: new Map() })
  useEffect(() => {
    const el = ruler.current
    if (!el) return
    let live = true
    const size = new ResizeObserver(() => setWide(el.clientWidth))
    size.observe(el)
    void document.fonts?.ready.then(() => live && setFonts((n) => n + 1))
    return () => {
      live = false
      size.disconnect()
    }
  }, [])
  const lineOf = (depth: number, line: string) => `${depth}:${line}`
  useLayoutEffect(() => {
    const el = ruler.current
    const wide = el?.clientWidth ?? 0
    if (!el || !wide) return
    const fresh = tall.wide === wide && tall.fonts === fonts
    const by = fresh ? tall.by : new Map<string, number>()
    const want = new Map<string, HTMLElement>()
    for (const r of list) {
      const line = r.kind === 'dir' ? lines.get(r.path) : undefined
      if (line === undefined || by.has(lineOf(r.depth, line)) || want.has(lineOf(r.depth, line))) continue
      const row = document.createElement('div')
      row.className = 'tr dir lined'
      row.style.paddingLeft = `${8 + r.depth * 16}px`
      const text = document.createElement('span')
      text.className = 'fl2'
      text.textContent = line
      row.append(document.createElement('span'), document.createElement('span'), text)
      want.set(lineOf(r.depth, line), text)
    }
    if (fresh && !want.size) return
    el.replaceChildren(...[...want.values()].map((t) => t.parentElement!))
    for (const [k, t] of want) by.set(k, Math.max(LINE, Math.ceil(t.getBoundingClientRect().height)))
    el.replaceChildren()
    setTall({ wide, fonts, by: new Map(by) })
  })
  const heightOf = useCallback((r: Row) => {
    const line = r.kind === 'dir' ? lines.get(r.path) : undefined
    return line === undefined ? ROW : ROW + (tall.by.get(lineOf(r.depth, line)) ?? LINE)
  }, [lines, tall])
  const guides = (depth: number) => Array.from({ length: depth }, (_, i) => <span key={i} className="guide" style={{ left: 14 + i * 16 }} />)
  return (
    <>
      {kinds}
      {changed && files.length > 0 && <Changed files={files} on={changePage} go={goChange} />}
      {!list.length && <p className="side-none">{loading ? <Loader size="small" /> : filter ? 'No file goes by that name.' : 'Nothing to show yet.'}</p>}
      <Virtual
        items={list}
        at={at}
        here={here ?? folder}
        heightOf={lines.size ? heightOf : undefined}
        settled={tall.wide > 0}
        ruler={<div ref={ruler} className="tr-ruler" aria-hidden="true" />}
        keyOf={(r) => (r.kind === 'dir' ? `d:${r.path}` : `f:${r.file.path}`)}
        render={(r) =>
          r.kind === 'dir' ? (
            // The whole row opens the folder, its line too; the chevron only opens and closes it here.
            <div
              className={`tr dir${lines.has(r.path) ? ' lined' : ''}${r.path === folder ? ' on' : ''}`}
              role="treeitem"
              aria-expanded={r.open}
              style={{ paddingLeft: 8 + r.depth * 16, height: lines.has(r.path) ? heightOf(r) : undefined }}
              onClick={(e) => {
                if ((e.target as HTMLElement).closest('.ch, .nm')) return
                setToggled((t) => new Map(t).set(r.path, true))
                goFolder?.(r.path)
              }}
            >
              {guides(r.depth)}
              <button type="button" className="ch" aria-label={r.open ? `Close ${r.name}` : `Open ${r.name}`} onClick={() => setToggled((t) => new Map(t).set(r.path, !r.open))}>
                {CHEVRON(r.open)}
              </button>
              <button
                type="button"
                className="nm"
                title={goFolder ? `What ${r.name} is for` : undefined}
                onClick={() => {
                  setToggled((t) => new Map(t).set(r.path, true))
                  goFolder?.(r.path)
                }}
              >
                {r.name.split('/').map((p, i) => <span key={i}>{i > 0 && <i>/</i>}{p}</span>)}
              </button>
              {lines.has(r.path) && <span className="fl2">{lines.get(r.path)}</span>}
            </div>
          ) : (
            <button
              type="button"
              className={`tr${r.file.path === here ? ' on' : ''}${(r.file.skip && !PICTURE.test(r.file.path)) || r.file.gone ? ' dim' : ''}`}
              role="treeitem"
              aria-current={r.file.path === here ? 'page' : undefined}
              title={PICTURE.test(r.file.path) ? r.file.path : (r.file.skip ?? r.file.path)}
              style={{ paddingLeft: 8 + r.depth * 16 }}
              data-look={r.file.path}
              onClick={() => go(r.file.path)}
            >
              {guides(r.depth)}
              <span className="ch" />
              <span className="nm mono">{r.name}</span>
              <FileSquare ideas={r.file.ideas} />
              <Counts added={r.file.added} removed={r.file.removed} />
            </button>
          )
        }
      />
    </>
  )
}

/** Your agent's square at work (Way-2): red, filling a third a stage, still. */
const FILLED: Record<WorkStage, string> = { reading: '33%', checking: '66%', writing: '100%' }
export const DOING: Record<WorkStage, string> = { reading: 'Reading it', checking: 'Checking its ideas', writing: 'Writing it plainly' }

/** The square alone says how far: its stage is in words only when you point at it, and for a screen reader. */
export function AtWork({ stage, said }: { stage?: WorkStage; said?: string }) {
  const style = { '--f': stage ? FILLED[stage] : '0%' } as React.CSSProperties
  return said ? <span className="wq" role="img" aria-label={said} title={said} style={style} /> : <span className="wq" aria-hidden="true" style={style} />
}

/** A part of the path, folded or open (Way-3): its name, how much is read, why it comes next. */
function PartHead({ name, count, why, open, toggle, ask, next }: {
  name: string
  count: string
  why?: string
  open: boolean
  toggle: () => void
  ask?: ReactNode
  /** The next file to read is in it, folded away (Order-3). */
  next?: boolean
}) {
  return (
    <>
      <button type="button" className={`ord-ph${open ? ' open' : ''}`} aria-expanded={open} onClick={toggle}>
        <span className="chev" aria-hidden="true" />
        <b>{name}</b>
        <span className="ct">{next && <span className="ord-next">Next · </span>}{count}</span>
      </button>
      {why && <p className="ord-why">{why}</p>}
      {ask}
    </>
  )
}

/**
 * The path through what's open (L-1, Read-1 to Read-4, Way-1 to Way-4): the
 * way in, then each part of the rest, each step a file and why to read it
 * there. Your agent's dial shows until it's read a file, filling a stage at
 * a time while it works; then the square is filled, ready for you, and
 * hollow once you've read it plainly. The rest it hasn't read are quieter. The way in starts open and
 * the parts folded; a part waits for you to ask for it, when there's a lot.
 */
/** What's open's overview, as the way in's first step. */
export type Overview = { title: string; why: string; on: boolean; go: () => void }

function Order({ order, parts, overview, finding, reading, leftOff, here, read, agentRead, agent, go, readPart, toFiles }: {
  order: WayStep[]
  parts: WayPart[]
  overview?: Overview
  finding?: boolean
  /** The files your agent is working through, and how far each is. */
  reading: StepAt[]
  /** How many files the path leaves off. */
  leftOff?: number
  here?: string
  read: (path: string) => boolean
  /** Whether your agent has read a step's file, once that's known. */
  agentRead?: (path: string) => boolean
  agent: string
  go: (path: string) => void
  /** Asks for a part to be read. */
  readPart?: (name: string) => void
  /** Shows the Files tab. */
  toFiles?: () => void
}) {
  const Agent = agent.charAt(0).toUpperCase() + agent.slice(1)
  // Open: the way in, and whichever part you opened; the part you're in opens too.
  const [opened, setOpened] = useState<Record<string, boolean>>({})
  const inPart = here ? parts.find((p) => p.steps.some((s) => s.path === here))?.name : undefined
  const isOpen = (name: string, first: boolean) => opened[name] ?? (first || name === inPart)
  const toggle = (name: string, first: boolean) => setOpened((o) => ({ ...o, [name]: !isOpen(name, first) }))
  if (finding && !order.length) {
    return (
      <div className="side-list">
        <div className="ord-finding" role="status">
          <AtWork />
          <span>{Agent} is finding a way in<small>The files to read first, then the rest in parts.</small></span>
        </div>
      </div>
    )
  }
  const at = new Map(reading.map((r) => [r.path, r.stage]))
  // Where to go on: the first you haven't read along the whole path, the
  // overview first, but the one you're reading. It says Next (Order-3), and
  // so does its part when that's folded.
  const now = overview?.on ? '' : here
  const next = [...(overview ? [{ path: '' }] : []), ...order, ...parts.flatMap((p) => p.steps)].find((s) => !read(s.path) && s.path !== now)?.path
  let n = 0
  const rows = (steps: WayStep[]) =>
    steps.map((s) => {
      n += 1
      const stage = at.get(s.path)
      const later = !stage && !!agentRead && !agentRead(s.path)
      // Your agent's dial until it's read it, filled once it's ready for
      // you, hollow once you've read it plainly (Nathan, 29 October).
      const mark = stage ? <AtWork stage={stage} said={`${n}, ${Agent}: ${DOING[stage].toLowerCase()}`} />
        : read(s.path) ? <State state="read" label={`${n}, read`} />
        : later ? <State state="queued" label={`${n}, for ${Agent} to read first`} />
        : <State state="ready" label={`${n}, ready to read`} />
      return (
        <li key={s.path}>
          <button
            type="button"
            className={`lrow top${s.path === here ? ' on' : ''}${read(s.path) ? ' rd' : ''}${later ? ' later' : ''}${s.path === next ? ' next' : ''}`}
            aria-current={s.path === here ? 'step' : undefined}
            data-look={s.path}
            onClick={() => go(s.path)}
          >
            {mark}
            <span className="lt wrap"><code>{s.path.split('/').map((p, k) => <span key={k}>{k > 0 && <>/<wbr /></>}{p}</span>)}</code></span>
            {s.path === next && <span className="ord-next">Next</span>}
            {s.why && <small>{s.why}</small>}
          </button>
        </li>
      )
    })
  const count = (steps: Array<Pick<WayStep, 'path'>>) => `${steps.filter((s) => read(s.path)).length} of ${steps.length} read`
  // The overview, the way in's first row: it opens the change's or the
  // folder's own page.
  const first = overview && (() => {
    n += 1
    const stage = at.get('')
    return (
      <li key="">
        <button
          type="button"
          className={`lrow top${overview.on ? ' on' : ''}${read('') ? ' rd' : ''}${next === '' ? ' next' : ''}`}
          aria-current={overview.on ? 'step' : undefined}
          onClick={overview.go}
        >
          {stage ? <AtWork stage={stage} said={`1, ${Agent}: writing it`} /> : <State state={read('') ? 'read' : 'ready'} label={read('') ? '1, read' : '1, ready to read'} />}
          <span className="lt wrap">{overview.title}</span>
          {next === '' && <span className="ord-next">Next</span>}
          <small>{overview.why}</small>
        </button>
      </li>
    )
  })()
  const wayCount = count([...(overview ? [{ path: '' }] : []), ...order])
  const files = (k: number) => (k === 1 ? '1 file' : `${k} files`)
  const wayOpen = isOpen('', true)
  return (
    <div className="side-list">
      <PartHead name="The way in" count={wayCount} open={wayOpen} toggle={() => toggle('', true)} next={!wayOpen && (next === '' || order.some((s) => s.path === next))} />
      {wayOpen && <ol className="ord">{first}{rows(order)}</ol>}
      {parts.length > 0 && <div className="ord-sep" />}
      {parts.map((p) => {
        const open = isOpen(p.name, false)
        if (!open) n += p.steps.length
        return (
          <section key={p.name} aria-label={p.name}>
            <PartHead
              name={p.name}
              count={p.queued ? count(p.steps) : files(p.steps.length)}
              why={p.why}
              open={open}
              toggle={() => toggle(p.name, false)}
              next={!open && p.steps.some((s) => s.path === next)}
              ask={!p.queued && readPart && (
                <button type="button" className="ord-ask" onClick={() => readPart(p.name)}>
                  <span className="sq-at" aria-hidden="true" />
                  {p.steps.length === 1 ? 'Read this' : `Read these ${p.steps.length}`}
                </button>
              )}
            />
            {open && <ol className="ord">{rows(p.steps)}</ol>}
          </section>
        )
      })}
      {!!leftOff && (
        <p className="ord-left">
          {files(leftOff)} left off the path: no one needs {leftOff === 1 ? 'it' : 'them'} to understand this.{' '}
          {toFiles && <button type="button" className="link" onClick={toFiles}>In Files</button>}
        </p>
      )}
    </div>
  )
}

export function Side({ tab, setTab, files, loading, which, setWhich, change, here, look, go, notes, order, parts, finding, reading, leftOff, readPart, agentRead, agent, read, folders, folder, goFolder, changePage, goChange, overview, width, grip, foot }: {
  tab: Tab
  /** The key, at its foot, below whatever the tab shows. */
  foot?: ReactNode
  /** Its width, when it's beside the work rather than over it. */
  width?: number
  /** Its right edge, to drag wider or narrower. */
  grip?: ReactNode
  setTab: (t: Tab) => void
  /** The files the choice shows. */
  files: SideFile[]
  /** Whether they're still on their way. */
  loading?: boolean
  which: 'some' | 'all'
  setWhich: (w: 'some' | 'all') => void
  change: boolean
  here?: string
  /** Looks at a file ahead of opening it, once you've rested on its row: so it opens at once. */
  look?: (path: string) => void
  go: (path: string) => void
  /** The Notes tab, filtered, when there is one. */
  notes?: (filter: string) => ReactNode
  /** What's open's overview, the first on the way in: the change's own page, or the top folder's. */
  overview?: Overview
  /** The way in, when your agent gave one. */
  order?: WayStep[]
  /** The rest of the path, in parts, once your agent gave it. */
  parts?: WayPart[]
  /** Your agent is finding a way in. */
  finding?: boolean
  /** The files your agent is working through, and how far each is. */
  reading?: StepAt[]
  /** How many files the path leaves off. */
  leftOff?: number
  /** Asks for a part of the path to be read. */
  readPart?: (name: string) => void
  /** Whether your agent has read a file, once that's known. */
  agentRead?: (path: string) => boolean
  /** Your agent, as people say it. */
  agent?: string
  /** Whether you've read a step's file. */
  read?: (path: string) => boolean
  /** What folders are for. */
  folders?: FolderLine[]
  /** The folder whose page is open. */
  folder?: string
  goFolder?: (path: string) => void
  /** Whether the change's own page shows. */
  changePage?: boolean
  /** Opens the change's own page, from the head of its files. */
  goChange?: () => void
}) {
  const [filters, setFilters] = useState<Record<Tab, string>>({ order: '', files: '', notes: '' })
  const filter = filters[tab]
  const lines = useMemo(() => new Map((folders ?? []).map((f) => [f.path, f.line])), [folders])
  const tabs: Array<[Tab, string]> = [
    ...(order?.length || finding ? [['order', 'In order'] as [Tab, string]] : []),
    ['files', 'Files'],
    ...(notes ? [['notes', 'Notes'] as [Tab, string]] : []),
  ]
  const placeholder = tab === 'files' ? (change && which === 'some' ? 'Filter changed files' : 'Filter files') : 'Filter notes'
  // A file's row rested on a moment: it's looked at, so it opens at once.
  const resting = useRef<{ path: string; timer: number } | null>(null)
  const rest = (e: React.MouseEvent) => {
    const path = (e.target as Element).closest<HTMLElement>('[data-look]')?.dataset.look
    if (!look || path === resting.current?.path) return
    if (resting.current) window.clearTimeout(resting.current.timer)
    resting.current = path ? { path, timer: window.setTimeout(() => look(path), LOOK_AFTER) } : null
  }
  const leave = () => {
    if (resting.current) window.clearTimeout(resting.current.timer)
    resting.current = null
  }
  useEffect(() => leave, [])
  return (
    <aside className="side" aria-label="What’s open" style={width ? { width } : undefined} onMouseOver={rest} onMouseLeave={leave}>
      {grip}
      <div className="tabs" role="tablist">
        {tabs.map(([t, label]) => (
          <button key={t} type="button" role="tab" aria-selected={tab === t} className={tab === t ? 'on' : undefined} onClick={() => setTab(t)}>{label}</button>
        ))}
      </div>
      {tab !== 'order' && <label className="filter">
        <svg width="13" height="13" viewBox="0 0 13 13" aria-hidden="true"><circle cx="5.5" cy="5.5" r="4" fill="none" stroke="currentColor" strokeWidth="1.3" /><path d="m8.6 8.6 3 3" stroke="currentColor" strokeWidth="1.3" /></svg>
        <span className="sr-only">{placeholder}</span>
        <input type="search" value={filter} placeholder={placeholder} onChange={(e) => setFilters((f) => ({ ...f, [tab]: e.target.value }))} />
      </label>}
      {tab === 'order' && (order?.length || finding) && (
        <Order order={order ?? []} parts={parts ?? []} overview={overview} finding={finding} reading={reading ?? []} leftOff={leftOff} readPart={readPart} toFiles={() => setTab('files')} here={here} read={read ?? (() => false)} agentRead={agentRead} agent={agent ?? 'your agent'} go={go} />
      )}
      {tab === 'files' && (
        <Files
          files={files}
          loading={loading}
          here={here}
          go={go}
          goFolder={goFolder}
          folder={folder}
          lines={lines}
          filter={filter}
          changed={change && which === 'some'}
          changePage={changePage}
          goChange={goChange}
          kinds={change ? (
            <div className="opts">
              <Choice
                small
                label="Which files"
                value={which}
                onChange={setWhich}
                options={[{ value: 'some', label: 'Changed' }, { value: 'all', label: 'Every file' }]}
              />
            </div>
          ) : undefined}
        />
      )}
      {tab === 'notes' && notes?.(filter)}
      {foot}
    </aside>
  )
}
