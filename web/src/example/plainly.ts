// The example's change and folders while they aren't read plainly yet: being
// written, or where they are on your agent's list. Written, they're as the
// example's agent wrote them, in example.json.

import type { ChangePlainly, FolderPlainly, PlainWaiting } from '../api/types'

/** The change, not written yet: being written, or where it is on your agent's list. */
export function changeWaiting(waiting: PlainWaiting): ChangePlainly {
  return { what: null, fits: null, kind: null, change: null, ideas: [], parts: [], tools: [], touches: [], order: [], stale: false, waiting }
}

/** A folder, not written yet: being written, or where it is on your agent's list. */
export function folderWaiting(path: string, waiting: PlainWaiting): FolderPlainly {
  return { path, what: null, change: null, ideas: [], shape: null, parts: [], also: null, keeps: [], stale: false, waiting }
}
