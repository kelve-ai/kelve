// Your agent's page, on Your agent (boards YourAgent and YourAgentLive):
// a way back to a run still going, whichever else it shows; without a page, a
// way to get a verified score, signing in first; with one, its address to
// copy, how it stands, every verified score with a switch for whether it
// shows, and a way to take the page down. Your agent's screen puts this under
// its character and what it knows.

import { useCallback, useEffect, useState } from 'react'
import type { HostedView, ScoreView } from '../../api/types'
import { hrefOf } from '../../routes'
import { Btn, Toggle, useToast } from '../../ui/kit'
import { useLoad } from '../../useLoad'
import { SignInSheet, Squares, saidOf, send, shortPage, START, useHosted } from './common'

const LINK = (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7" />
  </svg>
)

export function YourPage({ example }: { example?: boolean }) {
  const hosted = useHosted()
  const h = hosted.data
  const scores = useLoad<ScoreView[]>(h?.signedIn ? '/api/hosted/scores' : null, ['settings'])
  const [signing, setSigning] = useState(() => example && new URLSearchParams(location.hash.split('?')[1] ?? '').get('as') === 'sign-in')
  const [toast, say] = useToast()
  const onSignedIn = useCallback(() => {
    setSigning(false)
    if (!example) location.hash = hrefOf(START, example)
  }, [example])
  if (!h) return null
  const list = scores.data ?? []
  if (!h.signedIn || !h.page || list.length === 0) {
    return (
      <div className="hs-page">
        {h.going && <Going run={h.going} example={example} />}
        {!h.going && h.available && (
          <section className="hs-cta" aria-label="Your page">
            <p>A verified score gives your agent a page to share.</p>
            {h.signedIn ? (
              <Btn kind="primary" size="large" href={hrefOf(START, example)}>Get a verified score</Btn>
            ) : (
              <Btn kind="primary" size="large" onClick={() => setSigning(true)}>Get a verified score</Btn>
            )}
          </section>
        )}
        {signing && <SignInSheet onClose={() => setSigning(false)} onDone={onSignedIn} />}
      </div>
    )
  }
  return (
    <div className="hs-page">
      {h.going && <Going run={h.going} example={example} />}
      <Live h={h} scores={list} example={example} reload={() => { hosted.reload(); scores.reload() }} say={say} />
      {toast}
    </div>
  )
}

/** A run still going, with a way back to it, however the page stands. */
function Going({ run, example }: { run: string; example?: boolean }) {
  return (
    <section className="hs-cta" aria-label="Your run">
      <p>Your agent is doing a verified run.</p>
      <Btn href={hrefOf(`agent/score/${encodeURIComponent(run)}`, example)}>See how it’s going</Btn>
    </section>
  )
}

function Live({ h, scores, example, reload, say }: { h: HostedView; scores: ScoreView[]; example?: boolean; reload: () => void; say: (t: string) => void }) {
  const [ask, setAsk] = useState(false)
  const [said, setSaid] = useState<string>()
  // What each switch shows while kelve.ai is asked.
  const [shown, setShown] = useState<Record<string, boolean>>({})
  useEffect(() => setShown({}), [scores])
  const page = h.page ?? ''
  const on = (s: ScoreView) => shown[s.id] ?? s.shown
  const showing = scores.filter((s) => on(s)).length
  const status = !h.public ? 'Your page is down. Turn a score on to put it back up.' : showing === 0 ? 'Nothing showing: your page has only your agent.' : `${showing} ${showing === 1 ? 'score' : 'scores'} on your page.`
  const toggle = (s: ScoreView, to: boolean) => {
    setShown((x) => ({ ...x, [s.id]: to }))
    send<ScoreView>(`/api/hosted/scores/${encodeURIComponent(s.id)}`, { shown: to }).then(reload, (e: Error) => {
      setSaid(e.message)
      setShown((x) => ({ ...x, [s.id]: s.shown }))
    })
  }
  const down = () =>
    send<HostedView>('/api/hosted/page', { public: false }).then(
      () => {
        setAsk(false)
        reload()
        say('Your page is taken down.')
      },
      (e: Error) => setSaid(e.message),
    )
  const copy = () => void navigator.clipboard?.writeText(page).then(() => say('Link copied'), () => {})
  return (
    <>
      <section className="hs-link" aria-label="Your page">
        <div>
          <a className="addr" href={page} target="_blank" rel="noreferrer">{shortPage(page)}</a>
          <Btn size="small" onClick={copy}>{LINK}Copy link</Btn>
        </div>
        <span className={`hs-on${h.public && showing > 0 ? ' up' : ''}`} role="status"><i aria-hidden="true" />{status}</span>
      </section>
      {said && <p className="hs-alert" role="alert">{said}</p>}
      <section className="hs-scores" aria-labelledby="hs-scores-h">
        <header>
          <h2 id="hs-scores-h">Verified scores</h2>
          <Btn kind="primary" href={hrefOf('agent/score/new', example)}>Get another score</Btn>
        </header>
        {scores.map((s) => (
          <div className="hs-srow" key={s.id}>
            <div className="k">
              <a href={hrefOf(`agent/score/${encodeURIComponent(s.run)}`, example)}>{s.kindName}</a>
              <span>{s.modelName}{s.verified ? '' : s.judgeCounted ? ', no clear difference' : ', not verified'}</span>
            </div>
            <div className="sq-at">
              <Squares squares={s.squares} small dim={!(on(s) && h.public)} label={saidOf(s.squares)} />
            </div>
            <div className="won"><b>{s.better}</b><span>of {s.of}</span></div>
            <div className="sh">
              <span id={`hs-show-${s.id}`}>On your page</span>
              <Toggle on={on(s) && h.public} label={`Show ${s.kindName} on ${s.modelName} on your page`} onChange={(to) => toggle(s, to)} />
            </div>
          </div>
        ))}
      </section>
      {h.public && (
        <section className="hs-down" aria-label="Take your page down">
          {ask ? (
            <>
              <span>Take your page down? Your scores are kept.</span>
              <Btn size="small" kind="quiet" onClick={() => setAsk(false)}>Keep it up</Btn>
              <Btn size="small" onClick={() => void down()}><span className="hs-danger">Take it down</span></Btn>
            </>
          ) : (
            <Btn size="small" kind="quiet" onClick={() => setAsk(true)}>Take your page down</Btn>
          )}
        </section>
      )}
    </>
  )
}
