// A folder's page is the folder, plainly: what it's for; what the change open
// did to it; how it works, as the steps one thing takes, each by the part that
// does it, or, with no one path, its parts by what each does; what most parts
// use, in a line; and what it keeps true, each with the part that keeps it.
// Each part opens plainly. While your agent writes it, where it is on its list,
// and meanwhile its parts with their lines.

import type { ReactNode } from 'react'
import { plainWhere, useList } from '../agentList'
import type { FolderPlainly, Opened, PartView } from '../api/types'
import { Loader } from '../Loader'
import { useLoad } from '../useLoad'
import { WaitingLine } from './ListPane'
import { AlsoLine, GroupHead, KindLine, listed, PartRow, within, Woven } from './PlainRows'
import { useSeenPlainly } from './seen'

/** A part of a folder, to open: a folder's page, or a file, plainly. */
export type Part = { path: string; folder: boolean }

/** Steps, or parts, in words: "5 steps, in the order they happen". */
const howMany = (n: number, steps: boolean) =>
  steps ? `${n} ${n === 1 ? 'step' : 'steps'}, in the order they happen` : `${n} ${n === 1 ? 'part' : 'parts'}, by what each does`

/** A part's name, a link to it, in mono. */
function PartLink({ folder, part, isFolder, open }: { folder: string; part: string; isFolder?: boolean; open: (p: Part) => void }) {
  return (
    <button type="button" className="plink mono" onClick={() => open({ path: part, folder: !!isFolder })}>
      {within(folder, part, isFolder)}
    </button>
  )
}

export function FolderPage({ opened, path, agent, open, idea }: {
  opened: Opened
  /** From the root; "" for the root. */
  path: string
  /** Your agent, by name: Claude Code. */
  agent: string
  /** Opens a part: a folder's page, or a file, plainly. */
  open: (p: Part) => void
  /** Opens an idea's entry, beside the work. */
  idea: (key: string) => void
}) {
  const ofThis = (e: { type: string; opened?: number }) => !('opened' in e) || e.opened === opened.id
  const view = useLoad<FolderPlainly>(`/api/opened/${opened.id}/plainly/folder?path=${encodeURIComponent(path)}`, ['plainly', 'asks'], ofThis)
  const v = view.data
  // The top's page is the overview, the way in's first step.
  useSeenPlainly(path === '' ? '' : null, !!v?.what)
  // Meanwhile, its parts with their lines.
  const parts = useLoad<PartView>(v && !v.what ? `/api/opened/${opened.id}/part?path=${encodeURIComponent(path)}` : null, ['read', 'plainly'], ofThis)
  const list = useList()
  const name = path || opened.root.replace(/\/+$/, '').split('/').pop() || opened.root
  const Agent = agent.charAt(0).toUpperCase() + agent.slice(1)
  let body: ReactNode
  if (view.error) body = <p className="fv-say">{view.error}</p>
  else if (!v) body = <p className="fv-say"><Loader size="small" /></p>
  else if (!v.what) {
    body = (
      <>
        {parts.data?.line && <p className="lede">{parts.data.line}</p>}
        <div className="pl-waiting">
          <WaitingLine page={path} where={plainWhere(list, v.waiting, opened.id)} working={!!list?.working} agent={Agent} writing={!!v.waiting?.writing} />
        </div>
        {!!parts.data?.parts.length && (
          <section aria-label="Its parts">
            <GroupHead name="Its parts" count={parts.data.parts.length} />
            {parts.data.parts.map((p) => (
              <button key={p.path} type="button" className="prt" onClick={() => open(p)}>
                <b className="mono">{within(path, p.path, p.folder)}</b>
                <span>{p.line ?? ''}</span>
              </button>
            ))}
          </section>
        )}
      </>
    )
  } else {
    const marked = v.parts.some((p) => p.mark)
    // What it uses and what keeps it true needn't be among its steps: a
    // name with no extension is taken for a folder.
    const isFolder = (part: string) => v.parts.find((p) => p.part === part)?.folder ?? !/\.[^/.]+$/.test(part.split('/').pop() ?? '')
    body = (
      <div className={marked ? undefined : 'unmarked'}>
        <p className="lede"><Woven text={v.what} ideas={v.ideas} idea={idea} /></p>
        {v.stale && <p className="used">Being brought up to date.</p>}
        {v.change && <KindLine><Woven text={v.change} ideas={v.ideas} idea={idea} /></KindLine>}
        {v.parts.length > 0 && (
          <section aria-label="How it works">
            <GroupHead name="How it works" count={howMany(v.parts.length, v.shape === 'steps')} />
            {v.parts.map((p) => (
              <PartRow key={p.part} mark={p.mark} goal={p.goal} where={within(path, p.part, p.folder)} says={p.says} ideas={p.ideas} before={p.before} idea={idea}>
                <button type="button" className="quiet-link showc" onClick={() => open({ path: p.part, folder: p.folder })}>
                  Open it<span className="sr-only">: {within(path, p.part, p.folder)}</span>
                </button>
              </PartRow>
            ))}
          </section>
        )}
        {v.also && v.also.parts.length > 0 && (
          <AlsoLine name="Also here">
            {listed(v.also.parts.map((p) => <PartLink key={p} folder={path} part={p} isFolder={isFolder(p)} open={open} />))}: <Woven text={v.also.says} ideas={v.also.ideas} idea={idea} />
          </AlsoLine>
        )}
        {v.keeps.length > 0 && (
          <section aria-label="What it keeps true">
            <GroupHead name="What it keeps true" />
            <ul className="rules">
              {v.keeps.map((k) => (
                <li key={k.says}>
                  <Woven text={k.says} ideas={k.ideas} idea={idea} />{' '}
                  <span className="by">Kept by <PartLink folder={path} part={k.by} isFolder={isFolder(k.by)} open={open} /></span>
                </li>
              ))}
            </ul>
          </section>
        )}
      </div>
    )
  }
  return (
    <div className="fv">
      <div className="fv-scroll">
        <div className="pl">
          <h2 className="fnm2 mono">{name}</h2>
          {body}
        </div>
      </div>
    </div>
  )
}
