// Files as they sit in their folders, as an editor's explorer shows them:
// each folder inside its parent, one name a row, folders first. A debrief and
// its change view list files this way, and the reader's side builds its own
// rows from the same tree.

import { type ReactNode, useEffect, useState } from 'react'

export type Dir<F> = { name: string; path: string; dirs: Map<string, Dir<F>>; files: F[] }

/** The folders and files as a tree. */
export function tree<F extends { path: string }>(files: F[]): Dir<F> {
  const root: Dir<F> = { name: '', path: '', dirs: new Map(), files: [] }
  for (const f of files) {
    const parts = f.path.split('/')
    let at = root
    for (let i = 0; i < parts.length - 1; i++) {
      const path = parts.slice(0, i + 1).join('/')
      let next = at.dirs.get(parts[i])
      if (!next) at.dirs.set(parts[i], (next = { name: parts[i], path, dirs: new Map(), files: [] }))
      at = next
    }
    at.files.push(f)
  }
  return root
}

/** Each by name; tooling's dot-folders last. */
export const byName = (a: string, b: string) => Number(a.startsWith('.')) - Number(b.startsWith('.')) || a.localeCompare(b)

const dirsOf = <F,>(dir: Dir<F>) => [...dir.dirs.values()].sort((a, b) => byName(a.name, b.name))
const filesOf = <F extends { path: string }>(dir: Dir<F>) => [...dir.files].sort((a, b) => byName(a.path, b.path))

/** Every file, in the order the tree lists them. */
export function inOrder<F extends { path: string }>(dir: Dir<F>): F[] {
  return [...dirsOf(dir).flatMap((d) => inOrder(d)), ...filesOf(dir)]
}

/** Every folder under `dir`. */
const allDirs = <F,>(dir: Dir<F>): Dir<F>[] => [...dir.dirs.values()].flatMap((d) => [d, ...allDirs(d)])

/** How many files a folder holds, all the way down, with those not listed. */
function counted<F>(dir: Dir<F>, more: Record<string, number>): number {
  return dir.files.length + (more[dir.path] ?? 0) + [...dir.dirs.values()].reduce((n, d) => n + counted(d, more), 0)
}

type Props<F> = {
  files: F[]
  /** A file's row, around its name. */
  file: (f: F, name: ReactNode) => ReactNode
  /** Whether a folder starts open, by its path. */
  startsOpen?: (path: string) => boolean
  /** Open everything, whatever was closed: while finding a file. */
  allOpen?: boolean
  /** The file being looked at: its folders open as it's reached. */
  current?: string | null
  /** Files in a folder that aren't listed, by the folder's path; the top folder's is ''. */
  more?: Record<string, number>
  /** What a folder's row says of how many files it holds. */
  note?: (count: number) => string
}

export function FileTree<F extends { path: string }>({ files, file, startsOpen, allOpen, current, more = {}, note }: Props<F>) {
  const root = tree(files)
  const [closed, setClosed] = useState<ReadonlySet<string>>(() => new Set(startsOpen ? allDirs(root).map((d) => d.path).filter((p) => !startsOpen(p)) : []))
  useEffect(() => {
    if (!current) return
    setClosed((was) => {
      const left = [...was].filter((p) => !current.startsWith(`${p}/`))
      return left.length === was.size ? was : new Set(left)
    })
  }, [current])

  const toggle = (path: string) =>
    setClosed((was) => {
      const next = new Set(was)
      if (!next.delete(path)) next.add(path)
      return next
    })

  const list = (dir: Dir<F>): ReactNode => (
    <ul className="ft-list">
      {dirsOf(dir).map((d) => {
        const open = allOpen || !closed.has(d.path)
        return (
          <li key={d.path}>
            <button type="button" className="ft-dir" aria-expanded={open} onClick={() => toggle(d.path)}>
              <svg className="ft-chev" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" style={{ transform: open ? 'rotate(90deg)' : undefined }}>
                <path d="m9 6 6 6-6 6" />
              </svg>
              <svg className="ft-icon" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2.5h8.5A1.5 1.5 0 0 1 21 9v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18Z" />
              </svg>
              <span className="ft-dir-n">{d.name}</span>
              {note && <span className="ft-note">{note(counted(d, more))}</span>}
            </button>
            {open && list(d)}
          </li>
        )
      })}
      {filesOf(dir).map((f) => (
        <li key={f.path}>
          {file(
            f,
            <span className="ft-name">
              <svg className="ft-icon" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="M14 3H7a1.5 1.5 0 0 0-1.5 1.5v15A1.5 1.5 0 0 0 7 21h10a1.5 1.5 0 0 0 1.5-1.5V7.5Z" />
                <path d="M14 3v4.5h4.5" />
              </svg>
              <span className="ft-file-n">{f.path.split('/').pop()}</span>
            </span>,
          )}
        </li>
      ))}
      {(more[dir.path] ?? 0) > 0 && <li className="ft-more">{more[dir.path]} more</li>}
    </ul>
  )

  return <div className="ft">{list(root)}</div>
}
