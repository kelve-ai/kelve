// Your agent's list (boards Q-2, Q-3, Q-5, J-1 to J-3): what it's doing now,
// what's next for what you have open, what you asked for in other things,
// what's ready for you, and what's paused. One line each, kelve's square saying
// where each stands; Do next and Take off on the row you point at. Notes for
// your agent join the list: the way to add to it is to leave one.

import { useContext, useEffect, useRef, useState } from 'react'
import { doWaiting, OpenContext, placeSaid, toDo, type Where } from '../agentList'
import { post } from '../api/client'
import type { AgentList, ListItem, Paused, Reading } from '../api/types'
import { State } from '../ui/ui'
import { agentName, roughly } from '../words'
import { AtWork, DOING } from './Side'

/** How far reading what's open has got, as the list counts its lines. */
type ListReading = Pick<Reading, 'read' | 'of' | 'pages' | 'pagesLeft'> & { opened: number }

const KIND: Record<string, string> = { reading: 'Reading', pages: 'Pages', plainly: 'Plainly', entry: 'Entry', note: 'Note' }
const OPENED: Record<string, string> = { change: 'Change', folder: 'Folder', file: 'File' }

/** How long something paused stays tinted, saying so. */
const JUST = 60 * 1000

/** Whether what's paused is shown, per browser. */
const SHOWN = 'kelve.list.paused'

