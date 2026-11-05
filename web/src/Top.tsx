// The reader's top: kelve's square and its name, the way home, which says so
// when pointed at, then what's open: its project and title. Settings sit at
// the right, then light or dark.

import { type ReactNode, useEffect, useState } from 'react'
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

export function Top({ children, right, home = '#/', settings = '#/settings', esc }: {
  /** Where kelve's name leads. */
  home?: string
  /** Escape goes there too, with nothing open beside the work: the reader's. */
  esc?: boolean
  /** Where settings are: the example's own, in the example. */
  settings?: string | null
  children?: ReactNode
  /** Before settings, at the right. */
  right?: ReactNode
}) {
  return (
    <header className="topbar">
      <a className="logo" href={home} aria-label="kelve: home" aria-describedby="logo-tip">
        <i aria-hidden="true" />kelve
        <span className="logo-tip" id="logo-tip" role="tooltip">{esc ? 'Home, or Esc' : 'Home'}</span>
      </a>
      {children}
      <span className="sp" />
      {right}
      {settings && <a className="settings-key" href={settings}>Settings</a>}
      <ThemeSwitch />
    </header>
  )
}
