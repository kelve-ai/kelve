// What your agent opened in kelve (#/o/<id>). Work it handed over, a change or
// a worktree, opens as its debrief, or file by file while there's no debrief
// (screens/Change.tsx). Anything else opens here, plainly: one bar, the title
// and your agent's line; the side panel, with the files; and the file. Your
// agent's list opens beside the work. The address holds what it shows:
// #/o/<id>?f=<file>&l=<line>&w=all&k=list

import { Component, type ReactNode, useCallback, useEffect, useMemo, useState } from 'react'
import type { ChangeView, FolderView, ListItem, Opened, Settings } from '../api/types'
import { OpenContext, useAgentList } from '../agentList'
import { Loader, Loading } from '../Loader'
import { Top } from '../Top'
import { useHeld, useLoad } from '../useLoad'
import { agentKindName, useNarrow } from '../words'
import { post } from '../api/client'
import { fetchFile, fileAhead, FileView, fileIn } from './FileView'
import { type SideFile, Side } from './Side'
import { AgentLine, ListPane } from './ListPane'
import { Grip, usePaneWidths, useWindowWidth, WORK_LEAST } from './widths'

type At = { f: string | null; l?: number; w: 'some' | 'all'; k: 'list' | null }

/** The address, read. */
function read(): At {
  const q = new URLSearchParams(location.hash.split('?')[1] ?? '')
  const l = Number(q.get('l'))
  return {
    f: q.get('f'),
    l: l > 0 ? l : undefined,
    w: q.get('w') === 'all' ? 'all' : 'some',
    k: q.get('k') === 'list' ? 'list' : null,
  }
}

/** What the reader shows, kept in the address so it survives a reload, and Back goes back. */
function useAt(base: string): [At, (next: Partial<At>, replace?: boolean) => void] {
  const [at, setAt] = useState(read)
  useEffect(() => {
    const on = () => setAt(read())
    on()
    window.addEventListener('hashchange', on)
    return () => window.removeEventListener('hashchange', on)
  }, [])
  const go = useCallback(
    (next: Partial<At>, replace = false) => {
      const all = { ...read(), ...next }
      const q = new URLSearchParams()
      if (all.f) q.set('f', all.f)
      if (all.f && all.l) q.set('l', String(all.l))
      if (all.w === 'all') q.set('w', 'all')
      if (all.k) q.set('k', all.k)
      const hash = `${base}${q.size ? `?${q}` : ''}`
      if (replace) {
        history.replaceState(null, '', hash)
        setAt(read())
      } else location.hash = hash
    },
    [base],
  )
  return [at, go]
}

/** If the reader can't be drawn, it says so, rather than the page going blank. */
class Fallback extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false }
  static getDerivedStateFromError() {
    return { failed: true }
  }
  componentDidCatch(error: unknown) {
    console.warn('kelve could not draw the reader', error)
  }
  render() {
    if (this.state.failed) return <div className="reader"><Top /><Loading error="Something went wrong drawing this. Reload the page to try again." /></div>
    return this.props.children
  }
}

/** Where work handed over shows instead: its debrief, or the change file by file while it has none. */
export function handedOverAt(opened: Opened): string | null {
  if (opened.debrief != null) return `#/debrief/${opened.debrief}`
  if (opened.handover != null) return `#/handover/${opened.handover}`
  return null
}

export function Reader({ id, example }: {
  id: number
  /** The app's own example, read only, at #/example. */
  example?: boolean
}) {
  // Asks too: the files your agent is working through move as it works.
  const opened = useLoad<Opened>(`/api/opened/${id}`, ['opened', 'asks', 'debriefs'])
  const elsewhere = !example && opened.data ? handedOverAt(opened.data) : null
  useEffect(() => {
    if (elsewhere) location.replace(elsewhere)
  }, [elsewhere])
  if (!opened.data || elsewhere) return <div className="reader"><Top /><Loading error={opened.error} /></div>
  return (
    <Fallback>
      <Desk opened={opened.data} example={example} />
    </Fallback>
  )
}

/** The folder's name, from where it is: `~/projects/ledger` is ledger. */
const project = (root: string) => root.replace(/\/+$/, '').split('/').pop() || root

