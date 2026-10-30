// Entries, beside the work (boards M-2 to M-4, X-4 and X-5). Clicking an idea
// opens its entry as a pane at the right; the work narrows beside it and never
// goes away. An entry ends with how the idea relates; clicking one opens its
// entry beside, and the one before narrows and dims. At most two show; the path
// has the rest.

import { useEffect, useState } from 'react'
import { ideaKey, useList, whereIs } from '../agentList'
import { changed, inExample, post } from '../api/client'
import { Agreeing } from '../screens/SignIn'
import type { AskIn, AskKind, Checking, FlagIn, FlagRelation, GraphAccount, GraphSignIn, IdeaPage, Note, Origins, Outline, OutlineIdea, RelationKind, Settings, Source } from '../api/types'
import { Entry } from '../Entry'
import { Loader } from '../Loader'
import { isLesson, Learned, LEARNED, LESSON, LessonPane } from './Lessons'
import { Button, Quiet } from '../ui/ui'
import { useLoad } from '../useLoad'
import { agentName } from '../words'
import { firstWords, useNotes } from './NoteParts'
import { Square, useSeen } from './Square'
import { WaitingLine } from './ListPane'
import { Grip, type PaneKind } from './widths'

const lower = (name: string) => name.charAt(0).toLowerCase() + name.slice(1)

/** How many of an entry's claims are checked, a small square each. */
function Pips({ checking }: { checking: Checking }) {
  if (!checking.claims.length) return null
  return (
    <span className="pips" aria-label={`${checking.claims.filter((c) => c.state !== 'waiting').length} of ${checking.claims.length} claims checked`}>
      {checking.claims.map((c) => <i key={c.n} className={c.state === 'held' ? 'held' : c.state === 'failed' ? 'failed' : undefined} />)}
    </span>
  )
}

/**
 * Where an entry came from, under its first line (board Value-2): from the
 * shared graph, checked against its sources and how widely it's read; or
 * written by your agent just now, while the graph checks it, a square for
 * each claim. Dotted claims in it aren't checked yet.
 */
function FromLine({ page, agent }: { page: IdeaPage; agent: string }) {
  const e = page.entry
  if (!e) return null
  if (page.fromGraph) {
    const n = e.sources.length
    const said = [
      'From the shared graph',
      n > 0 && `checked against ${n} ${n === 1 ? 'source' : 'sources'}`,
      page.reads === 1 ? 'read once' : page.reads > 1 && `read ${page.reads.toLocaleString('en-GB')} times`,
    ].filter(Boolean)
    return <p className="ep-from"><i aria-hidden="true" />{said.join(' · ')}</p>
  }
  if (page.checking && page.checking.status === 'pending') {
    return (
      <p className="ep-from mine" role="status">
        <span>Written by {agent} just now · being checked</span>
        <Pips checking={page.checking} />
      </p>
    )
  }
  return null
}

/** How the shared graph's check of an entry your agent wrote stands (X-5): turned away, or claims that failed; while it's checked, `FromLine` says so. */
function CheckLine({ checking, agent }: { checking: Checking; agent: string }) {
  const failed = checking.claims.filter((c) => c.state === 'failed' && !c.words)
  const rejected = checking.status === 'rejected'
  return (
    <>
      {rejected && (
        <div className="check turned" role="status">
          <p>The shared graph turned this entry away. Only you can read it. {agent} will be asked to fix what failed; if it can’t, those sentences go.</p>
          <Pips checking={checking} />
        </div>
      )}
      {failed.map((c) => (
        <div key={c.n} className="flagged">
          <p className="bad">{c.text}</p>
          <p>
            {c.found ? `This doesn’t match its source. ${c.found.replace(/\.?$/, '.')}` : 'This doesn’t match its source.'}{' '}
            {agent} will be asked to fix it; if it can’t, the sentence goes.
          </p>
        </div>
      ))}
    </>
  )
}

