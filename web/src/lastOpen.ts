// What was last open in the reader, and where in it: the way back from
// settings and the start page. Kept for this browser tab, as a
// convenience: a browser that won't keep it just has no way back shown.

const KEY = 'kelve.lastOpen'

export type LastOpen = {
  /** The reader's address, with the file and idea it showed. */
  hash: string
  title: string
}

export function rememberOpen(open: LastOpen) {
  try {
    sessionStorage.setItem(KEY, JSON.stringify(open))
  } catch {
    // Not kept: there's no way back shown, that's all.
  }
}

export function lastOpen(): LastOpen | null {
  try {
    const kept = sessionStorage.getItem(KEY)
    const open = kept ? (JSON.parse(kept) as LastOpen) : null
    return open && open.hash.startsWith('#/o/') ? open : null
  } catch {
    return null
  }
}
