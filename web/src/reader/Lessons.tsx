// Lessons (boards Hive-1 to Hive-5): what people's agents learned using the
// ideas in the work, from the shared graph. The reader as it is, with the least
// added: a line at the top of a change in the kind line's shape, saying what
// the graph gave; a small tag on a part, how many it used and how many held,
// opening to a short row for each (board Lessons-2); a lesson opened in the
// pane an entry opens in, written like an entry, its Do its heading (boards
// Better-1 to Better-4); the lessons about what's open, in the side; and what
// the agent learned, at the end, leaving later, with Stop. Wherever a lesson is
// listed, a small green square marks it, and it's listed by its Do; ideas keep
// their red.

import { useState } from 'react'
import type { LearnedView, LessonsView, LessonView, Origins } from '../api/types'
import { inExample, post } from '../api/client'
import { Loader } from '../Loader'
import { Rich } from '../rich'
import { useLoad } from '../useLoad'
import { say } from '../words'
import { GroupHead } from './PlainRows'
import { Square } from './Square'

/** The step that opens a lesson in the panes, beside entries. */
export const LESSON = 'lesson:'
export const isLesson = (key: string) => key.startsWith(LESSON)
/** What the agent learned, opened as a lesson is: `lesson:learned:<id>`. */
export const LEARNED = 'learned:'

/** A lesson's mark: a small green square, where an idea's is red. */
export const LessonSquare = () => <i className="lessq" aria-hidden="true" />

/** The lessons about what's open, and one file of it when the side's on one. */
export function useLessons(opened: number, path?: string | null) {
  const q = path ? `?path=${encodeURIComponent(path)}` : ''
  return useLoad<LessonsView>(`/api/opened/${opened}/lessons${q}`, ['lessons', 'read'])
}

const held = (l: LessonView) => l.used.some((u) => u.outcome === 'held')
const didnt = (l: LessonView) => l.used.some((u) => u.outcome === 'didnt')

/**
 * The use that says most about how a lesson went here: one that didn't
 * hold, then one that did, the strongest showing first, what kelve saw
 * before what the agent said, and a use as it worked before its answer to a
 * job; then one it said didn't apply. Undefined when nothing showed either way.
 */
function bestUse(l: LessonView): LessonView['used'][number] | undefined {
  const strength = (s: string) => (s === 'passed' || s === 'cameBack' ? 3 : s === 'stayed' || s === 'changedBack' ? 2 : 1)
  const best = (uses: LessonView['used']) =>
    [...uses].sort((a, b) => strength(b.shown) - strength(a.shown) || Number(a.moment === 'asked') - Number(b.moment === 'asked'))[0]
  return best(l.used.filter((x) => x.outcome === 'didnt')) ?? best(l.used.filter((x) => x.outcome === 'held')) ?? l.used.find((x) => x.shown === 'notHere')
}

/** What showed it, for its line on a part: the strongest the agent's uses showed; nothing when nothing did. */
function heldLine(l: LessonView): string {
  return bestUse(l)?.line ?? ''
}

/**
 * At the top of a change or a file: what the shared graph gave this work, in
 * the kind line's shape (board Value-1). On a change, first the ideas: how
 * many someone's agent had already written, and how few yours had to; then
 * the lessons.
 */