/** What a report can be about: the entry, or one way the idea relates. */
function reportable(o?: Outline): Array<{ label: string; relation: FlagRelation }> {
  if (!o || o.proposed) return []
  return [
    ...(o.rows[0]?.items ?? []).map((i) => ({ label: `That it rests on ${lower(i.name)}`, relation: { kind: 'restsOn' as const, to: i.key } })),
    ...o.kindOf.map((i) => ({ label: `That it’s a kind of ${lower(i.name)}`, relation: { kind: 'kindOf' as const, to: i.key } })),
    ...o.answers.map((i) => ({ label: `That it answers ${lower(i.name)}`, relation: { kind: 'answers' as const, to: i.key } })),
    ...o.alternatives.map((i) => ({ label: `That ${lower(i.name)} does the same job`, relation: { kind: 'alternative' as const, to: i.key } })),
  ]
}

/**
 * Two quiet links at the end of an entry: Something wrong?, a short report
 * for the shared graph; and Didn't help me understand, which asks for what
 * would.
 */
function Report({ ideaKey, outline }: { ideaKey: string; outline?: Outline }) {
  const [open, setOpen] = useState<'flag' | 'ask'>()
  const [sent, setSent] = useState<string>()
  if (sent) return <p className="ep-flag" role="status">{sent}</p>
  if (open === 'flag') return <Flag ideaKey={ideaKey} outline={outline} close={() => setOpen(undefined)} sent={setSent} />
  if (open === 'ask') return <Ask ideaKey={ideaKey} close={() => setOpen(undefined)} sent={setSent} />
  return (
    <p className="ep-flag links">
      <Quiet onClick={() => setOpen('flag')}>Something wrong?</Quiet>
      <Quiet onClick={() => setOpen('ask')}>Didn’t help me understand</Quiet>
    </p>
  )
}

/** What a reader can say an entry needs. */
const NEEDS: Array<[AskKind, string]> = [
  ['example', 'Needs an example'],
  ['tooHard', 'Too hard'],
  ['missing', 'Missing something'],
]

/** Didn't help me understand: what would, sent with nothing but the entry it's about. */
function Ask({ ideaKey, close, sent }: { ideaKey: string; close: () => void; sent: (said: string) => void }) {
  const [kind, setKind] = useState<AskKind>()
  const [said, setSaid] = useState<string>()
  const send = (ev: React.FormEvent) => {
    ev.preventDefault()
    if (!kind) return
    setSaid(undefined)
    post(`/api/ideas/${encodeURIComponent(ideaKey)}/ask`, { kind } satisfies AskIn).then(
      () => sent('Sent. The next agent that adds to the shared graph and opens this entry is asked for it.'),
      (err: Error) => setSaid(err.message),
    )
  }
  return (
    <form className="ep-flag form" onSubmit={send}>
      <fieldset>
        <legend>What would help?</legend>
        {NEEDS.map(([k, label]) => (
          <label key={k}><input type="radio" name={`need-${ideaKey}`} checked={kind === k} onChange={() => setKind(k)} /> {label}</label>
        ))}
      </fieldset>
      <p className="row">
        <button type="submit" className="raised small" disabled={!kind}>Send it</button>
        <Quiet onClick={close}>Never mind</Quiet>
      </p>
      <p className="said">Only your choice goes to the shared graph, with the entry it’s about.</p>
      {said && <p className="warn">{said}</p>}
    </form>
  )
}

/** Something wrong? A short report for the shared graph. */
function Flag({ ideaKey, outline, close, sent }: { ideaKey: string; outline?: Outline; close: () => void; sent: (said: string) => void }) {
  const [why, setWhy] = useState('')
  const about = reportable(outline)
  const [which, setWhich] = useState(-1)
  const [said, setSaid] = useState<string>()
  const send = (ev: React.FormEvent) => {
    ev.preventDefault()
    setSaid(undefined)
    const relation = which >= 0 ? about[which]?.relation : undefined
    post(`/api/ideas/${encodeURIComponent(ideaKey)}/flag`, { why, relation } satisfies FlagIn).then(
      () => sent('Sent. The shared graph checks it again.'),
      (err: Error) => setSaid(err.message),
    )
  }
  return (
    <form className="ep-flag form" onSubmit={send}>
      {about.length > 0 && (
        <fieldset>
          <legend>What’s wrong?</legend>
          <label><input type="radio" name={`about-${ideaKey}`} checked={which < 0} onChange={() => setWhich(-1)} /> The entry</label>
          {about.map((a, i) => (
            <label key={`${a.relation.kind}:${a.relation.to}`}><input type="radio" name={`about-${ideaKey}`} checked={which === i} onChange={() => setWhich(i)} /> {a.label}</label>
          ))}
        </fieldset>
      )}
      <label htmlFor={`flag-${ideaKey}`}>
        {which >= 0 ? 'Why doesn’t it help you understand?' : 'What’s wrong with it?'} Say what’s wrong with the idea, not your own work: this goes to the shared graph.
      </label>
      <textarea id={`flag-${ideaKey}`} rows={3} value={why} onChange={(ev) => setWhy(ev.target.value)} />
      <p className="row">
        <button type="submit" className="raised small" disabled={!why.trim()}>Send it</button>
        <Quiet onClick={close}>Never mind</Quiet>
      </p>
      {said && <p className="warn">{said}</p>}
    </form>
  )
}

