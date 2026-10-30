// Your agent's list (boards Q-1 to Q-9, J-1 to J-3), as every screen reads it:
// what it's doing now, what's next, what was asked for in other things, what's
// ready, what's paused, and where on it something you asked for is.

import { createContext, useContext, useEffect } from 'react'
import type { AgentList, ListItem, PlainWaiting } from './api/types'
import { useLoad, type Loaded } from './useLoad'

/** What to say to your agent to have it work through its list. */
export const DO_WAITING = 'Do what’s waiting for you in kelve.'

/** What's open in this tab: its number and title, for what to say to your agent. */
export const OpenContext = createContext<{ id: number; title: string } | null>(null)

/**
 * What to say to your agent to have it do what's waiting for what's open in
 * this tab, by its number, so an agent works on it alone, however many tabs
 * and agents there are (Nathan, 30 October); anywhere else, its whole list.
 */
export function doWaiting(open: { id: number; title: string } | null): string {
  return open ? `Do what’s waiting for you in kelve for “${open.title}” (number ${open.id}).` : DO_WAITING
}

/** The list, kept fresh as kelve's jobs and notes change, and asked for again when the agent stops counting as working, so it says so on time. */
export function useAgentList(enabled = true): Loaded<AgentList> {
  const list = useLoad<AgentList>(enabled ? '/api/list' : null, ['asks', 'agent', 'notes', 'idea', 'read', 'opened'])
  const { working, workingUntil } = list.data ?? {}
  const { reload } = list
  useEffect(() => {
    if (!working || workingUntil == null) return
    const t = window.setTimeout(reload, Math.max(0, workingUntil - Date.now()) + 1000)
    return () => window.clearTimeout(t)
  }, [working, workingUntil, reload])
  return list
}

/** The list, for any part of the reader, without each loading it again. */
export const ListContext = createContext<AgentList | undefined>(undefined)
export const useList = () => useContext(ListContext)

export const ideaKey = (key: string) => `idea:${key}`
export const noteKey = (id: string) => `note:${id}`

/**
 * Where something stands on the list: being done, waiting, ready, or not
 * on it. Waiting, its place is its row, 1 being the first you see; a page
 * that's part of reading what's open is in that one row, so it says how
 * many files are read before it instead.
 */
export type Where =
  | { state: 'now'; item: ListItem }
  | { state: 'next'; place: number; files?: number; item: ListItem }
  | { state: 'ready'; item: ListItem }
  | null

export function whereIs(list: AgentList | undefined, key: string): Where {
  if (!list) return null
  const now = list.now.find((i) => i.key === key)
  if (now) return { state: 'now', item: now }
  const at = list.next.findIndex((i) => i.key === key)
  if (at >= 0) return { state: 'next', place: list.now.length + at + 1, item: list.next[at] }
  // Asked for in something else: after what's open's.
  const after = list.elsewhere.findIndex((i) => i.key === key)
  if (after >= 0) return { state: 'next', place: list.now.length + list.next.length + after + 1, item: list.elsewhere[after] }
  const ready = list.ready.find((i) => i.key === key)
  return ready ? { state: 'ready', item: ready } : null
}

/**
 * Where a page read plainly is while it isn't written: part of reading
 * what's open there, one row for all of it, so it says how many files are
 * read before it, as kelve counts them, with its own job to move up.
 * None while what it's written from is still being read.
 */
export function plainWhere(list: AgentList | undefined, waiting: PlainWaiting | null | undefined, opened: number): Where {
  if (waiting?.before == null) return null
  const row = whereIs(list, `read:${opened}`)?.item
  const item: ListItem = {
    ...(row ?? { key: `read:${opened}`, kind: 'reading', title: '', sub: null, opened, idea: null, note: null, left: null, askedIn: null, hand: [] }),
    asks: waiting.ask != null ? [waiting.ask] : [],
  }
  return waiting.writing ? { state: 'now', item } : { state: 'next', place: waiting.before + 1, files: waiting.before, item }
}

/** Its place, as the list says it: "6 files before this" or "2nd"; none when it's next. */
export function placeSaid(where: Where): string | null {
  if (where?.state !== 'next') return null
  if (where.files != null) return where.files > 0 ? `${where.files} file${where.files === 1 ? '' : 's'} before this` : null
  return where.place > 1 ? ordinal(where.place) : null
}

/** 1st, 2nd, 3rd, 4th… */
export function ordinal(n: number): string {
  const tens = n % 100
  const end = tens >= 11 && tens <= 13 ? 'th' : ['th', 'st', 'nd', 'rd'][n % 10] ?? 'th'
  return `${n}${n % 10 > 3 ? 'th' : end}`
}

/** How many things are left for the agent: what's paused waits until it's opened again. */
export const toDo = (list: AgentList | undefined) => (list ? list.now.length + list.next.length + list.elsewhere.length : 0)
