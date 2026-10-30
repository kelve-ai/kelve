// Fetching what a screen shows, and fetching it again whenever it changes.
// What was fetched is kept by its path, so coming back to something shows it
// at once while it's fetched again, and what's likely next can be fetched
// before it's opened.

import { useCallback, useEffect, useRef, useState } from 'react'
import { changesSoFar, get, onChange, type Screen } from './api/client'
import type { ServerEvent } from './api/types'

export type Loaded<T> = {
  data?: T
  error?: string
  /** Whether what it shows was fetched since it was opened, not only kept from before, which may be out of date. */
  fresh?: boolean
  /** Fetch it again now. */
  reload: () => void
}

/** What was fetched for a path: when, and whether it was only looked at, with nothing asked for it. */
type Kept = { data: unknown; changes: number; when: number; ahead: boolean }

/** What each path last fetched, the latest last. */
const kept = new Map<string, Kept>()

/** How many paths are kept. */
const KEEP = 200

/** Fetched this lately, with nothing changed since, it isn't fetched again as it's shown. */
const FRESH = 2_000

/** A fetch under way, for a path. */
type Going = { done: Promise<unknown>; changes: number; ahead: boolean }
const going = new Map<string, Going>()

/** Fetches `path`, or with `look`, the same only looked at, and keeps it as `path`'s. */
function fetchKept(path: string, look?: string): Going {
  const changes = changesSoFar()
  const done = get<unknown>(look ?? path).then((data) => {
    kept.delete(path)
    kept.set(path, { data, changes, when: Date.now(), ahead: !!look })
    if (kept.size > KEEP) kept.delete(kept.keys().next().value as string)
    return data
  })
  const g: Going = { done, changes, ahead: !!look }
  going.set(path, g)
  const off = () => {
    if (going.get(path) === g) going.delete(path)
  }
  done.then(off, off)
  return g
}

/** Whether what `path` shows is kept, so it can show at once. */
export const isKept = (path: string) => kept.has(path)

/**
 * Fetches what the person just opened, unless it's kept: settles once
 * there's something to show. With `look`, it's only looked at.
 */
export function fetchNow(path: string, look?: string): Promise<unknown> {
  if (kept.has(path)) return Promise.resolve()
  return (going.get(path) ?? fetchKept(path, look)).done.catch(() => {})
}

/**
 * Fetches what's likely opened next, so it shows at once, unless it's kept
 * or on its way. `look` is the same address asking only to look: nothing
 * is started for it, and opening it asks again.
 */
export function fetchAhead(path: string, look?: string) {
  if (kept.has(path) || going.has(path)) return
  fetchKept(path, look).done.catch(() => {})
}

/**
 * GETs `path`, and again whenever one of `screens` changes, unless `about`
 * says what the server changed isn't what this shows. What it showed stays
 * while it fetches again, so nothing blinks, and what was fetched for a
 * path before shows at once while it's fetched again. One fetch runs at a
 * time: a burst of changes while it runs makes one more fetch after it,
 * not one each. A null path fetches nothing.
 */
export function useLoad<T>(path: string | null, screens: Screen[], about?: (event: ServerEvent) => boolean): Loaded<T> {
  const [state, setState] = useState<{ path: string | null; data?: T; error?: string; fresh?: boolean }>(() => ({ path, data: path ? (kept.get(path)?.data as T) : undefined }))
  const running = useRef<{ path: string; again: boolean } | null>(null)
  const key = screens.join(',')
  const aboutRef = useRef(about)
  aboutRef.current = about

  const load = useCallback(
    (opening = false) => {
      if (!path) return
      if (running.current?.path === path) {
        running.current.again = true
        return
      }
      const had = kept.get(path)
      // Just fetched as it was opened, and nothing's changed since: shown as it is.
      if (opening && had && !had.ahead && had.changes === changesSoFar() && Date.now() - had.when < FRESH) {
        setState((s) => (s.path === path && s.data === had.data && s.fresh ? s : { path, data: had.data as T, fresh: true }))
        return
      }
      const run = { path, again: false }
      running.current = run
      const settle = () => {
        if (running.current !== run) return
        running.current = null
        if (run.again) load()
      }
      // A fetch for it already on its way, since the last change, is this one too.
      const now = going.get(path)
      const fetching = now && !now.ahead && now.changes === changesSoFar() ? now : fetchKept(path)
      // One only looking shows meanwhile, if it comes first.
      if (now?.ahead) {
        now.done.then(
          (data) => {
            if (running.current === run) setState((s) => (s.path === path && s.data !== undefined ? s : { path, data: data as T }))
          },
          () => {},
        )
      }
      fetching.done.then(
        (data) => {
          if (running.current === run) setState({ path, data: data as T, fresh: true })
          settle()
        },
        (e: Error) => {
          if (running.current === run) setState((s) => (s.path === path ? { ...s, error: e.message } : { path, data: kept.get(path)?.data as T, error: e.message }))
          settle()
        },
      )
    },
    [path],
  )

  useEffect(() => {
    load(true)
    const off = onChange(key ? (key.split(',') as Screen[]) : [], (event) => {
      if (event && aboutRef.current && !aboutRef.current(event)) return
      load()
    })
    return () => {
      off()
      // A fetch for what this no longer shows is let go.
      running.current = null
    }
  }, [load, key])

  const reload = useCallback(() => load(), [load])
  // A different path is a different thing: what it fetched before, if anything, not the last one's.
  if (state.path !== path) return { data: path ? (kept.get(path)?.data as T | undefined) : undefined, reload }
  return { data: state.data, error: state.error, fresh: state.fresh, reload }
}

/** How long what's shown stays while what's next loads, before it goes. */
const HOLD = 150

/**
 * What a view shows, held while the next thing it's to show loads: what it
 * showed stays, rather than a blank or a loader, until the next is in or a
 * moment has passed. `id` says what it shows; `ready`, whether what's next
 * is in already; `load` fetches it.
 */
export function useHeld<T>(id: string, view: T, ready: boolean, load: () => Promise<unknown>): T {
  const shown = useRef<{ id: string; view: T } | null>(null)
  const [released, release] = useState<string | null>(null)
  const holding = !!shown.current && shown.current.id !== id && !ready && released !== id
  const loadRef = useRef(load)
  loadRef.current = load
  useEffect(() => {
    if (!holding) return
    let done = false
    const go = () => {
      if (done) return
      done = true
      release(id)
    }
    void loadRef.current().then(go, go)
    const t = window.setTimeout(go, HOLD)
    return () => {
      done = true
      window.clearTimeout(t)
    }
  }, [holding, id])
  if (holding && shown.current) return shown.current.view
  shown.current = { id, view }
  return view
}
