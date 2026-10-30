// Numbers and times as people say them: "two", "about three minutes",
// "this morning". Rough times only, never clock times.

import { useEffect, useState } from 'react'
import type { AgentKind, Timestamp } from './api/types'

const COUNT = ['no', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine', 'ten', 'eleven', 'twelve']

/** A small number in words, and a bigger one in figures. */
export const say = (n: number) => COUNT[n] ?? String(n)

/** A list as a sentence says it: "Codex, Cursor and Antigravity CLI". */
export function and(items: string[]): string {
  if (items.length <= 1) return items.join('')
  return `${items.slice(0, -1).join(', ')} and ${items[items.length - 1]}`
}

/** A node's name as people write it: "control plane", and "volume (audio)" for one that shares its name. */
export const name = (n: string) => {
  const [words, sense] = n.split('--')
  return sense ? `${words.replace(/-/g, ' ')} (${sense.replace(/-/g, ' ')})` : words.replace(/-/g, ' ')
}

const DAY = 24 * 60 * 60 * 1000

function startOfDay(d: Date) {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()
}

/** When something happened, roughly: "just now", "this morning", "yesterday", "on Friday", "on 3 October". */
export function roughly(at: Timestamp, now = new Date()): string {
  const t = new Date(at)
  if (Number.isNaN(t.getTime())) return 'a while ago'
  const ago = now.getTime() - t.getTime()
  if (ago < 5 * 60 * 1000) return 'just now'
  const days = Math.round((startOfDay(now) - startOfDay(t)) / DAY)
  if (days <= 0) {
    if (ago < 60 * 60 * 1000) return 'in the last hour'
    const h = t.getHours()
    return h < 12 ? 'this morning' : h < 18 ? 'this afternoon' : 'this evening'
  }
  if (days === 1) return 'yesterday'
  if (days < 7) return `on ${t.toLocaleDateString('en-GB', { weekday: 'long' })}`
  return `on ${dayMonth(at)}`
}

/** "3 October". */
export function dayMonth(at: Timestamp): string {
  return new Date(at).toLocaleDateString('en-GB', { day: 'numeric', month: 'long' })
}

/** Below this width, the page is a phone's: one column, figures stacked. */
const NARROW = '(max-width: 760px)'

export function useNarrow() {
  const [narrow, setNarrow] = useState(() => window.matchMedia(NARROW).matches)
  useEffect(() => {
    const m = window.matchMedia(NARROW)
    const on = () => setNarrow(m.matches)
    m.addEventListener('change', on)
    return () => m.removeEventListener('change', on)
  }, [])
  return narrow
}

/** Whether to keep still: the system asks for less motion. */
export const stillPlease = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches

/** Dollars, to the cent: "$0.84". */
export const dollars = (v: number) => `$${v.toFixed(2)}`

/** The agents kelve knows by the names they give themselves. */
const AGENTS: Record<string, string> = {
  'claude-code': 'Claude Code',
  claude: 'Claude Code',
  codex: 'Codex',
  'codex-mcp-client': 'Codex',
  cursor: 'Cursor',
  'cursor-vscode': 'Cursor',
  copilot: 'Copilot',
  'github-copilot': 'Copilot',
  'visual-studio-code': 'Copilot',
  gemini: 'Gemini CLI',
  'gemini-cli': 'Gemini CLI',
}

/** An agent as people say it: "Claude Code" for `claude-code`; "your agent" when none has said hello. */
export function agentName(name: string | null | undefined): string {
  if (!name) return 'your agent'
  const known = AGENTS[name.toLowerCase()]
  if (known) return known
  const words = name.replace(/[-_]+/g, ' ').trim()
  return words ? words.charAt(0).toUpperCase() + words.slice(1) : 'your agent'
}

/** How long ago, in a few words: "20 seconds ago", "6 minutes ago", "2 hours ago". */
export function ago(at: Timestamp, now = new Date()): string {
  const s = Math.max(0, Math.round((now.getTime() - new Date(at).getTime()) / 1000))
  if (s < 10) return 'just now'
  if (s < 60) return `${s} seconds ago`
  const m = Math.round(s / 60)
  if (m < 60) return m === 1 ? 'a minute ago' : `${m} minutes ago`
  const h = Math.round(m / 60)
  if (h < 24) return h === 1 ? 'an hour ago' : `${h} hours ago`
  return roughly(at, now)
}

/** The agent the person chose in settings, as people say it. */
export function agentKindName(kind: AgentKind | null | undefined): string {
  switch (kind) {
    case 'claudeCode': return 'Claude Code'
    case 'codex': return 'Codex'
    case 'cursor': return 'Cursor'
    case 'gemini': return 'Gemini CLI'
    default: return 'your agent'
  }
}