/** One way an idea relates, as a row: its square, its name, and why. */
function Rel({ idea, open, on }: { idea: OutlineIdea; open: (key: string) => void; on: boolean }) {
  return (
    <button type="button" className={`rel${on ? ' on' : ''}`} onClick={() => open(idea.key)}>
      <Square ideaKey={idea.key} fp={idea.foundation} name={idea.name} />
      <span>
        <b>{idea.name}</b>
        {idea.why && <span className="w">{idea.why}</span>}
      </span>
    </button>
  )
}

/** How the idea relates (X-4): what it rests on first, then the rest. */
function Relations({ outline, open, next }: { outline: Outline; open: (key: string) => void; next?: string }) {
  const groups: Array<[string, OutlineIdea[], RelationKind]> = [
    ['Rests on', outline.rows[0]?.items ?? [], 'restsOn'],
    ['Kind of', outline.kindOf, 'kindOf'],
    ['Answers', outline.answers, 'answers'],
    ['Used with', outline.usedWith, 'usedWith'],
    ['Instead', outline.alternatives, 'alternative'],
  ]
  // A way down followed is counted, once, with nothing about who.
  const follow = (kind: RelationKind, to: string) => {
    if (!inExample()) void post('/api/follows', { from: outline.key, kind, to }).catch(() => {})
    open(to)
  }
  const shown = groups.filter(([, items]) => items.length)
  const ground = outline.foundation && !(outline.rows[0]?.items.length)
  if (!shown.length && !ground) return null
  return (
    <section className="rels" aria-label="How it relates">
      {ground && (
        <>
          <span className="lb main">Rests on</span>
          <p className="first">Nothing: this is a first principle, the ground everything above it stands on.</p>
        </>
      )}
      {shown.map(([label, items, kind], i) => (
        <div key={label} className="rg">
          <span className={`lb${i === 0 && !ground && label === 'Rests on' ? ' main' : ''}`}>{label}</span>
          <div className="vals">{items.map((idea) => <Rel key={idea.key} idea={idea} open={(to) => follow(kind, to)} on={idea.key === next} />)}</div>
        </div>
      ))}
    </section>
  )
}

function Sources({ sources }: { sources: Source[] }) {
  if (!sources.length) return null
  const site = (url: string) => {
    try {
      return new URL(url).hostname.replace(/^www\./, '')
    } catch {
      return ''
    }
  }
  return (
    <ol className="sources" aria-label="Sources">
      {sources.map((s) => (
        <li key={s.n}>
          <b>{s.n}</b>
          <span>
            {s.url ? <a href={s.url} target="_blank" rel="noreferrer noopener">{s.title}</a> : s.title}
            <small>{[s.publisher, s.url && site(s.url)].filter(Boolean).filter((x, i, all) => all.indexOf(x) === i).join(', ')}</small>
          </span>
        </li>
      ))}
    </ol>
  )
}

/**
 * Your notes on the idea (N-4), from every project, and a way to add one.
 * They're yours: never sent to the shared graph.
 */
