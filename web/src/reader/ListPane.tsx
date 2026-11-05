// Your agent's list: what it's doing now, what's next for what you have
// open, what you asked for in other things, and what's ready for you. One
// line each, kelve's square saying where each stands; Do next and Take off on
// the row you point at. What's on it is what you sent back from a debrief, to
// have it another way.

import { useContext, useEffect, useRef, useState } from 'react'
import { doWaiting, OpenContext, toDo } from '../agentList'
import { post } from '../api/client'
import type { AgentList, ListItem } from '../api/types'
import { State } from '../ui/ui'
import { agentName } from '../words'

const KIND: Record<string, string> = { redo: 'Redo' }

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
  const said = meta ?? ''
  return (
    <div className={`lrow${item.sub ? ' top' : ''}`} role="listitem">
      <State state={state} />
      <button type="button" className="lt lopen" onClick={() => onOpen(item)}>
        <i>{KIND[item.kind] ?? item.kind}</i>
        {item.title}
      </button>
      {onFirst || onOff ? (
        <span className="lact">
          {onOff && <button type="button" className="quiet-link loff" aria-label={`Take ${(KIND[item.kind] ?? item.kind).toLowerCase()} ${item.title} off the list`} onClick={() => onOff(item)}>Take off</button>}
          {onFirst && <button type="button" className="raised small" onClick={() => onFirst(item)}>Do next</button>}
        </span>
      ) : null}
      <span className="lm">{said}</span>
      {item.sub && <small>{item.sub}</small>}
    </div>
  )
}

export function ListPane({ list, agent, onOpen, close }: {
  list: AgentList | undefined
  /** Your agent, as people say it: "Claude Code". */
  agent: string
  onOpen: (item: ListItem) => void
  close: () => void
}) {
  const [said, setSaid] = useState<string>()
  const first = (item: ListItem) => void post('/api/list/first', { asks: item.asks })
  // Taken off: every job the row stands for, put aside.
  const off = (body: { asks: number[] }) => {
    setSaid(undefined)
    post('/api/list/off', body).catch((e: Error) => setSaid(e.message))
  }
  // Asked for in something else, or still being done for it: in what, in
  // place of how far, so it's never taken for what's open's.
  const where = (item: ListItem) => (item.askedIn ? `in ${item.askedIn}` : undefined)
  return (
    <div className="lpane">
      <div className="lhead">
        <h2>{agent}’s list</h2>
        <span className={`lstate${list?.working ? '' : ' idle'}`}>
          <State state={list?.working ? 'now' : 'queued'} />
          {list?.working ? 'Working' : 'Waiting'}
        </span>
        <button type="button" className="np-close lclose" onClick={close} aria-label="Close the list">
          <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="m3 3 6 6M9 3l-6 6" stroke="currentColor" strokeWidth="1.4" /></svg>
        </button>
      </div>
      {list && !list.working && toDo(list) > 0 && <IdleLine agent={agent} left={toDo(list)} />}
      {list && list.now.length > 0 && (
        <section aria-label="Now">
          <p className="lgroup"><b>Now</b></p>
          <div role="list">{list.now.map((i) => <Row key={i.key} item={i} state="now" onOpen={onOpen} meta={where(i)} onOff={(it) => off({ asks: it.asks })} />)}</div>
        </section>
      )}
      <section aria-label="Next">
        <p className="lgroup"><b>Next</b>{list && list.next.length > 0 && <span>{list.next.length}</span>}</p>
        <div role="list">{list?.next.map((i) => <Row key={i.key} item={i} state="queued" onOpen={onOpen} onFirst={first} onOff={(it) => off({ asks: it.asks })} />)}</div>
        {list && list.next.length === 0 && <p className="side-none">Nothing’s next.</p>}
      </section>
      {list && list.elsewhere.length > 0 && (
        <section aria-label="Asked for in other things">
          <p className="lgroup"><b>Asked for in other things</b><span>{list.elsewhere.length}</span></p>
          <div role="list">{list.elsewhere.map((i) => <Row key={i.key} item={i} state="queued" onOpen={onOpen} meta={where(i)} onOff={(it) => off({ asks: it.asks })} />)}</div>
        </section>
      )}
      {list && list.ready.length > 0 && (
        <section aria-label="Ready for you">
          <p className="lgroup"><b>Ready for you</b><span>{list.ready.length}</span></p>
          <div role="list">{list.ready.map((i) => <Row key={i.key} item={i} state="ready" onOpen={onOpen} />)}</div>
        </section>
      )}
      {said && <p className="warn">{said}</p>}
    </div>
  )
}

/** The agent's line at the top of the reader: its square, its name, and how much is left. */
export function AgentLine({ list, agent: chosen, open, onClick }: {
  list: AgentList | undefined
  /** The agent chosen in settings: the one working is named while one is. */
  agent: string
  open?: boolean
  onClick: () => void
}) {
  const left = toDo(list)
  const working = !!list?.working
  const agent = working && list?.by ? agentName(list.by) : chosen
  return (
    <button
      type="button"
      className={`agent-line${working ? '' : ' idle'}${open ? ' open' : ''}`}
      // What it shows comes first, as it reads, so it can be said to pick it.
      aria-label={`${agent} ${left} to do: its list, ${working ? 'working' : 'waiting'}`}
      aria-expanded={open}
      onClick={onClick}
    >
      <State state={working ? 'now' : 'queued'} />
      {/* Spaces between the parts, which flex doesn't draw, so the text
          reads as it looks: "Claude Code 2", as its label begins. */}
      <span className="an">{agent}</span>{' '}
      <i>{left}</i>
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

/** The one line for your agent's list, at its top while nothing's taking it (board Waiting-6): kelve knows only that, not what your agent's doing, so it says what to do. */
function IdleLine({ agent, left }: { agent: string; left: number }) {
  return (
    <div className="wline">
      <State state="queued" />
      <span className="wl-t"><b>Waiting for you to ask {agent}</b> <span className="wl-q">· {left} thing{left === 1 ? '' : 's'} to do</span></span>
      <CopySay agent={agent} />
    </div>
  )
}
