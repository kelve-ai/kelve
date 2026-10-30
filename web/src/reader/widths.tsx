// How wide the panes beside the work are, each as wide as what's in it
// reads best at: an entry, the one before it, a note's page, your agent's
// list; and the side panel. Drag a pane's left
// edge, or the side panel's right edge, to change it; what you set is kept
// in this browser, for every pane of its kind.

import { useCallback, useEffect, useRef, useState } from 'react'

export type PaneKind = 'entry' | 'back' | 'note' | 'side' | 'list'

// Each fits beside the side panel and the work on a laptop of 1440: an
// entry, the side panel and the least the work needs come to 1420.
const DEFAULTS: Record<PaneKind, number> = {
  // Prose at about 70 characters a line, with room for diagrams and code.
  entry: 600,
  // Glanced at, not read.
  back: 440,
  // What you write, at the measure of what you read.
  note: 600,
  // One line each: a kind, a path, where it stands.
  list: 480,
  side: 340,
}
// Only what you set is kept, so the defaults reach you as they change.
const KEY = 'kelve:widths'

/** Narrower than this, a pane can't be read. */
const LEAST = 340
/** What a pane always leaves for the work. */
const LEAVES = 420

/** The least the work needs beside the side panel. */
export const WORK_LEAST = 480
/** The side panel's narrowest and widest. */
export const SIDE_LEAST = 200
export const SIDE_MOST = 520

function kept(): Record<PaneKind, number> {
  try {
    const k = JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<Record<PaneKind, number>>
    return { ...DEFAULTS, ...Object.fromEntries(Object.entries(k).filter(([, v]) => typeof v === 'number')) }
  } catch {
    return { ...DEFAULTS }
  }
}

export function usePaneWidths(): [Record<PaneKind, number>, (kind: PaneKind, width: number) => void] {
  const [widths, setWidths] = useState(kept)
  const set = useCallback((kind: PaneKind, width: number) => {
    setWidths((w) => ({ ...w, [kind]: Math.round(width) }))
    try {
      const mine = JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<Record<PaneKind, number>>
      localStorage.setItem(KEY, JSON.stringify({ ...mine, [kind]: Math.round(width) }))
    } catch {
      // Storage can be off; the width then lasts until the page closes.
    }
  }, [])
  return [widths, set]
}

/** The window's width, as it changes. */
export function useWindowWidth() {
  const [width, setWidth] = useState(() => window.innerWidth)
  useEffect(() => {
    const on = () => setWidth(window.innerWidth)
    window.addEventListener('resize', on)
    return () => window.removeEventListener('resize', on)
  }, [])
  return width
}

const fitPane = (w: number) => Math.max(LEAST, Math.min(w, window.innerWidth - LEAVES))
const fitSide = (w: number) => Math.max(SIDE_LEAST, Math.min(w, SIDE_MOST, window.innerWidth - WORK_LEAST))

/** A pane's left edge, or the side panel's right edge (`right`), to drag wider or narrower; arrow keys move it too. */
export function Grip({ width, set, label, right }: { width: number; set: (width: number) => void; label: string; right?: boolean }) {
  const from = useRef<{ x: number; width: number } | null>(null)
  const fit = right ? fitSide : fitPane
  // Dragging away from what it's beside widens it: right for the side panel, left for a pane.
  const way = right ? 1 : -1
  return (
    <div
      className={`pane-grip${right ? ' right' : ''}`}
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={width}
      aria-valuemin={right ? SIDE_LEAST : LEAST}
      tabIndex={0}
      onPointerDown={(e) => {
        e.preventDefault()
        e.currentTarget.setPointerCapture(e.pointerId)
        from.current = { x: e.clientX, width }
        document.body.classList.add('resizing')
      }}
      onPointerMove={(e) => {
        if (from.current) set(fit(from.current.width + way * (e.clientX - from.current.x)))
      }}
      onPointerUp={() => {
        from.current = null
        document.body.classList.remove('resizing')
      }}
      onKeyDown={(e) => {
        if (e.key === 'ArrowLeft') set(fit(width - way * 24))
        else if (e.key === 'ArrowRight') set(fit(width + way * 24))
        else return
        e.preventDefault()
      }}
    />
  )
}