function Desk({ opened, example }: { opened: Opened; example?: boolean }) {
  const [at, go] = useAt(example ? '#/example' : `#/o/${opened.id}`)
  const narrow = useNarrow()
  const isChange = opened.kind === 'change'
  const change = useLoad<ChangeView>(isChange ? `/api/opened/${opened.id}/change` : null, ['opened'])
  const folder = useLoad<FolderView>(isChange ? null : `/api/opened/${opened.id}/folder`, ['opened'])
  const list = useLoad<FolderView>(isChange && at.w === 'all' ? `/api/opened/${opened.id}/folder?list=true` : null, ['opened'])
  const settings = useLoad<Settings>('/api/settings', ['settings'])
  const agentList = useAgentList()
  const [sideOpen, setSideOpen] = useState(false)
  const [widths, setWidth] = usePaneWidths()
  const windowWidth = useWindowWidth()

  // The files the side panel lists: the changed files, or every file.
  const changedFiles: SideFile[] = useMemo(
    () => (change.data?.files ?? []).map((f) => ({ path: f.path, skip: f.skip, gone: f.status === 'deleted', added: f.added, removed: f.removed })),
    [change.data],
  )
  const folderFiles: SideFile[] = useMemo(() => (folder.data?.files ?? []).map((f) => ({ path: f.path, skip: f.skip })), [folder.data])
  const everyFile: SideFile[] = useMemo(() => {
    const changed = new Map(changedFiles.map((f) => [f.path, f]))
    return (list.data?.files ?? []).map((f) => changed.get(f.path) ?? { path: f.path, skip: f.skip })
  }, [list.data, changedFiles])
  const sideFiles = isChange ? (at.w === 'all' ? everyFile : changedFiles) : folderFiles

  // The file shown: the one asked for, else the one opened, else a change's
  // first, else the folder's readme, else its first that can be read.
  const path = useMemo(() => {
    if (at.f) return at.f
    if (isChange) return changedFiles.find((f) => !f.skip && !f.gone)?.path ?? changedFiles[0]?.path ?? null
    if (opened.kind === 'file' && opened.path) return opened.path
    const files = folder.data?.files ?? []
    const readme = files.find((f) => /^readme(\.md|\.txt)?$/i.test(f.path))
    return (readme ?? files.find((f) => !f.skip))?.path ?? null
  }, [at.f, isChange, changedFiles, opened, folder.data])
  const changed = useMemo(() => new Set(changedFiles.map((f) => f.path)), [changedFiles])
  const inChange = isChange && !!path && changed.has(path)
  // A file likely opened next, one in the side you've rested on, is looked at, so it opens at once.
  const ahead = useCallback((f: string) => fileAhead(opened.id, f, changed.has(f)), [opened.id, changed])
  const agentName = agentKindName(settings.data?.agent)

  // Something on the agent's list: here, or in what else it's for.
  const openItem = useCallback(
    (item: ListItem) => {
      if (item.opened != null && item.opened !== opened.id && !example) {
        location.hash = `#/o/${item.opened}`
        return
      }
      go({ k: null })
    },
    [go, opened.id, example],
  )
  // What the tab shows is what's open: going to something opened before
  // opens it again.
  useEffect(() => {
    if (!example) void post(`/api/opened/${opened.id}/again`).catch(() => {})
  }, [opened.id, example])

  // Escape closes what's open beside the work; with nothing, it goes home.
  // Never when something else has it: a field, an editor, a key with others held.
  const home = example ? '#/example/home' : '#/'
  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return
      const t = e.target as HTMLElement
      if (t.closest('input, textarea, select, [contenteditable="true"], [role="dialog"], [role="menu"], [role="listbox"]')) return
      if (at.k) go({ k: null })
      else location.hash = home
    }
    window.addEventListener('keydown', on)
    return () => window.removeEventListener('keydown', on)
  }, [at.k, go, home])

  const title = opened.title
  const loaded = isChange ? change : folder
  const page = !!at.k
  // The list takes its width, and whether the side panel still fits beside the work.
  const showSide = narrow ? sideOpen : windowWidth >= widths.side + WORK_LEAST + (page ? widths.list : 0)
  // Going to another file, this one stays until it's in, for a moment, rather than blinking.
  const workId = loaded.error ? 'error' : path ? `f:${path}:${inChange}` : 'none'
  const work = loaded.error ? (
    <p className="fv-say">{loaded.error}</p>
  ) : path ? (
    <FileView key={`${path}:${inChange}`} opened={opened} path={path} inChange={inChange} line={at.l} />
  ) : (
    <p className="fv-say">{loaded.data ? 'There’s no file here to read.' : <Loader size="small" />}</p>
  )
  const held = useHeld(
    workId,
    work,
    path && !loaded.error ? fileIn(opened.id, path, inChange) : true,
    () => (path ? fetchFile(opened.id, path, inChange) : Promise.resolve()),
  )
  // What's open in this tab, for what to say to your agent: this, by its number.
  const forThis = useMemo(() => ({ id: opened.id, title }), [opened.id, title])
  // The list, beside the work.
  const pageView = at.k === 'list' ? (
    <section className="pane page" style={narrow ? undefined : { width: widths.list }} aria-label={`${agentName}’s list`}>
      {!narrow && <Grip width={widths.list} set={(w) => setWidth('list', w)} label="The width of the list" />}
      <div className="pane-in">
        <ListPane list={agentList.data} agent={agentName} onOpen={openItem} close={() => go({ k: null })} />
      </div>
    </section>
  ) : null
  return (
    <OpenContext.Provider value={forThis}>
      <div className={`reader${page ? ' with-panes' : ''}`}>
        <Top
          home={home}
          esc
          settings={example ? '#/example/settings/agent' : '#/settings'}
          right={<AgentLine list={agentList.data} agent={agentName} open={at.k === 'list'} onClick={() => go({ k: at.k === 'list' ? null : 'list' })} />}
        >
          {narrow && !page && (
            <button type="button" className="side-key" aria-expanded={sideOpen} onClick={() => setSideOpen((o) => !o)}>Files</button>
          )}
          {/* The project, unless the title already says it. */}
          {project(opened.root).toLowerCase() !== title.toLowerCase() && <span className="proj" title={opened.root}>{project(opened.root)}</span>}
          <nav className="path" aria-label="Where you are">
            <h1 className="title" title={title}><b>{title}</b></h1>
          </nav>
        </Top>
        <div className="frame">
          <main className="work" aria-label={title}>
            <div className="view">
              {showSide && (
                <Side
                  width={narrow ? undefined : widths.side}
                  grip={narrow ? undefined : <Grip right width={widths.side} set={(w) => setWidth('side', w)} label="The width of the side" />}
                  files={sideFiles}
                  loading={!loaded.data && !loaded.error}
                  which={at.w}
                  setWhich={(w) => go({ w }, true)}
                  change={isChange}
                  here={path ?? undefined}
                  look={ahead}
                  go={(f) => {
                    setSideOpen(false)
                    go({ f, l: undefined })
                  }}
                />
              )}
              {held}
            </div>
          </main>
          {pageView}
        </div>
      </div>
    </OpenContext.Provider>
  )
}
