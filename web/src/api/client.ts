// Talking to kelve's server, and telling every screen when something it
// shows has changed, here or on the server, so it can fetch again. Cookies
// go with every call; a 401 means this browser is signed out.

import type { ServerEvent } from './types'

/** A failed call, with the server's sentence for people. */
export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message)
  }
}

// ---------------------------------------------------------------- signed out

let out = false
const outListeners = new Set<() => void>()

export const isSignedOut = () => out

/** Calls `fn` once this browser turns out to be signed out. */
export function onSignedOut(fn: () => void) {
  outListeners.add(fn)
  return () => void outListeners.delete(fn)
}

function signedOut() {
  if (out) return
  out = true
  outListeners.forEach((fn) => fn())
}

// ---------------------------------------------------------------- the example

/**
 * The example: answers recorded from the
 * sandbox, by the address each was asked for, so the reader runs with no
 * server. Read only: nothing it does is kept.
 */
export type Fixture = { opened: number; answers: Record<string, unknown> }

let example: Fixture | null = null

/** From now on, every call is answered from the example. */
export function answerFromExample(f: Fixture) {
  example = f
}

export const inExample = () => example !== null

/** A call, answered from the example. */
function fromExample<T>(f: Fixture, path: string, init?: RequestInit): Promise<T> {
  if (init?.method && init.method !== 'GET') return Promise.reject(new ApiError('The example can’t be changed. Open your own work to do that.', 400))
  if (path in f.answers) return Promise.resolve(structuredClone(f.answers[path]) as T)
  return Promise.reject(new ApiError('That isn’t part of the example.', 404))
}

// ---------------------------------------------------------------- calls

async function call<T>(path: string, init?: RequestInit): Promise<T> {
  if (example) return fromExample<T>(example, path, init)
  let res: Response
  try {
    res = await fetch(path, { credentials: 'same-origin', ...init })
  } catch {
    throw new ApiError('kelve didn’t answer. Check that it’s running, then try again.', 0)
  }
  if (res.status === 401) {
    signedOut()
    throw new ApiError('This browser is signed out of kelve.', 401)
  }
  const text = await res.text()
  let body: unknown
  try {
    body = text ? JSON.parse(text) : undefined
  } catch {
    body = undefined
  }
  if (!res.ok) {
    const said = (body as { error?: string } | undefined)?.error
    throw new ApiError(said ?? `Something went wrong (${res.status}). Try again in a moment.`, res.status)
  }
  return body as T
}

const json = (method: string, body: unknown): RequestInit => ({
  method,
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(body),
})

export const get = <T>(path: string) => call<T>(path)
export const post = <T>(path: string, body: unknown = {}) => call<T>(path, json('POST', body))
export const del = <T>(path: string) => call<T>(path, { method: 'DELETE' })

// ---------------------------------------------------------------- changes

/** The screens the server says changed: each fetches again. */
export type Screen = ServerEvent['type']
const SCREENS: Screen[] = ['settings', 'opened', 'notes', 'asks', 'agent']

type Listener = (event?: ServerEvent) => void
const listeners = new Map<Screen, Set<Listener>>()

/** How many changes there have been, here or on the server: what was fetched before the last may be out of date. */
let changes = 0
export const changesSoFar = () => changes

/** Something changed: every screen showing it fetches again. With no screens named, all of them do. */
export function changed(...screens: Screen[]) {
  changes++
  const which = screens.length ? screens : SCREENS
  const fns = new Set<Listener>()
  for (const s of which) listeners.get(s)?.forEach((fn) => fns.add(fn))
  fns.forEach((fn) => fn())
}

/** The server said what changed: the screens showing it hear what. */
function told(event: ServerEvent) {
  changes++
  listeners.get(event.type)?.forEach((fn) => fn(event))
}

/** Calls `fn` when any of these screens changes, with what the server said, when it said. */
export function onChange(screens: Screen[], fn: Listener) {
  for (const s of screens) {
    if (!listeners.has(s)) listeners.set(s, new Set())
    listeners.get(s)!.add(fn)
  }
  return () => screens.forEach((s) => listeners.get(s)?.delete(fn))
}

/**
 * Keeps a report of each error on the page, through kelve, which tidies
 * it and sends it nowhere (Settings shows it). A report that can't be
 * kept is let go: it never makes another.
 */
export function watchPage() {
  let keeping = false
  const keep = (message: string, stack: string) => {
    if (keeping || out || example) return
    keeping = true
    void post('/api/problems', { message, stack })
      .catch(() => {})
      .finally(() => (keeping = false))
  }
  window.addEventListener('error', (e) => keep(String(e.message), e.error instanceof Error ? (e.error.stack ?? '') : ''))
  window.addEventListener('unhandledrejection', (e) => {
    const r = e.reason
    keep(r instanceof Error ? r.message : String(r), r instanceof Error ? (r.stack ?? '') : '')
  })
}

/**
 * Listens to the server's stream of changes, each a `ServerEvent` naming the
 * screen to fetch again. After the stream drops and comes back, everything
 * fetches again, since anything could have changed meanwhile; and if kelve
 * came back on another build, the page reloads, so it's never the old one.
 *
 * Only while the tab is in sight: a browser keeps at most six connections
 * to one address, and every kelve tab's stream holds one, so with six tabs
 * open a seventh never loads. A tab out of sight lets its stream go, and
 * back in sight listens again, as after a drop.
 */
export function listen() {
  if (example) return () => {}
  let source: EventSource | undefined
  let dropped = false
  // The build this page came from: the first the stream names.
  let page: string | undefined
  const open = () => {
    const s = new EventSource('/api/stream')
    s.onmessage = (message) => {
      let event: ServerEvent
      try {
        event = JSON.parse(message.data) as ServerEvent
      } catch {
        return // Not one kelve knows: nothing to fetch.
      }
      if (event.type === 'page') {
        if (page !== undefined && page !== event.build) window.location.reload()
        page ??= event.build
        return
      }
      told(event)
    }
    s.onerror = () => {
      dropped = true
    }
    s.onopen = () => {
      if (dropped) changed()
      dropped = false
    }
    source = s
  }
  const sight = () => {
    if (document.hidden) {
      source?.close()
      source = undefined
      // What it misses meanwhile is fetched once it's back.
      dropped = true
    } else if (!source) {
      open()
    }
  }
  sight()
  document.addEventListener('visibilitychange', sight)
  return () => {
    document.removeEventListener('visibilitychange', sight)
    source?.close()
  }
}
