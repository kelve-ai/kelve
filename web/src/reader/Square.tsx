// The one mark: a small square is an idea. Red if it's new in this change, grey
// if not; filled until you've read it, hollow once you have; black, on a line,
// for a first principle. Its label says the same in words, for anyone who can't
// tell the colours apart.

import { createContext, useContext } from 'react'

/** How the reader draws an idea's square: new in what's open, and read. */
export type Seen = {
  isNew: (key: string) => boolean
  isRead: (key: string) => boolean
  /** Marks it read now, before the server says so. */
  read: (key: string) => void
}

export const SeenContext = createContext<Seen>({ isNew: () => false, isRead: () => false, read: () => {} })

export const useSeen = () => useContext(SeenContext)

/** A square, in words: "Jitter, new in this change, not read yet". */
export function squareLabel(name: string, isNew: boolean, read: boolean, fp = false): string {
  if (fp) return `${name}, a first principle${read ? '' : ', not read yet'}`
  return `${name}${isNew ? ', new in this change' : ''}${read ? ', read' : ', not read yet'}`
}

export function Square({ ideaKey, fp, size, name }: {
  ideaKey: string
  fp?: boolean
  size?: 'small' | 'big'
  /** Given, the square is labelled for screen readers; without, it's left out. */
  name?: string
}) {
  const seen = useSeen()
  const isNew = seen.isNew(ideaKey)
  const read = seen.isRead(ideaKey)
  const cls = ['sq', isNew && !fp ? 'new' : '', read && !fp ? 'read' : '', fp ? 'fp' : '', size ?? ''].filter(Boolean).join(' ')
  return name ? <i className={cls} role="img" aria-label={squareLabel(name, isNew, read, fp)} /> : <i className={cls} aria-hidden="true" />
}