function IdeaNotes({ ideaKey, name }: { ideaKey: string; name: string }) {
  const n = useNotes()
  const mine = useLoad<Note[]>(`/api/ideas/${encodeURIComponent(ideaKey)}/notes`, ['notes'])
  if (!n) return null
  const notes = mine.data ?? []
  return (
    <section className="ep-notes" aria-label="Your notes on it">
      {notes.length > 0 && (
        <>
          <span className="lb">Just you</span>
          {notes.map((note) => (
            <div key={note.file} className="nwell still">
              {note.text.split(/\n\s*\n/).map((p) => firstWords(p)).filter(Boolean).map((p, i) => <p key={i}>{p}</p>)}
            </div>
          ))}
          <p className="np-where">Shown wherever {name} turns up, in any project.</p>
        </>
      )}
      <Quiet onClick={() => n.page(null, [ideaKey])}>Add a note</Quiet>
    </section>
  )
}

/**
 * Under an entry your agent wrote that the shared graph didn't have (O-4):
 * the first time an account is mentioned. Why to add it (the next person
 * reads it at once, and everyone's agent writes less), and what goes:
 * knowledge only, never the person's work.
 */
function AddAsk({ name, agent }: { name: string; agent: string }) {
  const n = useNotes()
  const account = useLoad<GraphAccount>('/api/graph/account', ['settings'])
  const settings = useLoad<Settings>('/api/settings', ['settings'])
  const origins = useLoad<Origins>(n ? `/api/opened/${n.opened}/origins` : null, ['idea'])
  const [said, setSaid] = useState<string>()
  const a = account.data
  if (inExample() || !a?.graph || a.from === 'env' || (a.from === 'app' && a.agreed) || !settings.data?.askToAdd) return null
  const o = origins.data
  const Agent = agent.charAt(0).toUpperCase() + agent.slice(1)
  const adding = `#/adding?idea=${encodeURIComponent(name)}`
  const add = (provider = 'github') => {
    if (a.from === 'app') {
      location.hash = adding
      return
    }
    post<GraphSignIn>('/api/graph/signin', { provider }).then(
      (x) => {
        window.open(x.address, '_blank', 'noopener')
        location.hash = adding
      },
      (e: Error) => setSaid(e.message),
    )
  }
  return (
    <section className="addq" aria-label={`Add ${name} to the shared graph`}>
      <h3>Add {name} to the shared graph.</h3>
      <p>
        {o && o.ideas > 1 && o.fromGraph > 0 && `${o.fromGraph} of the ${o.ideas} ideas here opened at once, because people who met them before you added them. `}
        {Agent} wrote {name} just for you. Add it, and the next person who meets {name} reads it straight away, instead of waiting for their agent to write it. Every entry added means less waiting, and less for your agent to write, for everyone, you included.
        {a.from === null && ' Signing in is also how you read the shared graph, so most ideas open at once.'}
      </p>
      <p className="what">
        Only knowledge is added: this entry, its sources, and how {name} relates to other ideas, and, as {agent} works, the lessons it learns. Never your code, file names or notes, or which ideas your work uses. The shared graph checks each claim against its sources before anyone else reads it.
      </p>
      <p className="row">
        {a.from === 'app' ? <Button onClick={() => add()}>Add {name}</Button> : (
          <>
            <Button onClick={() => add('github')}>Sign in with GitHub and add</Button>
            <Button onClick={() => add('google')}>Sign in with Google and add</Button>
          </>
        )}
        <Quiet onClick={() => void post('/api/settings', { keepToMyself: true }).then(() => changed('settings'))}>Keep it to myself</Quiet>
      </p>
      {a.from !== 'app' && <Agreeing />}
      {said && <p className="warn">{said}</p>}
    </section>
  )
}

