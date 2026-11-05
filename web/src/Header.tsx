// The top of every page (board Shell): kelve's square and its name, the three
// places, Home, Skills and Your agent, then Settings, and light or dark, one
// click away. Meeting your agent has only kelve's name, and a way to skip.

import type { ReactNode } from 'react'
import { hrefOf } from './routes'
import { ThemeSwitch } from './Top'

const PLACES = [
  { place: 'home', label: 'Home', path: '' },
  { place: 'skills', label: 'Skills', path: 'skills' },
  { place: 'agent', label: 'Your agent', path: 'agent' },
] as const

const GEAR = (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z" />
  </svg>
)

export function Header({ here, example, bare, actions }: {
  /** The place the page is under, said as the current page. */
  here?: 'home' | 'skills' | 'agent' | 'settings' | null
  /** In the example: every link stays in it. */
  example?: boolean
  /** Only kelve's name, and a way to skip: for meeting your agent. */
  bare?: boolean
  /** Only kelve's name, and these: Cancel and Save, for changing how your agent looks. */
  actions?: ReactNode
}) {
  return (
    <header className="hd">
      <div className="hd-l">
        <a className="hd-logo" href={hrefOf('', example)}>
          <i aria-hidden="true" />kelve
        </a>
        {!bare && !actions && (
          <nav className="hd-nav" aria-label="kelve">
            {PLACES.map((p) => (
              <a key={p.place} href={hrefOf(p.path, example)} aria-current={here === p.place ? 'page' : undefined}>
                {p.label}
              </a>
            ))}
          </nav>
        )}
      </div>
      <div className="hd-r">
        {actions ? (
          actions
        ) : bare ? (
          <a className="btn btn-g btn-sm" href={hrefOf('', example)}>Skip for now</a>
        ) : (
          <a className="hd-set" href={hrefOf('settings/agent', example)} aria-current={here === 'settings' ? 'page' : undefined}>
            {GEAR}Settings
          </a>
        )}
        {!actions && <ThemeSwitch />}
      </div>
    </header>
  )
}
