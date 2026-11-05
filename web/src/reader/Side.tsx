// The side panel (boards X-1 to X-3): the files, with a filter. They're a
// tree; for a change, it says how much changed, and
// each file how many lines it added and took out. The tree scrolls on its
// own, and is drawn as it scrolls, so 20,000 files feel the same as 20.

import { type ReactNode, useEffect, useMemo, useRef, useState } from 'react'
import { Loader } from '../Loader'
import { byName, type Dir as TreeDir, tree } from '../ui/FileTree'
import { Choice } from '../ui/ui'

/** A file as the side panel lists it; in a change, with the lines it added and took out. */
export type SideFile = { path: string; skip?: string | null; gone?: boolean; added?: number; removed?: number }

/** How long a file's row is rested on before the file is looked at, so passing over rows looks at none. */
const LOOK_AFTER = 80

type Row =
  | { kind: 'dir'; path: string; name: string; depth: number; open: boolean }
  | { kind: 'file'; file: SideFile; name: string; depth: number }

type Dir = TreeDir<SideFile>

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

/** A change's files, all told: how many, and the lines they added and took out. */
function Changed({ files }: { files: SideFile[] }) {
  const added = files.reduce((n, f) => n + (f.added ?? 0), 0)
  const removed = files.reduce((n, f) => n + (f.removed ?? 0), 0)
  return (
    <p className="lgroup changed">
      <b>Changed</b>
      <span>{files.length === 1 ? '1 file' : `${files.length} files`}</span>
      <Counts added={added} removed={removed} />
    </p>
  )
}

const ROW = 28

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

function Files({ files, loading, here, go, filter, kinds, changed }: {
  files: SideFile[]
  /** Still on its way. */
  loading?: boolean
  here?: string
  go: (path: string) => void
  filter: string
  kinds?: ReactNode
  /** A change's own files: headed with how much changed. */
  changed?: boolean
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
  const at = list.findIndex((r) => r.kind === 'file' && r.file.path === here)
  const guides = (depth: number) => Array.from({ length: depth }, (_, i) => <span key={i} className="guide" style={{ left: 14 + i * 16 }} />)
  return (
    <>
      {kinds}
      {changed && files.length > 0 && <Changed files={files} />}
      {!list.length && <p className="side-none">{loading ? <Loader size="small" /> : filter ? 'No file goes by that name.' : 'Nothing to show yet.'}</p>}
      <Virtual
        items={list}
        at={at}
        here={here}
        keyOf={(r) => (r.kind === 'dir' ? `d:${r.path}` : `f:${r.file.path}`)}
        render={(r) =>
          r.kind === 'dir' ? (
            <div className="tr dir" role="treeitem" aria-expanded={r.open} style={{ paddingLeft: 8 + r.depth * 16 }}>
              {guides(r.depth)}
              <button type="button" className="ch" aria-label={r.open ? `Close ${r.name}` : `Open ${r.name}`} onClick={() => setToggled((t) => new Map(t).set(r.path, !r.open))}>
                {CHEVRON(r.open)}
              </button>
              <button type="button" className="nm" onClick={() => setToggled((t) => new Map(t).set(r.path, !r.open))}>
                {r.name.split('/').map((p, i) => <span key={i}>{i > 0 && <i>/</i>}{p}</span>)}
              </button>
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
              <Counts added={r.file.added} removed={r.file.removed} />
            </button>
          )
        }
      />
    </>
  )
}

export function Side({ files, loading, which, setWhich, change, here, look, go, width, grip }: {
  /** Its width, when it's beside the work rather than over it. */
  width?: number
  /** Its right edge, to drag wider or narrower. */
  grip?: ReactNode
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
}) {
  const [filter, setFilter] = useState('')
  const placeholder = change && which === 'some' ? 'Filter changed files' : 'Filter files'
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
      <label className="filter">
        <svg width="13" height="13" viewBox="0 0 13 13" aria-hidden="true"><circle cx="5.5" cy="5.5" r="4" fill="none" stroke="currentColor" strokeWidth="1.3" /><path d="m8.6 8.6 3 3" stroke="currentColor" strokeWidth="1.3" /></svg>
        <span className="sr-only">{placeholder}</span>
        <input type="search" value={filter} placeholder={placeholder} onChange={(e) => setFilter(e.target.value)} />
      </label>
      <Files
        files={files}
        loading={loading}
        here={here}
        go={go}
        filter={filter}
        changed={change && which === 'some'}
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
    </aside>
  )
}