function Row({ item, state, onOpen, onFirst, onOff, meta }: {
  item: ListItem
  state: 'ready' | 'now' | 'queued' | 'done'
  onOpen: (item: ListItem) => void
  onFirst?: (item: ListItem) => void
  /** Takes it off the list: every job it stands for is put aside. */
  onOff?: (item: ListItem) => void
  /** What's said on the right, in place of the usual. */
  meta?: string
}) {
  // Reading and pages say what they are in kelve's words; what's in hand
  // is under them, each square filling as it goes (List-1, List-2).
  const lined = item.kind === 'reading' || item.kind === 'pages'
  const mono = item.kind === 'plainly'
  const said = meta ?? ''
  return (
    <div className={`lrow${item.sub || item.hand.length ? ' top' : ''}`} role="listitem">
      <State state={state} />
      <button type="button" className="lt lopen" onClick={() => onOpen(item)}>
        {!lined && <i>{KIND[item.kind] ?? item.kind}</i>}
        {mono ? <code>{item.title}</code> : item.title}
      </button>
      {onFirst || onOff ? (
        <span className="lact">
          {onOff && <button type="button" className="quiet-link loff" aria-label={`Take ${lined ? item.title.toLowerCase() : `${(KIND[item.kind] ?? item.kind).toLowerCase()} ${item.title}`} off the list`} onClick={() => onOff(item)}>Take off</button>}
          {onFirst && <button type="button" className="raised small" onClick={() => onFirst(item)}>Do next</button>}
        </span>
      ) : null}
      <span className="lm">{said}</span>
      {item.sub && <small>{item.sub}</small>}
      {item.hand.length > 0 && (
        <ul className="lhand" aria-label="In hand now">
          {item.hand.map((h) => (
            <li key={h.path}>
              <AtWork stage={h.stage} said={`${h.path}: ${DOING[h.stage].toLowerCase()}`} />
              <code>{h.path}</code>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/** How far a line on the list has got: on what's open's, how many of its files or pages are done, of how many; on the rest's pages, the same; else how many are left. */
function counted(item: ListItem, now: boolean, reading?: ListReading): string | undefined {
  const left = item.left ?? 0
  const r = reading && item.opened === reading.opened ? reading : undefined
  const pages = r?.pages ?? 0
  const pagesDone = pages - (r?.pagesLeft ?? 0)
  let done: number | undefined
  let of: number | undefined
  if (item.key.startsWith('read:') && r) {
    of = r.of - pages
    done = Math.min(r.read - pagesDone, of)
  } else if (item.key.startsWith('pages:') && r && pages > 0) {
    of = pages
    done = pagesDone
  } else if (item.of != null) {
    of = item.of
    done = item.of - left
  }
  if (now && of != null && of > 0) return `${Math.max(0, done ?? 0)} of ${of}`
  if (!left) return undefined
  const unit = item.kind === 'reading' ? 'file' : item.key.startsWith('pages:') && !item.title.startsWith('Folder') ? 'page' : 'folder'
  return now ? `${left} to go` : `${left} ${unit}${left === 1 ? '' : 's'}`
}

/** The way in and its folders' pages, done, while the rest's pages are written after them (List-2). */
function doneLines(list: AgentList | undefined, reading?: ListReading): Array<{ title: string; count: string }> {
  if (!list || !reading) return []
  const on = [...list.now, ...list.next]
  const lines = (key: string) => on.some((i) => i.key === `${key}:${reading.opened}`)
  if (!lines('rest') || lines('read') || lines('pages')) return []
  const files = reading.of - reading.pages
  return [
    ...(files > 0 ? [{ title: 'Reading the way in', count: `${files} of ${files}` }] : []),
    ...(reading.pages > 0 ? [{ title: 'Folder pages for the way in', count: `${reading.pages} of ${reading.pages}` }] : []),
  ]
}

/** Something opened before what's open, its reading paused: Open goes back to it; Take off puts its reading aside. */
function PausedRow({ p, just, onOpen, onOff }: { p: Paused; just: boolean; onOpen: (p: Paused) => void; onOff: (p: Paused) => void }) {
  return (
    <div className={`lrow top${just ? ' just' : ''}`} role="listitem">
      <span className="lpause" aria-hidden="true" />
      <span className="lt"><i>{OPENED[p.kind] ?? p.kind}</i>{p.title}</span>
      <span className="lm lboth">
        <button type="button" className="quiet-link loff" aria-label={`Take ${p.title} off the list`} onClick={() => onOff(p)}>Take off</button>
        <button type="button" className="raised small" onClick={() => onOpen(p)}>Open</button>
      </span>
      <small>{p.left} file{p.left === 1 ? '' : 's'} left to read · {just ? 'paused just now' : roughly(p.openedAt)}</small>
    </div>
  )
}

/** Whether something was paused just now: true until a minute after, when it's drawn again. */
function useJust(paused: Paused[]): (p: Paused) => boolean {
  const [now, setNow] = useState(() => Date.now())
  const at = paused.map((p) => new Date(p.pausedAt).getTime())
  const soonest = Math.min(...at.filter((t) => now - t < JUST))
  useEffect(() => {
    if (!Number.isFinite(soonest)) return
    const t = window.setTimeout(() => setNow(Date.now()), soonest + JUST - Date.now() + 100)
    return () => window.clearTimeout(t)
  }, [soonest])
  return (p) => now - new Date(p.pausedAt).getTime() < JUST
}

/** What's paused, as the list's last group: one line until shown (J-1), then a row each (J-2); what was paused just now stays out, tinted (J-3). */
function PausedGroup({ paused, onOpen, onOff }: { paused: Paused[]; onOpen: (p: Paused) => void; onOff: (p: Paused) => void }) {
  const [shown, setShown] = useState(() => {
    try {
      return localStorage.getItem(SHOWN) === 'shown'
    } catch {
      return false
    }
  })
  const just = useJust(paused)
  const show = (to: boolean) => {
    setShown(to)
    try {
      localStorage.setItem(SHOWN, to ? 'shown' : 'hidden')
    } catch {
      // Remembered for this page only.
    }
  }
  const out = shown ? paused : paused.filter(just)
  const more = paused.length - out.length
  return (
    <section aria-label="Paused">
      <p className="lgroup">
        <b>Paused</b><span>{paused.length}</span>
        <button type="button" className="quiet-link lfold" onClick={() => show(!shown)}>{shown ? 'Hide' : 'Show'}</button>
      </p>
      <div role="list">
        {out.map((p) => <PausedRow key={p.opened} p={p} just={just(p)} onOpen={onOpen} onOff={onOff} />)}
        {more > 0 && (
          <div className="lrow top lrest" role="listitem">
            <span className="lpause" aria-hidden="true" />
            <span className="lt">{out.length ? `${more} more` : `${more} thing${more === 1 ? '' : 's'} you opened before`}</span>
            <span />
            <small>{out.length ? 'Opened before that.' : 'Not read further until you open them again.'}</small>
          </div>
        )}
      </div>
    </section>
  )
}

export function ListPane({ list, agent, onOpen, onOpenPaused, reading, newNote, close }: {
  list: AgentList | undefined
  /** Your agent, as people say it: "Claude Code". */
  agent: string
  onOpen: (item: ListItem) => void
  /** Opens again something paused: it's what's open, and its reading carries on. */
  onOpenPaused: (p: Paused) => Promise<void>
  /** How far reading what's open has got. */
  reading?: ListReading
  /** Starts a note for your agent. */
  newNote: () => void
  close: () => void
}) {
  const [said, setSaid] = useState<string>()
  const first = (item: ListItem) => void post('/api/list/first', { asks: item.asks })
  // Taken off: every job the row stands for, or what's paused's reading, put aside.
  const off = (body: { asks?: number[]; paused?: number }) => {
    setSaid(undefined)
    post('/api/list/off', body).catch((e: Error) => setSaid(e.message))
  }
  const openPaused = (p: Paused) => {
    setSaid(undefined)
    onOpenPaused(p).catch((e: Error) => setSaid(e.message))
  }
  // Asked for in something else, or still being done for it: in what, in
  // place of how far, so it's never taken for what's open's.
  const where = (item: ListItem) => (item.askedIn ? `in ${item.askedIn}` : undefined)
  // While the rest's pages are written: the way in and its pages, done (List-2).
  const done = doneLines(list, reading)
  return (
    <div className="lpane">
      <div className="lhead">
        <h2>{agent}’s list</h2>
        <span className={`lstate${list?.working ? '' : ' idle'}`}>
          <State state={list?.working ? 'now' : 'queued'} />
          {list?.working ? 'Working' : 'Not working'}
        </span>
        <button type="button" className="np-close lclose" onClick={close} aria-label="Close the list">
          <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="m3 3 6 6M9 3l-6 6" stroke="currentColor" strokeWidth="1.4" /></svg>
        </button>
      </div>
      {list && !list.working && toDo(list) > 0 && <IdleLine agent={agent} left={toDo(list)} />}
      {list && list.now.length > 0 && (
        <section aria-label="Now">
          <p className="lgroup"><b>Now</b></p>
          <div role="list">{list.now.map((i) => <Row key={i.key} item={i} state="now" onOpen={onOpen} meta={where(i) ?? counted(i, true, reading)} onOff={(it) => off({ asks: it.asks })} />)}</div>
        </section>
      )}
      <section aria-label="Next">
        <p className="lgroup"><b>Next</b>{list && list.next.length > 0 && <span>{list.next.length}</span>}</p>
        <div role="list">{list?.next.map((i) => <Row key={i.key} item={i} state="queued" onOpen={onOpen} meta={counted(i, false, reading)} onFirst={first} onOff={(it) => off({ asks: it.asks })} />)}</div>
        <button type="button" className="ladd" onClick={newNote}>Note for {agent}</button>
      </section>
      {done.length > 0 && (
        <section aria-label="Done">
          <p className="lgroup"><b>Done</b></p>
          <div role="list">{done.map((d) => (
            <div key={d.title} className="lrow" role="listitem">
              <State state="done" />
              <span className="lt">{d.title}</span>
              <span className="lm">{d.count}</span>
            </div>
          ))}</div>
        </section>
      )}
      {list && list.elsewhere.length > 0 && (
        <section aria-label="Asked for in other things">
          <p className="lgroup"><b>Asked for in other things</b><span>{list.elsewhere.length}</span></p>
          <div role="list">{list.elsewhere.map((i) => <Row key={i.key} item={i} state="queued" onOpen={onOpen} meta={where(i) ?? counted(i, false)} onOff={(it) => off({ asks: it.asks })} />)}</div>
        </section>
      )}
      {list && list.ready.length > 0 && (
        <section aria-label="Ready for you">
          <p className="lgroup"><b>Ready for you</b><span>{list.ready.length}</span></p>
          <div role="list">{list.ready.map((i) => <Row key={i.key} item={i} state="ready" onOpen={onOpen} />)}</div>
        </section>
      )}
      {list && list.paused.length > 0 && <PausedGroup paused={list.paused} onOpen={openPaused} onOff={(p) => off({ paused: p.opened })} />}
      {said && <p className="warn">{said}</p>}
    </div>
  )
}

/** The agent's line at the top of every page: its square, its name, and how much is left; while what's open is read, how far that's got (board H-5), the line under it filling as files are read, each once its ideas are checked and it's read plainly, then a folder's or a change's own page; lighter for files read whose ideas are still being checked, or that are still being read plainly. */
export function AgentLine({ list, agent: chosen, open, onClick, reading }: {
  list: AgentList | undefined
  /** The agent chosen in settings: the one working is named while one is. */
  agent: string
  open?: boolean
  onClick: () => void
  /** How far reading what's open has got, while it's read. */
  reading?: Pick<Reading, 'read' | 'of' | 'checking'>
}) {
  const left = toDo(list)
  const working = !!list?.working
  const agent = working && list?.by ? agentName(list.by) : chosen
  const of = reading?.of ?? 0
  const read = Math.min(reading?.read ?? 0, of)
  const checking = Math.min(reading?.checking ?? 0, of - read)
  const said = reading && of > 0 ? (working ? `reading ${read} of ${of}` : `${read} of ${of} read`) : null
  return (
    <button
      type="button"
      className={`agent-line${working ? '' : ' idle'}${open ? ' open' : ''}${said ? ' reading' : ''}`}
      // What it shows comes first, as it reads, so it can be said to pick it.
      aria-label={`${agent} ${said ?? `${left} to do`}: its list, ${working ? 'working' : 'not working'}`}
      aria-expanded={open}
      onClick={onClick}
    >
      <State state={working ? 'now' : 'queued'} />
      {/* Spaces between the parts, which flex doesn't draw, so the text
          reads as it looks: "Claude Code 2", as its label begins. */}
      <span className="an">{agent}</span>{' '}
      {said ? <em>{said}</em> : <i>{left}</i>}
      {said && <>{' '}<i className="short">{read}/{of}</i></>}
      {said && (
        <span className="mini" aria-hidden="true">
          <i style={{ width: `${(100 * read) / of}%` }} />
          {checking > 0 && <i className="checking" style={{ width: `${(100 * checking) / of}%` }} />}
        </span>
      )}
    </button>
  )
}

/**
 * What to say to your agent to have it work through its list, a click
 * away: once copied, where to paste it, until the page changes.
 */
function CopySay({ agent }: { agent: string }) {
  const say = doWaiting(useContext(OpenContext))
  const [copied, setCopied] = useState(false)
  const said = useRef<HTMLSpanElement>(null)
  useEffect(() => {
    if (copied) said.current?.focus()
  }, [copied])
  const copy = () => {
    // Where the browser won't let a page write to the clipboard, the words
    // are selected and copied as a person would.
    const byHand = () => {
      const area = document.createElement('textarea')
      area.value = say
      area.setAttribute('readonly', '')
      area.style.position = 'fixed'
      area.style.opacity = '0'
      document.body.append(area)
      area.select()
      const done = document.execCommand('copy')
      area.remove()
      if (done) setCopied(true)
    }
    if (navigator.clipboard) void navigator.clipboard.writeText(say).then(() => setCopied(true), byHand)
    else byHand()
  }
  return copied ? (
    <span ref={said} className="wl-ok" role="status" tabIndex={-1}>Copied. Paste it to {agent}.</span>
  ) : (
    <button type="button" className="raised small" onClick={copy} aria-label={`Copy what to say to ${agent}: ${say}`}>Copy what to say</button>
  )
}

/** The one line for your agent's list, at its top while your agent isn't working (board Waiting-6). */
function IdleLine({ agent, left }: { agent: string; left: number }) {
  return (
    <div className="wline">
      <State state="queued" />
      <span className="wl-t"><b>{agent} isn’t working</b> <span className="wl-q">· {left} thing{left === 1 ? '' : 's'} waiting</span></span>
      <CopySay agent={agent} />
    </div>
  )
}

/**
 * Something asked for and not written yet, in one slim line (boards
 * Waiting-0 to Waiting-5): kelve's square, a few words, and at most one
 * thing to do, always there. Being written, a thin moving line; first on
 * the list, nothing to do; further down, Do next; your agent not working,
 * what to say to it. An entry being written has its steps under it.
 */
export function WaitingLine({ page, where, working, agent, writing, reading, steps }: {
  /** What it's for, a file's path or an idea's key: Copied lasts until it changes. */
  page: string
  where: Where
  working: boolean
  /** Your agent, as people say it: "Claude Code". */
  agent: string
  /** Being written now, as kelve knows it from the job, whatever the list says. */
  writing?: boolean
  /** The file itself being read now, before it's written plainly (Read-3). */
  reading?: boolean
  /** While it's being written: each step, done, under way, or to come. */
  steps?: Array<{ said: string; state: 'fin' | 'now' | 'wait' }>
}) {
  if (reading || writing || where?.state === 'now') {
    return (
      <div className="waiting">
        <div className="wline" role="status">
          <State state="now" />
          <span className="wl-t"><b>{reading && !writing ? `${agent} is reading this now` : `${agent} is writing this`}</b></span>
          <span className="wl-bar" aria-hidden="true"><i /></span>
        </div>
        {steps && steps.length > 0 && (
          <ul className="wl-steps" aria-label="Its steps">
            {steps.map((s) => (
              <li key={s.said} data-state={s.state}>
                <State state={s.state === 'fin' ? 'done' : s.state === 'now' ? 'now' : 'queued'} label={s.state === 'fin' ? 'Done' : s.state === 'now' ? 'Under way' : 'To come'} />
                {s.said}
              </li>
            ))}
          </ul>
        )}
      </div>
    )
  }
  const next = where?.state === 'next' ? where : null
  const place = placeSaid(where)
  // Part of reading what's open: how many files are read before it.
  const files = next?.files != null
  if (!working) {
    const when = !next ? 'on its list' : !place ? 'next when it starts' : files ? place : `${place} when it starts`
    return (
      <div className="waiting">
        <div className="wline">
          <State state="queued" />
          <span className="wl-t" role="status"><b>{agent} isn’t working</b> <span className="wl-q">· {when}</span></span>
          <CopySay key={page} agent={agent} />
        </div>
      </div>
    )
  }
  return (
    <div className="waiting">
      <div className="wline">
        <State state="queued" />
        <span className="wl-t" role="status"><b>{!next ? `On ${agent}’s list` : !place ? `Next for ${agent}` : files ? `${agent} reads ${place}` : `${place} for ${agent}`}</b></span>
        {next && place && next.item.asks.length > 0 && (
          <button type="button" className="raised small" onClick={() => void post('/api/list/first', { asks: next.item.asks })}>Do next</button>
        )}
      </div>
    </div>
  )
}
