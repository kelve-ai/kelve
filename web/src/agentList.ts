// Your agent's list (boards Q-1 to Q-9, J-1 to J-3), as every screen reads it:
// what it's doing now, what's next, what was asked for in other things, and
// what's ready.

import { createContext, useEffect } from 'react'
import type { AgentList } from './api/types'
import { useLoad, type Loaded } from './useLoad'

/** What to say to your agent to have it work through its list. */
export const DO_WAITING = 'Do kelve’s jobs.'

/** How to start what's waiting, wherever something is asked of your agent. */
export const START_NOW = `To start now, tell it: ${DO_WAITING}`

/** The same, for what may have started already. */
export const IF_NOT_STARTED = `If it hasn’t started, tell it: ${DO_WAITING}`

/** What's open in this tab: its number and title, for what to say to your agent. */
export const OpenContext = createContext<{ id: number; title: string } | null>(null)

/**
 * What to say to your agent to have it do what's waiting for what's open in
 * this tab, by its number, so an agent works on it alone, however many tabs
 * and agents there are (Nathan, 30 October); anywhere else, its whole list.
 */
export function doWaiting(open: { id: number; title: string } | null): string {
  return open ? `Do kelve’s jobs for “${open.title}” (number ${open.id}).` : DO_WAITING
}

/** The list, kept fresh as kelve's jobs change, and asked for again when the agent stops counting as working, so it says so on time. */
export function useAgentList(enabled = true): Loaded<AgentList> {
  const list = useLoad<AgentList>(enabled ? '/api/list' : null, ['asks', 'agent', 'opened'])
  const { working, workingUntil } = list.data ?? {}
  const { reload } = list
  useEffect(() => {
    if (!working || workingUntil == null) return
    const t = window.setTimeout(reload, Math.max(0, workingUntil - Date.now()) + 1000)
    return () => window.clearTimeout(t)
  }, [working, workingUntil, reload])
  return list
}

/** How many things are left for the agent: what's paused waits until it's opened again. */
export const toDo = (list: AgentList | undefined) => (list ? list.now.length + list.next.length + list.elsewhere.length : 0)
