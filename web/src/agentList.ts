// Your agent's list (boards Q-1 to Q-9, J-1 to J-3), as every screen reads it:
// what it's doing now, what's next, what was asked for in other things, what's
// ready, what's paused, and where on it something you asked for is.

import { createContext, useContext, useEffect } from 'react'
import type { AgentList, ListItem } from './api/types'
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
  const list = useLoad<AgentList>(enabled ? '/api/list' : null, ['asks', 'agent', 'notes', 'opened'])
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

export const noteKey = (id: string) => `note:${id}`

/**
 * Where something stands on the list: being done, waiting, ready, or not
 * on it. Waiting, its place is its row, 1 being the first you see.
 */
export type Where =
  | { state: 'now'; item: ListItem }
  | { state: 'next'; place: number; item: ListItem }
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

/** 1st, 2nd, 3rd, 4th… */
export function ordinal(n: number): string {
  const tens = n % 100
  const end = tens >= 11 && tens <= 13 ? 'th' : ['th', 'st', 'nd', 'rd'][n % 10] ?? 'th'
  return `${n}${n % 10 > 3 ? 'th' : end}`
}

/** How many things are left for the agent: what's paused waits until it's opened again. */
export const toDo = (list: AgentList | undefined) => (list ? list.now.length + list.next.length + list.elsewhere.length : 0)