export function GaveLine({ lessons, agent, origins }: { lessons: LessonView[]; agent: string; origins?: Origins }) {
  const Agent = agent.charAt(0).toUpperCase() + agent.slice(1)
  const o = origins && origins.fromGraph > 0 ? origins : undefined
  const ideas = o
    ? `${o.fromGraph} of ${o.ideas} ${o.ideas === 1 ? 'idea was' : 'ideas were'} already written${o.byAgent > 0 ? `, so ${Agent} wrote ${o.byAgent}` : ''}.`
    : ''
  if (!lessons.length) {
    if (!ideas) return null
    return (
      <p className="chg gv">
        <span className="mkt new">From the graph</span>
        <span>{ideas}</span>
      </p>
    )
  }
  const n = lessons.length
  const h = lessons.filter((l) => held(l) && !didnt(l)).length
  const d = lessons.filter(didnt).length
  const used = n === 1 ? 'a lesson' : `${say(n)} lessons`
  const tested = h + d
  const cap = (w: string) => w.charAt(0).toUpperCase() + w.slice(1)
  // Which were put to the test, and how they did: the rest it only read as it worked.
  const how =
    tested === 0 ? '.'
    : tested === n && d === 0 ? (n === 1 ? ', and it held.' : n === 2 ? ', and both held.' : ', and all held.')
    : tested === n && h === 0 ? (n === 1 ? ', and it didn’t hold.' : ', and none held.')
    : d === 0 ? `; ${say(h)} ${h === 1 ? 'was' : 'were'} put to the test, and held.`
    : h === 0 ? `; ${say(d)} ${d === 1 ? 'was' : 'were'} put to the test, and didn’t hold.`
    : `; ${say(h)} held when put to the test, and ${say(d)} didn’t.`
  const line = ideas ? `${ideas} It was given ${used}${how}` : cap(`${Agent} was given ${used} from the shared graph${how}`)
  return (
    <p className="chg gv">
      <span className="mkt new">From the graph</span>
      <span>{line}</span>
    </p>
  )
}

/** How a lesson went here, for its row: what showed it held or didn't, or when it was given. */
function howLine(l: LessonView): string {
  const shown = heldLine(l)
  if (shown) return shown
  const m = l.used[0]?.moment
  // kelve knows it gave the lesson, not whether the agent acted on it: only
  // what came after can show that.
  const when = m === 'failed' ? 'as something failed' : m === 'asked' ? 'when it asked for lessons' : m === 'wrote' ? 'as it wrote the usual way it replaces' : 'as it worked on this file'
  return `given ${when}, and not put to the test`
}

/**
 * On a part (board Lessons-2): one small tag in the graph's green, how many
 * lessons and how many held; it opens to a short row for each, which opens
 * the lesson beside the work. Shut, the part reads as it would without.
 */