/** No entry yet: on your agent's list (boards Q-7 to Q-9). */
function NoEntry({ page, agent }: { page: IdeaPage; agent: string }) {
  const Agent = agent.charAt(0).toUpperCase() + agent.slice(1)
  const list = useList()
  const [again, setAgain] = useState(false)
  // Taken off the list: it waits until you keep it for your agent again.
  if (page.stage === 'off') {
    return (
      <div className="noentry">
        {page.definition && <p className="def">{page.definition}</p>}
        <p>You took this off {Agent}’s list, so it isn’t written.</p>
        <button
          type="button"
          className="raised small"
          disabled={again}
          onClick={() => {
            setAgain(true)
            post(`/api/ideas/${encodeURIComponent(page.key)}/write`).catch(() => setAgain(false))
          }}
        >
          Keep it for {Agent} to write
        </button>
      </div>
    )
  }
  // While it's written, its steps, from the step it's on.
  const stage = page.stage
  const writing = stage === 'writing' || stage === 'checking' || stage === 'fixing'
  const steps = writing
    ? [
        { said: 'Written', state: stage === 'writing' ? 'now' : 'fin' },
        { said: 'Its claims checked', state: stage === 'writing' ? 'wait' : 'now' },
        { said: 'Shown here', state: 'wait' },
      ] as const
    : undefined
  return (
    <div className="noentry">
      {page.definition && <p className="def">{page.definition}</p>}
      <WaitingLine page={page.key} where={whereIs(list, ideaKey(page.key))} working={!!list?.working} agent={Agent} writing={writing} steps={steps?.map((s) => ({ ...s }))} />
    </div>
  )
}

export function EntryPane({ ideaKey, open, named, next, followed, opened }: {
  ideaKey: string
  /** What's open, where asking for an entry not written yet is kept for Ready for you. */
  opened?: number
  open: (key: string) => void
  named: (key: string, name: string) => void
  /** The idea open beside this one, whose row stays lit. */
  next?: string
  /** Reached by following a relation, for the count of which are followed. */
  followed?: boolean
}) {
  const seen = useSeen()
  const page = useLoad<IdeaPage>(`/api/ideas/${encodeURIComponent(ideaKey)}${followed ? '?followed=true' : ''}`, ['idea', 'asks'], (e) => e.type !== 'idea' || e.key === ideaKey)
  const outline = useLoad<Outline>(`/api/ideas/${encodeURIComponent(ideaKey)}/outline`, ['idea', 'asks'])
  const [again, setAgain] = useState(false)
  const name = page.data?.name
  useEffect(() => {
    if (name) named(ideaKey, name)
  }, [ideaKey, name, named])
  // Opened is read: its square fills everywhere.
  const loaded = !!page.data
  useEffect(() => {
    if (loaded) seen.read(ideaKey)
  }, [loaded, ideaKey, seen])
  // Opened with no entry yet: asked for by name, so Ready for you says when it's written.
  const unwritten = !!page.data && !page.data.entry && page.data.split.length === 0
  useEffect(() => {
    if (unwritten && opened != null) void post(`/api/opened/${opened}/asked`, { key: ideaKey }).catch(() => {})
  }, [unwritten, opened, ideaKey])

  if (!page.data) {
    return <article className="ep">{page.error ? <p className="ep-say">{page.error}</p> : <Loader size="small" label="Loading its entry" />}</article>
  }
  const p = page.data
  const e = p.entry
  const agent = agentName(p.agent)
  const o = outline.data
  if (p.split.length > 0) {
    return (
      <article className="ep" aria-label={p.name}>
        <h2><Square ideaKey={ideaKey} size="big" />{p.name}</h2>
        <p className="q">This turned out to be two ideas. Which do you mean?</p>
        <div className="vals split">
          {p.split.map((i) => (
            <button key={i.id} type="button" className="rel" onClick={() => open(i.id)}>
              <Square ideaKey={i.id} name={i.name} />
              <span><b>{i.name}</b></span>
            </button>
          ))}
        </div>
        <p className="ep-say">Where your files use it, kelve marks them again with the right one when you next open them.</p>
      </article>
    )
  }
  const checking = p.checking && p.checking.status !== 'accepted' ? p.checking : null
  return (
    <article className="ep" aria-label={p.name}>
      <h2><Square ideaKey={ideaKey} size="big" fp={o?.foundation} />{p.name}</h2>
      {p.question && <p className="q">{p.question}</p>}
      <FromLine page={p} agent={agent.charAt(0).toUpperCase() + agent.slice(1)} />
      {checking && <CheckLine checking={checking} agent={agent.charAt(0).toUpperCase() + agent.slice(1)} />}
      {e && p.problems.length > 0 && (
        <p className="ep-say">
          {p.problems.length === 1 ? 'The shared graph’s checks found a problem with this entry since it was written: ' : 'The shared graph’s checks found problems with this entry since it was written: '}
          {p.problems.join(' ')}
          {p.rewriting && ` It’s kept for ${agent} to write again; this one shows until the new one is checked.`}
        </p>
      )}
      {e && <Entry entry={e} onIdea={(idea) => open(idea.id)} claims={checking?.claims} fixer={agent} />}
      {e && p.keptHere && <AddAsk name={p.name} agent={agent} />}
      {e && p.stage === 'checking' && !p.checking && p.step && <p className="ep-say">{p.step}</p>}
      {!e && p.state === 'writing' && <NoEntry page={p} agent={agent} />}
      {p.state === 'waiting' && <p className="ep-say">{p.step}</p>}
      {p.state === 'dropped' && (
        <div className="noentry">
          <p>The shared graph couldn’t stand behind the entry {agent} wrote. {p.step}</p>
          <button
            type="button"
            className="raised small"
            disabled={again}
            onClick={() => {
              setAgain(true)
              void post(`/api/ideas/${encodeURIComponent(ideaKey)}/write`).finally(() => page.reload())
            }}
          >
            Keep it for {agent} to write again
          </button>
        </div>
      )}
      <IdeaNotes ideaKey={ideaKey} name={p.name} />
      {o && <Relations outline={o} open={open} next={next} />}
      <Learned ideaKey={ideaKey} name={p.name} lessons={p.lessons ?? []} all={p.lessonsAll ?? 0} open={(id) => open(LESSON + id)} idea={(key) => open(key)} />
      {e && <Sources sources={e.sources} />}
      {e && p.canFlag && <Report ideaKey={ideaKey} outline={o} />}
    </article>
  )
}

