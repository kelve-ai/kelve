// A verified run (boards RunProgress and RunResult): while it goes, a square
// for each task as it comes out, the robot working, and a way to cancel; once
// it's done, how many tasks came out better, the gain and how sure it is, and
// whether it goes on your page.

import { useEffect, useState } from 'react'
import { del } from '../../api/client'
import type { RunView, ScoreView } from '../../api/types'
import { hrefOf } from '../../routes'
import { Character } from '../../ui/Character'
import { Btn } from '../../ui/kit'
import { useLoad } from '../../useLoad'
import { Squares, count, saidOf, send, shortPage, tally, useHosted } from './common'

/** How often a run's page asks how it stands. */
const EVERY = 2_000

export function Run({ id, example }: { id: string; example?: boolean }) {
  const run = useLoad<RunView>(`/api/hosted/runs/${encodeURIComponent(id)}`, ['settings'])
  const r = run.data
  const going = r?.state === 'queued' || r?.state === 'running'
  const { reload } = run
  useEffect(() => {
    if (!going || example) return
    const t = window.setInterval(reload, EVERY)
    return () => window.clearInterval(t)
  }, [going, example, reload])
  if (!r) {
    return run.error ? (
      <div className="hs-screen mid">
        <p className="hs-alert" role="alert">{run.error}</p>
        <div className="hs-acts"><Btn href={hrefOf('agent', example)}>Back to your agent</Btn></div>
      </div>
    ) : null
  }
  if (r.state === 'finished' && r.score) return <Result score={r.score} example={example} />
  return <Progress run={r} example={example} onCancelled={reload} />
}

function Progress({ run: r, example, onCancelled }: { run: RunView; example?: boolean; onCancelled: () => void }) {
  const [said, setSaid] = useState<string>()
  const t = tally(r.squares)
  const done = t.better + t.same + t.worse
  const stopped = r.state === 'cancelled' || r.state === 'failed' || r.state === 'estimated'
  const cancel = () => void del<RunView>(`/api/hosted/runs/${encodeURIComponent(r.run)}`).then(onCancelled, (e: Error) => setSaid(e.message))
  const paused = r.pausedUntil ? new Date(r.pausedUntil) : null
  return (
    <div className="hs-center">
      <Character size={184} move={stopped || paused ? 'idle' : 'work'} />
      <div className="hs-title">
        <h1>{stopped ? (r.state === 'failed' ? 'The run stopped' : r.state === 'estimated' ? 'This run didn’t start' : 'The run is cancelled') : `Your agent is doing ${r.kindName} tasks`}</h1>
        <p className="sub">
          {r.state === 'cancelled' ? 'kelve.ai deleted your skills.' : r.state === 'failed' ? (r.why ?? 'kelve.ai couldn’t finish it. Your skills are deleted.') : r.state === 'estimated' ? 'Nothing ran. Start another from Your agent.' : `Against a standard agent, on ${r.modelName}.`}
        </p>
      </div>
      <section aria-label="Tasks">
        <Squares squares={r.squares} />
        <p className="hs-tally">
          {done} of {r.squares.length} done:
          <span className="hs-dot better" aria-hidden="true" />{t.better} better,
          <span className="hs-dot same" aria-hidden="true" />{t.same} the same,
          <span className="hs-dot worse" aria-hidden="true" />{t.worse} worse.
        </p>
      </section>
      {said && <p className="hs-alert" role="alert">{said}</p>}
      <div className="hs-foot">
        {stopped ? (
          <>
            <p />
            <Btn href={hrefOf('agent', example)}>Back to your agent</Btn>
          </>
        ) : (
          <>
            <p>
              {paused
                ? `Paused: your Claude plan reached its limit. It goes on by itself at ${paused.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}.`
                : r.state === 'queued'
                  ? 'Waiting for room to start. You can leave this page.'
                  : `About ${count(r.minutesLeft ?? 1, 'minute')} left. You can leave this page.`}
            </p>
            <Btn onClick={cancel}>Cancel the run</Btn>
          </>
        )}
      </div>
    </div>
  )
}

/** "Your agent passed about 15 more of every 100 hidden checks, between 6 and 24." */
function gainOf(s: ScoreView) {
  const n = (x: number) => Math.round(x)
  if (s.verified) return <><b>Verified.</b> Your agent passed about {n(s.gain)} more of every 100 hidden checks, between {n(s.gainLow)} and {n(s.gainHigh)}.</>
  return <><b>No clear difference</b> from a standard agent, so it isn’t verified. You can still publish it, and your page says so.</>
}

function Result({ score: start, example }: { score: ScoreView; example?: boolean }) {
  const hosted = useHosted()
  const [score, setScore] = useState(start)
  const [kept, setKept] = useState(false)
  const [said, setSaid] = useState<string>()
  const show = (shown: boolean) =>
    send<ScoreView>(`/api/hosted/scores/${encodeURIComponent(score.id)}`, { shown }).then(
      (s) => {
        setScore(s)
        setKept(false)
        setSaid(undefined)
        hosted.reload()
      },
      (e: Error) => setSaid(e.message),
    )
  const page = hosted.data?.page
  return (
    <div className="hs-screen wide">
      <section className="hs-score" aria-label="Your score">
        <Character size={200} move="happy" />
        <div>
          <h1 className="hs-result">
            <span className="hs-big">
              <b>{score.better}</b>
              <span>of {score.of}</span>
            </span>
            <span className="hs-headline">{score.kindName} tasks better than a standard agent</span>
          </h1>
          <p className="hs-gain">{gainOf(score)}</p>
        </div>
      </section>
      <Squares squares={score.squares} result label={saidOf(score.squares)} />
      <section className="hs-sec" aria-label="Your page">
        {said && <p className="hs-alert" role="alert">{said}</p>}
        {score.shown ? (
          <div className="hs-done" role="status">
            <span>It’s on your page.</span>
            {page && <a className="btn btn-s" href={page} target="_blank" rel="noreferrer">Open {shortPage(page)}</a>}
            <Btn kind="quiet" href={hrefOf('agent', example)}>Back to your agent</Btn>
          </div>
        ) : kept ? (
          <div className="hs-done" role="status">
            <span>Kept private. Only you can see it.</span>
            <Btn href={hrefOf('agent', example)}>Back to your agent</Btn>
            <Btn kind="quiet" onClick={() => setKept(false)}>Change your mind</Btn>
          </div>
        ) : (
          <>
            <p className="hs-quiet">Only the score goes on your page, never your skills or your work.</p>
            <div className="hs-acts">
              <Btn kind="primary" size="large" onClick={() => void show(true)}>Publish to your page</Btn>
              <Btn kind="quiet" size="large" onClick={() => setKept(true)}>Keep it private</Btn>
            </div>
          </>
        )}
      </section>
    </div>
  )
}
