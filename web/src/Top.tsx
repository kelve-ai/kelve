// The top of every page: kelve's square and its name, the way to the start
// page, which says so when pointed at (board Home-1), then what the page is
// about: in the reader, the project and the path through entries; elsewhere,
// the way back to what was last open. Search sits at the right, where there's
// something to search, then settings, and light or dark, on every page.

import { type ReactNode, useEffect, useState } from 'react'
import { useAgentList } from './agentList'
import type { Settings } from './api/types'
import { lastOpen } from './lastOpen'
import { AgentLine } from './reader/ListPane'
import { useLoad } from './useLoad'
import { agentKindName } from './words'
import { onTheme, setTheme, shownTheme } from './theme'

/** Light or dark, one click away at the top of every page. */
export function ThemeSwitch() {
  const [shown, setShown] = useState(shownTheme)
  useEffect(() => onTheme(() => setShown(shownTheme())), [])
  const next = shown === 'dark' ? 'light' : 'dark'
  return (
    <button type="button" className="theme-switch" onClick={() => setTheme(next)} aria-label={`Switch to the ${next} theme`} title={`Switch to the ${next} theme`}>
      <i aria-hidden="true" />
    </button>
  )
}

const MAC = /Mac|iPhone|iPad/.test(navigator.platform)

export function Top({ search, children, right, home = '#/', settings = '#/you', here, esc }: {
  /** Opens search, where there's something to search. */
  search?: () => void
  /** Where kelve's name leads. */
  home?: string
  /** Escape goes there too, with nothing open beside the work: the reader's. */
  esc?: boolean
  /** Where settings are: the example's own, in the example. */
  settings?: string | null
  /** The page showing, for the link that leads to it. */
  here?: 'settings' | 'start'
  children?: ReactNode
  /** Before search, at the right. */
  right?: ReactNode
}) {
  return (
    <header className="topbar">
      <a className="logo" href={home} aria-label="kelve: the start page" aria-describedby="logo-tip">
        <i aria-hidden="true" />kelve
        <span className="logo-tip" id="logo-tip" role="tooltip">{esc ? 'Start page · Esc' : 'Start page'}</span>
      </a>
      {children}
      <span className="sp" />
      {right}
      {search && (
        <button type="button" className="search-key" onClick={search}>
          Search <span aria-label={MAC ? 'Command K' : 'Control K'}>{MAC ? '⌘K' : 'Ctrl K'}</span>
        </button>
      )}
      {settings && <a className="settings-key" href={settings} aria-current={here === 'settings' ? 'page' : undefined}>Settings</a>}
      <ThemeSwitch />
    </header>
  )
}

/** The top of a page that isn't the reader: the way back to what was open, and your agent's list. */
export function PageTop({ here, settings }: { here?: 'settings' | 'start'; settings?: string | null }) {
  const back = lastOpen()
  const list = useAgentList()
  const mine = useLoad<Settings>('/api/settings', ['settings'])
  const agent = agentKindName(mine.data?.agent)
  // The list opens beside what was open last.
  const openList = back ? () => { location.hash = `${back.hash.split('?')[0]}?k=list` } : undefined
  return (
    <Top
      here={here}
      settings={settings}
      right={
        <>
          {back && (
            <nav className="top-links" aria-label="kelve">
              <a href={back.hash}>Back to {back.title}</a>
            </nav>
          )}
          {openList && <AgentLine list={list.data} agent={agent} onClick={openList} />}
        </>
      }
    />
  )
}
