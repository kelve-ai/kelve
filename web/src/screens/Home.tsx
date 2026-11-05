// Home (boards Home, HomeEmpty, HomeWorking, HomeAttention): your agent,
// big, with what it says in a bubble beside it, the few things it asks of
// you, and what happened, a day at a time, each line with its pixel icon.

import { useEffect, useState } from 'react'
import type { HomeAsk, HomeView, TimelineAct, TimelineItem } from '../api/types'
import { post } from '../api/client'
import { exampleHome } from '../example/debriefs'
import { hrefOf } from '../routes'
import { useLoad } from '../useLoad'
import { Character } from '../ui/Character'
import { useToast } from '../ui/kit'
import { type Pixel, PixelIcon } from '../ui/PixelIcon'

const ACT: Record<TimelineAct, string> = { undo: 'Undo', debriefIt: 'Debrief it', open: 'Open', tryItOn: 'Try it on', seeIt: 'See it' }

const asOf = () => new URLSearchParams(location.hash.split('?')[1] ?? '').get('as')

/** Minutes and seconds since a time: 4:12. */
function since(from: string, now: number): string {
  const s = Math.max(0, Math.floor((now - new Date(from).getTime()) / 1000))
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
}

/** A clock that ticks once a second while it's shown. */
function useNow(on: boolean): number {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    if (!on) return
    const t = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(t)
  }, [on])
  return now
}

const time = (at: string) => new Date(at).toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' })

