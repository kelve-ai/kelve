// A change opens on its own page, plainly: what it changes and why; why, as
// your agent said it; where it fits; its kind, before → now; how it works now,
// by purpose, each part marked, with what it did before and the files it's in;
// what tools made, in a line; what else it touches; and its files, in the order
// to read them. While your agent writes it, where it is on its list, and
// meanwhile its files.

import type { ReactNode } from 'react'
import { plainWhere, useList } from '../agentList'
import type { ChangePlainly, Opened, Origins } from '../api/types'
import { Loader } from '../Loader'
import { useLoad } from '../useLoad'
import { GaveLine, LearnedList, UsedLine, useLessons, usedOn } from './Lessons'
import { WaitingLine } from './ListPane'
import { AlsoLine, GroupHead, KindLine, listed, PartRow, Woven } from './PlainRows'
import type { SideFile } from './Side'
import { useSeenPlainly } from './seen'

const IMAGE = /\.(png|jpe?g|gif|webp|avif|bmp|ico)$/i
const LOCKFILE = /(\.lockb?|-lock\.(json|ya?ml)|(^|\/)go\.sum)$/i

const count = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`

/** What tools made, in words: "62 screenshots, 12 generated files and the lockfile". */
export function madeByTools(paths: string[]): string {
  const shots = paths.filter((p) => IMAGE.test(p)).length
  const locks = paths.filter((p) => !IMAGE.test(p) && LOCKFILE.test(p)).length
  const made = paths.length - shots - locks
  const said = [
    shots ? count(shots, 'screenshot', 'screenshots') : '',
    made ? count(made, 'generated file', 'generated files') : '',
    locks ? (locks === 1 ? 'the lockfile' : `${locks} lockfiles`) : '',
  ].filter(Boolean)
  return said.length > 1 ? `${said.slice(0, -1).join(', ')} and ${said[said.length - 1]}` : (said[0] ?? '')
}

/** How many lines a file added and took out: `+9 −3`. */
function Counts({ file }: { file?: SideFile }) {
  const added = file?.added ?? 0
  const removed = file?.removed ?? 0
  if (!added && !removed) return <em />
  const label = [added ? `${added} added` : '', removed ? `${removed} taken out` : ''].filter(Boolean).join(', ')
  return (
    <em role="img" aria-label={label}>
      {added > 0 && <span className="a">+{added}</span>}
      {added > 0 && removed > 0 && ' '}
      {removed > 0 && <span className="r">{'−'}{removed}</span>}
    </em>
  )
}

/** Its files, each a link, numbered, with how many lines it added and took out. */
function FileList({ name, paths, files, open }: { name: string; paths: string[]; files: SideFile[]; open: (path: string) => void }) {
  const by = new Map(files.map((f) => [f.path, f]))
  return (
    <section aria-label={name}>
      <GroupHead name={name} count={paths.length} />
      {paths.map((p, i) => (
        <button key={p} type="button" className="prt counted" onClick={() => open(p)}>
          <b className="mono"><span className="n">{i + 1}</span>{p}</b>
          <Counts file={by.get(p)} />
        </button>
      ))}
    </section>
  )
}

export function ChangePage({ opened, files, agent, open, idea, lesson, lessonOn }: {
  opened: Opened
  /** The change's files, with the lines each added and took out. */
  files: SideFile[]
  /** Your agent, by name: Claude Code. */
  agent: string
  /** Opens a file of the change, plainly. */
  open: (path: string) => void
  /** Opens an idea's entry, beside the work. */
  idea: (key: string) => void
  /** Opens a lesson, beside the work. */
  lesson: (id: string) => void
  /** The lesson open beside it, whose link stays lit. */
  lessonOn?: string | null
}) {
  const lessons = useLessons(opened.id).data
  const origins = useLoad<Origins>(`/api/opened/${opened.id}/origins`, ['idea']).data
  const view = useLoad<ChangePlainly>(`/api/opened/${opened.id}/plainly/change`, ['plainly', 'asks'], (e) => !('opened' in e) || e.opened === opened.id)
  const list = useList()
  const v = view.data
  // The change's own page is the overview, the way in's first step.
  useSeenPlainly('', !!v?.what)
  const Agent = agent.charAt(0).toUpperCase() + agent.slice(1)
  // The line the agent gave when it opened this: what it is, or what it changed.
  const said = opened.said?.trim() && <p className="used">{Agent} said: “{opened.said.trim()}”</p>
  const link = (p: string) => (
    <button key={p} type="button" className="plink mono" onClick={() => open(p)}>{p}</button>
  )
  let body: ReactNode
  if (view.error) body = <p className="fv-say">{view.error}</p>
  else if (!v) body = <p className="fv-say"><Loader size="small" /></p>
  else if (!v.what) {
    const tools = new Set(v.tools)
    body = (
      <>
        {said}
        <div className="pl-waiting">
          <WaitingLine page={`change:${opened.id}`} where={plainWhere(list, v.waiting, opened.id)} working={!!list?.working} agent={Agent} writing={!!v.waiting?.writing} />
        </div>
        {/* What the graph gave and what the agent learned don't wait for the reading: what it learned may leave first. */}
        <GaveLine lessons={lessons?.used ?? []} agent={agent} origins={origins} />
        <FileList name="Its files" paths={files.filter((f) => !tools.has(f.path)).map((f) => f.path)} files={files} open={open} />
        <LearnedList learned={lessons?.learned ?? []} agent={agent} open={lesson} />
      </>
    )
  } else {
    body = (
      <>
        <p className="lede"><Woven text={v.what} ideas={v.ideas} idea={idea} /></p>
        {v.stale && <p className="used">Being brought up to date.</p>}
        {said}
        {v.fits && <p className="used">Where it fits: <Woven text={v.fits} ideas={v.ideas} idea={idea} /></p>}
        {v.kind && <KindLine kind={v.kind}>{v.change && <Woven text={v.change} ideas={v.ideas} idea={idea} />}</KindLine>}
        <GaveLine lessons={lessons?.used ?? []} agent={agent} origins={origins} />
        {v.parts.length > 0 && (
          <section aria-label="How it works now">
            <GroupHead name="How it works now" count={`${count(v.parts.length, 'part', 'parts')}, by purpose`} />
            {v.parts.map((p) => (
              <PartRow key={`${p.goal}:${p.files.join()}`} mark={p.mark} goal={p.goal} says={p.says} ideas={p.ideas} before={p.before} idea={idea}>
                <UsedLine lessons={usedOn(lessons, p.files)} open={lesson} on={lessonOn} />
                {p.files.length > 0 && <p className="also">{p.mark === 'moved' ? 'Now in' : 'In'} {listed(p.files.map(link))}</p>}
              </PartRow>
            ))}
          </section>
        )}
        {v.tools.length > 0 && (
          <AlsoLine name="Made by tools" title={v.tools.join('\n')}>{madeByTools(v.tools)}: not read.</AlsoLine>
        )}
        {v.touches.length > 0 && (
          <section aria-label="What else it touches">
            <GroupHead name="What else it touches" />
            <ul className="rules">
              {v.touches.map((t) => <li key={t.says}><Woven text={t.says} ideas={t.ideas} idea={idea} /></li>)}
            </ul>
          </section>
        )}
        {v.order.length > 0 && <FileList name="Its files, in the order to read them" paths={v.order} files={files} open={open} />}
        <LearnedList learned={lessons?.learned ?? []} agent={agent} open={lesson} />
      </>
    )
  }
  return (
    <div className="fv">
      <div className="fv-scroll">
        <div className="pl">
          <h2 className="fnm2">{opened.title}</h2>
          {body}
        </div>
      </div>
    </div>
  )
}