export function UsedLine({ lessons, open, on }: { lessons: LessonView[]; open: (id: string) => void; on?: string | null }) {
  const [shut, setShut] = useState(true)
  if (!lessons.length) return null
  const n = lessons.length
  const h = lessons.filter((l) => held(l) && !didnt(l)).length
  const d = lessons.filter(didnt).length
  const showing = !shut || lessons.some((l) => l.id === on)
  return (
    <div className="lsn">
      <button type="button" className="lc" aria-expanded={showing} onClick={() => setShut(showing)}>
        <i aria-hidden="true" />
        {n === 1 ? 'A lesson' : `${say(n).charAt(0).toUpperCase()}${say(n).slice(1)} lessons`} from the graph
        {h > 0 && <span className="x"> · {say(h)} held</span>}
        {d > 0 && <span className="x nh"> · {say(d)} didn’t hold</span>}
        <span className="v" aria-hidden="true">{showing ? '▴' : '▾'}</span>
      </button>
      {showing && (
        <ul className="lrows">
          {lessons.map((l) => (
            <li key={l.id}>
              <LessonSquare />
              <span>
                <button type="button" className={`plink${on === l.id ? ' on' : ''}`} onClick={() => open(l.id)}><Rich text={l.do} /></button>
                <span className={didnt(l) ? 'nh' : held(l) ? 'h' : 'q'}>{howLine(l)}</span>
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/** The lessons used on some files: those given on them, and those given as something failed in the work. */
export function usedOn(view: LessonsView | undefined, files: string[]): LessonView[] {
  if (!view) return []
  // What was put to the test first: it's what shows the graph's worth.
  const weight = (l: LessonView) => (didnt(l) ? 1 : held(l) ? 0 : 2)
  return view.used.filter((l) => l.used.some((u) => (u.path ? files.includes(u.path) : false))).sort((a, b) => weight(a) - weight(b))
}

/** How long what the agent learned waits before it leaves, in words: "a day", "30 seconds". */
export function holdWords(secs: number): string {
  const n = (amount: number, one: string, many: string) => (amount === 1 ? one : `${amount} ${many}`)
  if (secs >= 86_400 && secs % 86_400 === 0) return n(secs / 86_400, 'a day', 'days')
  if (secs >= 3600) return n(Math.round(secs / 3600), 'an hour', 'hours')
  if (secs >= 60) return n(Math.round(secs / 60), 'a minute', 'minutes')
  return n(secs, 'a second', 'seconds')
}

/** When something the agent learned leaves, as a person would say it. */
export function leaves(l: LearnedView, now = new Date()): string {
  const at = new Date(l.leavesAt)
  const days = Math.round((new Date(at.toDateString()).getTime() - new Date(now.toDateString()).getTime()) / 86_400_000)
  const mins = Math.ceil((at.getTime() - now.getTime()) / 60_000)
  if (at <= now) return 'Leaving now'
  // Soon, as when kelve's told to hold lessons a short while.
  if (mins <= 1) return 'Leaves in a minute'
  if (mins < 60) return `Leaves in ${mins} minutes`
  if (days <= 0) return 'Leaves today'
  if (days === 1) return 'Leaves tomorrow'
  return `Leaves on ${at.toLocaleDateString('en-GB', { weekday: 'long' })}`
}

/** Where something the agent learned is, in a few words. */
export function learnedState(l: LearnedView): string {
  switch (l.state) {
    case 'waiting': return l.why ?? leaves(l)
    case 'stopped': return 'Stopped: it stays here'
    case 'checking': return 'Being checked'
    case 'live': return 'In the shared graph, not confirmed yet'
    case 'held': return 'Held back until another project finds the same'
    case 'same': return 'Already in the graph; counted as confirming it'
    case 'turnedAway': return l.why ? `Turned away: ${l.why}` : 'Turned away'
  }
}

/** One thing the agent learned, by its Do, with Stop while it waits; opened beside the work when it can be. */
export function LearnedItem({ l, stopped, open }: { l: LearnedView; stopped?: () => void; open?: (id: string) => void }) {
  const [busy, setBusy] = useState(false)
  const stop = () => {
    if (inExample()) return
    setBusy(true)
    void post(`/api/learned/${l.id}/stop`).then(() => stopped?.()).finally(() => setBusy(false))
  }
  return (
    <li className="lli">
      <LessonSquare />
      {open ? <button type="button" className="plink lt" onClick={() => open(LEARNED + l.id)}><Rich text={l.do} /></button> : <span><Rich text={l.do} /></span>}
      <span className="lq">
        {learnedState(l)}
        {l.state === 'waiting' && !l.why && (
          <> · <button type="button" className="plink" disabled={busy} onClick={stop}>Stop</button></>
        )}
      </span>
    </li>
  )
}

/** At the end of a change: what the agent learned in it, for the shared graph once it's checked. */
export function LearnedList({ learned, agent, open }: { learned: LearnedView[]; agent: string; open?: (id: string) => void }) {
  if (!learned.length) return null
  const Agent = agent.charAt(0).toUpperCase() + agent.slice(1)
  return (
    <section aria-label={`What ${Agent} learned`}>
      <GroupHead name={`What ${Agent} learned`} count="For the shared graph, once it’s checked" />
      <ul className="rules">
        {learned.map((l) => <LearnedItem key={l.id} l={l} open={open} />)}
      </ul>
    </section>
  )
}

/** How far a lesson is confirmed, for a row: "held for 14 projects", "from one project · not confirmed yet". */
export function standing(l: LessonView): string {
  if (l.standing === 'confirmed') return l.basis
  if (l.standing === 'doubted') return 'doubted: it often didn’t hold'
  return l.record.held + l.record.didnt === 0 ? (l.backedBy ? 'a page backs it · not confirmed yet' : 'from one project · not confirmed yet') : l.basis
}

/** "Instead of `x`": the usual way a lesson replaces. Its Do is the heading, so it isn't said again; the example follows. */
function Instead({ instead, example }: { instead: string; example?: string | null }) {
  return (
    <p className="linst">
      Instead of <span className="mono">{instead}</span>{example ? ':' : '.'}
    </p>
  )
}

/** One line on when a lesson holds, what backs it and how it's held. */
function holdsLine(holdsWhen: string, rest: string): string {
  const h = holdsWhen.trim()
  return `Holds when: ${h}${/[.!?]$/.test(h) ? '' : '.'}${rest}`
}

/** The ideas a lesson is about, as an entry's rows: red squares, each opening its entry. */
function About({ about, open }: { about: { id: string; name: string }[]; open: (key: string) => void }) {
  if (!about.length) return null
  return (
    <div className="rg">
      <span className="lb">About</span>
      <div className="vals">
        {about.map((a) => (
          <button key={a.id} type="button" className="rel" onClick={() => open(a.id)}>
            <Square ideaKey={a.id} name={a.name} />
            <span><b>{a.name}</b></span>
          </button>
        ))}
      </div>
    </div>
  )
}

/** A lesson, opened beside the work, in the pane an entry opens in, written like one (Better-1, Better-3): its Do the heading, then what it says, what it replaces, its example; then rows. */
export function LessonPane({ id, open }: { id: string; open: (key: string) => void }) {
  if (id.startsWith(LEARNED)) return <LearnedPane id={Number(id.slice(LEARNED.length))} open={open} />
  return <GraphLessonPane id={id} open={open} />
}

function GraphLessonPane({ id, open }: { id: string; open: (key: string) => void }) {
  const page = useLoad<LessonView>(`/api/lessons/${encodeURIComponent(id)}`, ['lessons'])
  const [told, setTold] = useState(false)
  if (!page.data) return <article className="ep">{page.error ? <p className="ep-say">{page.error}</p> : <Loader size="small" label="Loading the lesson" />}</article>
  const l = page.data
  const use = bestUse(l) ?? l.used.find((x) => x.moment !== 'asked') ?? l.used[0]
  const backed = l.backedBy ? ' A page backs it.' : ''
  const how = l.standing === 'confirmed' ? ` ${l.basis.charAt(0).toUpperCase()}${l.basis.slice(1)} for other agents.` : ` ${l.basis.charAt(0).toUpperCase()}${l.basis.slice(1)}.`
  const another = l.another ?? []
  return (
    <article className="ep lpane" aria-label="Lesson">
      <h2><LessonSquare /><span><Rich text={l.do} /></span></h2>
      <p className="lsay"><Rich text={l.says} /></p>
      {l.instead && <Instead instead={l.instead} example={l.example} />}
      {l.example && <pre className="lex">{l.example}</pre>}
      <p className="lnote">{holdsLine(l.holdsWhen, `${backed}${how}`)}</p>
      <section className="rels" aria-label="What it's about">
        {use && (
          <div className="rg">
            <span className="lb">Here</span>
            <p className="lv">
              {use.moment === 'failed' ? 'Given as something failed' : use.moment === 'asked' ? 'Given when it was asked for' : use.moment === 'wrote' ? 'Given as it wrote the usual way in' : 'Given as it worked on'}
              {use.path && <>{use.moment === 'failed' ? ', fixed in ' : ' '}<span className="mono">{use.path}</span></>}
              {'. '}
              {(use.outcome !== 'unknown' || use.shown === 'notHere') && `It ${use.line.charAt(0).toLowerCase()}${use.line.slice(1)}.`}
            </p>
          </div>
        )}
        {another.length > 0 && (
          <div className="rg">
            <span className="lb">Another way</span>
            <div className="vals">
              {another.map((a) => (
                <button key={a.id} type="button" className="rel" onClick={() => open(LESSON + a.id)}>
                  <LessonSquare />
                  <span><b><Rich text={a.do} /></b><span className="w">Holds when {a.holdsWhen.replace(/\.$/, '').replace(/^./, (c) => c.toLowerCase())} · {a.basis}</span></span>
                </button>
              ))}
            </div>
          </div>
        )}
        <About about={l.about} open={open} />
      </section>
      {l.backedBy && (
        <p className="lsrc"><a href={l.backedBy.url} target="_blank" rel="noreferrer">{new URL(l.backedBy.url).hostname}</a>: “{l.backedBy.quote}”</p>
      )}
      <p className="lsrc">
        {told ? 'Thanks: it counts as not holding, with nothing from your work.' : (
          <button
            type="button"
            className="plink"
            onClick={() => {
              setTold(true)
              if (!inExample()) void post(`/api/lessons/${encodeURIComponent(l.id)}/didnt`).catch(() => {})
            }}
          >
            Didn’t hold for you?
          </button>
        )}
      </p>
    </article>
  )
}

/** What the agent learned, opened as it will go, written like an entry (Better-2): under Here, the first way and the second, which never leave; and Stop until it leaves. */
function LearnedPane({ id, open }: { id: number; open: (key: string) => void }) {
  const page = useLoad<LearnedView>(`/api/learned/${id}`, ['lessons'])
  const [busy, setBusy] = useState(false)
  if (!page.data) return <article className="ep">{page.error ? <p className="ep-say">{page.error}</p> : <Loader size="small" label="Loading the lesson" />}</article>
  const l = page.data
  const stop = () => {
    if (inExample()) return
    setBusy(true)
    void post(`/api/learned/${l.id}/stop`).then(() => page.reload()).finally(() => setBusy(false))
  }
  const lines = (text: string | null | undefined, sign: string) => (text ?? '').split('\n').filter((x) => x.trim()).map((x) => `${sign} ${x}`)
  return (
    <article className="ep lpane" aria-label="Lesson">
      <h2><LessonSquare /><span><Rich text={l.do} /></span></h2>
      <p className="lsay"><Rich text={l.says} /></p>
      {l.instead && <Instead instead={l.instead} example={l.example} />}
      {l.example && <pre className="lex">{l.example}</pre>}
      <p className="lnote">{holdsLine(l.holdsWhen, '')}</p>
      <section className="rels" aria-label="What it's about">
        {l.first && (
          <div className="rg">
            <span className="lb">Here</span>
            <div className="lv">
              <p>Your agent replaced what it first wrote{l.path ? <> in <span className="mono">{l.path}</span></> : null}, and kelve asked it to write this.</p>
              <pre className="ldiff">
                {lines(l.first, '−').map((x, i) => <span key={`a${i}`} className="del">{x}{'\n'}</span>)}
                {lines(l.second, '+').map((x, i) => <span key={`b${i}`} className="add">{x}{'\n'}</span>)}
              </pre>
            </div>
          </div>
        )}
        <div className="rg">
          <span className="lb">Leaves</span>
          <p className="lv">
            {learnedState(l)}
            {l.state === 'waiting' && !l.why && (
              <>, for the graph to check. Only the lesson goes, never this work. <button type="button" className="plink" disabled={busy} onClick={stop}>Stop</button></>
            )}
          </p>
        </div>
        <About about={l.about} open={open} />
      </section>
    </article>
  )
}

/** Under an entry: what people's agents learned using it, a few, then all of them. */
export function Learned({ ideaKey, name, lessons, all, open, idea }: { ideaKey: string; name: string; lessons: LessonView[]; all: number; open: (id: string) => void; idea?: (key: string) => void }) {
  const [every, setEvery] = useState(false)
  const more = useLoad<LessonView[]>(every ? `/api/lessons/all?idea=${encodeURIComponent(ideaKey)}` : null, ['lessons'])
  if (!lessons.length) return null
  const list = every && more.data ? more.data : lessons
  return (
    <section className="rels" aria-label="Learned">
      <div className="rg">
        <span className="lb">Learned</span>
        <div className="vals">
          {list.map((l) => {
            // The other ideas it's about, which it leads to.
            const also = l.about.filter((a) => a.id !== ideaKey)
            return (
              <div key={l.id} className="lrel">
                <button type="button" className="rel" onClick={() => open(l.id)}>
                  <LessonSquare />
                  <span><b className="lw"><Rich text={l.do} /></b><span className="w">{standing(l)}</span></span>
                </button>
                {also.length > 0 && idea && (
                  <p className="lalso">
                    Also about{' '}
                    {also.map((a, i) => (
                      <span key={a.id}>{i > 0 && ', '}<button type="button" className="plink" onClick={() => idea(a.id)}>{a.name}</button></span>
                    ))}
                  </p>
                )}
              </div>
            )
          })}
          {all > list.length && (
            <button type="button" className="plink lall" onClick={() => setEvery(true)}>
              {every && !more.data ? <Loader size="small" /> : `All ${say(all)} lessons about ${name}`}
            </button>
          )}
        </div>
      </div>
    </section>
  )
}