export function Home({ example }: { example?: boolean }) {
  const loaded = useLoad<HomeView>(example ? null : '/api/home', ['debriefs', 'asks', 'agent', 'notes', 'settings'])
  const [mock, setMock] = useState<HomeView | null>(() => (example ? exampleHome(asOf()) : null))
  const home = example ? mock : loaded.data
  const [toast, say] = useToast(3200)
  const [fixed, setFixed] = useState(false)
  const [busy, setBusy] = useState<string | null>(null)
  const now = useNow(!!home?.workingSince)

  if (!home) {
    if (loaded.error) return <p className="hm-error" role="alert">{loaded.error}</p>
    return <div className="hm" aria-busy="true" />
  }

  /** Does what an ask's button says; in the example, only on the page. */
  const run = async (ask: HomeAsk, alt = false) => {
    setBusy(ask.id)
    try {
      if (ask.kind === 'hook' && alt) {
        if (!example) await post('/api/home/later', { ask: 'hook' })
        else setMock((m) => m && { ...m, asks: m.asks.filter((a) => a.id !== ask.id), say: 'A few things for you.' })
        say('I’ll ask again tomorrow.')
      } else if (ask.kind === 'hook') {
        if (!example) await post('/api/settings', { notesHook: true })
        else setMock((m) => m && { ...m, asks: m.asks.filter((a) => a.id !== ask.id) })
        setFixed(true)
      } else if (ask.kind === 'stalled') {
        const id = ask.id.replace(/^again:/, '')
        if (!example) await post(`/api/debriefs/${id}/again`)
        else setMock((m) => m && { ...m, asks: m.asks.filter((a) => a.id !== ask.id) })
        say('I’ll write it up next time I work.')
      }
      loaded.reload()
    } catch (e) {
      say((e as Error).message)
    } finally {
      setBusy(null)
    }
  }

  /** Debrief it: a debrief of work too small for one, written by your agent. */
  const debriefIt = async (item: TimelineItem) => {
    if (example) return say('In your own kelve, your agent writes this up.')
    try {
      const { id } = await post<{ id: string }>(`/api/handovers/${item.id.replace(/^handover:/, '')}/debrief`)
      location.hash = hrefOf(`debrief/${id}`)
    } catch (e) {
      say((e as Error).message)
    }
  }

  const working = home.state === 'working'
  const thanks = fixed && !home.asks.some((a) => a.kind === 'hook')
  const status = thanks ? 'All set' : working && home.workingSince ? `Working, ${since(home.workingSince, now)}` : home.status
  const dot = thanks || home.status === 'All set' ? 'ok' : home.state === 'empty' ? 'new' : working ? 'live' : 'wait'
  const words = thanks ? 'Thanks. I can reach kelve again.' : home.say
  const asks = home.asks
  return (
    <div className="hm">
      <section className="hm-me" aria-label="Your agent">
        <Character size={240} move={thanks ? 'happy' : home.moving === 'still' ? 'idle' : home.moving} label={home.name} />
        <div className="hm-name">
          <h1>{home.name}</h1>
          <a className="btn btn-g btn-sm btn-icon" href={hrefOf('agent/look', example)} aria-label="Change how it looks" title="Change how it looks">
            <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M18.4 2.6a2 2 0 0 1 2.9 2.9L11 15.8 8 16l.2-3 10.2-10.4Z" />
              <path d="M7 17c-2 0-3.5 1.4-3.5 3.5H7a3 3 0 0 0 0-6" />
            </svg>
          </a>
        </div>
        <span className="hm-status"><span className={`hm-dot ${dot}`} aria-hidden="true" />{status}</span>
      </section>

      <div className="hm-main">
        <section className="hm-bubble" aria-label="From your agent" key={words}>
          <p className="hm-say" aria-live="polite">{words}</p>
          {asks.length > 0 && (
            <div className="hm-asks">
              {asks.map((a) => (
                <div className="hm-ask" key={a.id}>
                  <PixelIcon kind={a.kind === 'fits' ? 'score' : (a.kind as Pixel)} />
                  <p>{a.text}</p>
                  {a.action && a.to && (
                    <a className={`btn ${a.main ? 'btn-p' : 'btn-s'}`} href={hrefOf(a.to, example)}>{a.action}</a>
                  )}
                  {a.action && !a.to && (
                    <button type="button" className={`btn ${a.main ? 'btn-p' : 'btn-s'}`} disabled={busy === a.id} onClick={() => void run(a)}>{a.action}</button>
                  )}
                  {a.alt && <button type="button" className="btn btn-g" disabled={busy === a.id} onClick={() => void run(a, true)}>{a.alt}</button>}
                </div>
              ))}
            </div>
          )}
        </section>

        <section className="hm-tl" aria-label="What happened">
          {home.days.length === 0 && (
            <div className="hm-empty"><span className="hm-ghost" aria-hidden="true" />What your agent does shows up here.</div>
          )}
          {home.days.map((d) => (
            <div className="hm-grp" key={d.label}>
              <h2 className="hm-day">{d.label}</h2>
              <ul className="hm-rows">
                {d.items.map((e, i) => (
                  <li className="hm-row" key={e.id} style={{ animationDelay: `${0.05 * i}s` }}>
                    <PixelIcon kind={e.kind as Pixel} />
                    <span className="hm-t">
                      {e.title}
                      {e.squares.length > 0 && (
                        <span className="hm-sq" role="img" aria-label={squaresSaid(e.squares)}>
                          {e.squares.map((q, j) => <i key={j} className={q} />)}
                        </span>
                      )}
                    </span>
                    {e.act === 'debriefIt' && <button type="button" className="btn btn-g btn-sm" onClick={() => void debriefIt(e)}>{ACT[e.act]}</button>}
                    {e.act && e.act !== 'debriefIt' && e.to && <a className="btn btn-g btn-sm" href={hrefOf(e.to, example)}>{ACT[e.act]}</a>}
                    <time className="hm-when" dateTime={e.at}>{time(e.at)}</time>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </section>
      </div>
      {toast}
    </div>
  )
}

/** A score's squares, said: "10 better, 1 the same, 1 worse". */
function squaresSaid(squares: TimelineItem['squares']): string {
  const n = (k: string) => squares.filter((q) => q === k).length
  return [`${n('better')} better`, `${n('same')} the same`, `${n('worse')} worse`].join(', ')
}
