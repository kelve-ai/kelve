// A step on the way through what's open is read only once it's seen as
// your agent left it: a file read plainly, or the overview, the change's
// or the top folder's own page, with what it says; a file read as code,
// with its marks done (Nathan, 29 October). Opening one before your agent
// has written it isn't reading it.

import { createContext, useContext, useEffect } from 'react'

/** Told the path of what's seen as your agent left it: "" for the overview. */
export const SeenPlainly = createContext<(path: string) => void>(() => {})

/** Says `path` is seen plainly, once it's `shown` with what it says. */
export function useSeenPlainly(path: string | null, shown: boolean) {
  const seen = useContext(SeenPlainly)
  useEffect(() => {
    if (path != null && shown) seen(path)
  }, [path, shown, seen])
}
