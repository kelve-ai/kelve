// Fits you: two results of one past task side by side, one by your agent with
// your skills and one by a standard agent, which is which hidden until you
// pick the one you'd rather keep. Then kelve says which was yours, and how
// close each came to what you kept. Your agent's page shows the tally.

import { useState } from 'react'
import { ApiError, post } from '../api/client'
import type { FitsPicked, FitsView } from '../api/types'
import { Wait } from './SkillsWait'
import { hrefOf } from '../routes'
import { Character } from '../ui/Character'
import { Btn } from '../ui/kit'
import { useLoad } from '../useLoad'

/** A change, line by line: what it adds and takes out, coloured. */
function Diff({ text, label }: { text: string; label: string }) {
  const lines = text.replace(/\n$/, '').split('\n')
  if (!text.trim()) return <p className="fit-none">No change.</p>
  return (
    <pre className="fit-diff" aria-label={label} tabIndex={0}>
      {lines.map((l, i) => {
        const cls = l.startsWith('+++') || l.startsWith('---') || l.startsWith('diff ') || l.startsWith('index ') ? 'h' : l.startsWith('@@') ? 'at' : l.startsWith('+') ? 'add' : l.startsWith('-') ? 'del' : ''
        return (
          <span key={i} className={cls}>
            {l}
            {'\n'}
          </span>
        )
      })}
    </pre>
  )
}

export function Fits({ example }: { example?: boolean }) {
  const fits = useLoad<FitsView>('/api/fits', ['skills'])
  const [picked, setPicked] = useState<FitsPicked | null>(null)
  const [error, setError] = useState('')
  if (!fits.data) return <Wait error={fits.error} example={example} />
  const f = fits.data
  const said = (e: unknown) => setError(e instanceof ApiError ? e.message : 'Something went wrong. Try again in a moment.')
  const start = async () => {
    try {
      await post('/api/fits/start')
      fits.reload()
    } catch (e) {
      said(e)
    }
  }
  const pick = async (side: 'left' | 'right' | 'same') => {
    if (!f.pick) return
    try {
      setPicked(await post<FitsPicked>(`/api/fits/${f.pick.id}/pick`, { picked: side }))
    } catch (e) {
      said(e)
    }
  }
  const tally = f.tally.of > 0 && (
    <p className="fit-tally">
      You picked your agent in <strong>{f.tally.picked} of {f.tally.of}</strong> blind tests.
    </p>
  )

  if (picked) {
    const mine = picked.mine === 'left' ? 'A' : 'B'
    const theirs = mine === 'A' ? 'B' : 'A'
    const chose = picked.picked === 'same' ? null : picked.picked === picked.mine
    return (
      <div className="fit">
        <h1>{chose === null ? 'About the same' : chose ? 'You picked your agent' : 'You picked the standard agent'}</h1>
        <p className="fit-line">
          {mine} was your agent, with your skills. {theirs} was a standard agent.
          {picked.skillsClose != null && picked.standardClose != null && (
            <> Yours came {picked.skillsClose}% close to what you kept; the standard one, {picked.standardClose}%.</>
          )}
        </p>
        <div className="sk-acts">
          <Btn kind="primary" onClick={() => { setPicked(null); fits.reload() }}>Next</Btn>
          <Btn kind="quiet" href={hrefOf('agent', example)}>Your agent</Btn>
        </div>
      </div>
    )
  }

  if (!f.pick)
    return (
      <div className="fit">
        <Character size={96} move={f.waiting > 0 ? 'work' : 'idle'} />
        <h1>Fits you</h1>
        {f.waiting > 0 ? (
          <p className="fit-line">
            Your agent is redoing {f.waiting === 1 ? 'a past task' : `${f.waiting} past tasks`}, with your skills and without. Each shows here when it’s ready.
          </p>
        ) : f.tasks > 0 ? (
          <>
            <p className="fit-line">Your agent redoes some of your past work twice: with your skills, and as a standard agent. You pick the one you’d rather keep, without knowing which is which.</p>
            <div className="sk-acts">
              <Btn kind="primary" onClick={() => void start()}>Start</Btn>
            </div>
          </>
        ) : (
          <p className="fit-line">There’s no past work to redo yet. Once your agent hands over work and you keep it, there will be.</p>
        )}
        {tally}
        {error && <p className="sk-error" role="alert">{error}</p>}
      </div>
    )

  return (
    <div className="fit">
      <h1>Which would you rather keep?</h1>
      <blockquote className="fit-ask">{f.pick.request}</blockquote>
      <div className="fit-sides">
        <section aria-label="A">
          <h2>A</h2>
          <Diff text={f.pick.left} label="A’s change" />
        </section>
        <section aria-label="B">
          <h2>B</h2>
          <Diff text={f.pick.right} label="B’s change" />
        </section>
      </div>
      <div className="sk-acts">
        <Btn onClick={() => void pick('left')}>Keep A</Btn>
        <Btn onClick={() => void pick('right')}>Keep B</Btn>
        <Btn kind="quiet" onClick={() => void pick('same')}>About the same</Btn>
      </div>
      {error && <p className="sk-error" role="alert">{error}</p>}
      {tally}
    </div>
  )
}
