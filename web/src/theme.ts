// Light or dark. Until you choose, kelve follows your system. The choice is
// kept in this browser; index.html applies it before the page first draws.

export type Theme = 'light' | 'dark' | 'system'

const KEY = 'kelve:theme'
const CHANGED = 'kelve:theme'

export function getTheme(): Theme {
  try {
    const t = localStorage.getItem(KEY)
    return t === 'light' || t === 'dark' ? t : 'system'
  } catch {
    return 'system'
  }
}

/** What the page shows now: your choice, or your system's. */
export function shownTheme(): 'light' | 'dark' {
  const t = getTheme()
  if (t !== 'system') return t
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

export function setTheme(t: Theme) {
  try {
    if (t === 'system') localStorage.removeItem(KEY)
    else localStorage.setItem(KEY, t)
  } catch {
    // Storage can be off; the choice then lasts until the page closes.
  }
  if (t === 'system') delete document.documentElement.dataset.theme
  else document.documentElement.dataset.theme = t
  window.dispatchEvent(new Event(CHANGED))
}

/** Calls `fn` when the theme changes, here or in the system's settings. */
export function onTheme(fn: () => void) {
  const media = window.matchMedia('(prefers-color-scheme: dark)')
  window.addEventListener(CHANGED, fn)
  media.addEventListener('change', fn)
  return () => {
    window.removeEventListener(CHANGED, fn)
    media.removeEventListener('change', fn)
  }
}