/** Where a pane's entry, or lesson, is read: what it shows first. */
export function paneAddress(step: { key: string; followed: boolean }) {
  if (!isLesson(step.key)) return `/api/ideas/${encodeURIComponent(step.key)}${step.followed ? '?followed=true' : ''}`
  const id = step.key.slice(LESSON.length)
  return id.startsWith(LEARNED) ? `/api/learned/${Number(id.slice(LEARNED.length))}` : `/api/lessons/${encodeURIComponent(id)}`
}

/** The panes: the last two entries of the path, the one before narrower and dimmed; one, beside a note. */
export function Panes({ path, open, close, named, widths, setWidth, beside, opened }: {
  path: Array<{ key: string; followed: boolean }>
  /** Opens an idea after the pane at `from`, closing those after it. */
  open: (key: string, from: number) => void
  /** Closes the pane at `at`, leaving the others open. */
  close: (at: number) => void
  named: (key: string, name: string) => void
  /** How wide each kind of pane is, where there's room to choose. */
  widths?: Record<PaneKind, number>
  setWidth: (kind: PaneKind, width: number) => void
  /** A note is open beside the last entry: only it shows, at full strength. */
  beside?: boolean
  /** What's open, for Ready for you. */
  opened?: number
}) {
  const shown = path.slice(beside ? -1 : -2)
  const first = path.length - shown.length
  return (
    <>
      {shown.map((step, i) => {
        const at = first + i
        const back = i < shown.length - 1
        const kind: PaneKind = back || beside ? 'back' : 'entry'
        return (
          <section
            key={`${at}:${step.key}`}
            className={`pane${back ? ' back' : ''}${beside ? ' beside' : ''}`}
            style={widths ? { width: widths[kind] } : undefined}
            aria-label={isLesson(step.key) ? 'The lesson' : back ? 'The entry before' : 'The entry'}
          >
            {widths && <Grip width={widths[kind]} set={(w) => setWidth(kind, w)} label={back ? 'The width of the entry before' : 'The width of the entry'} />}
            <button type="button" className="np-close pane-close" aria-label={back ? 'Close the entry before' : 'Close the entry'} onClick={() => close(at)}>
              <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="m3 3 6 6M9 3l-6 6" stroke="currentColor" strokeWidth="1.4" /></svg>
            </button>
            <div className="pane-in">
              {isLesson(step.key)
                ? <LessonPane id={step.key.slice(LESSON.length)} open={(key) => open(key, at)} />
                : <EntryPane ideaKey={step.key} followed={step.followed} named={named} next={path[at + 1]?.key} open={(key) => open(key, at)} opened={opened} />}
            </div>
          </section>
        )
      })}
    </>
  )
}
