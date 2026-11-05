// What kelve.ai's screens share: asking kelve, the account, a row of
// squares, and signing in (board SignIn), from wherever it's needed.

import { useEffect, useId, useRef, useState } from 'react'
import { get, inExample, post } from '../../api/client'
import type { HostedSignIn, HostedView, ScoreSquare, TaskSquare } from '../../api/types'
import { Character } from '../../ui/Character'
import { Btn, Spinner } from '../../ui/kit'
import { useLoad } from '../../useLoad'
import '../../styles/hosted.css'

/**
 * Asks kelve to do something. The example can't be changed, so there it
 * reads what the same address would answer instead, to show what comes next.
 */
export const send = <T,>(path: string, body: unknown = {}) => (inExample() ? get<T>(path) : post<T>(path, body))

/** The account on kelve.ai, fetched again whenever settings change. */
export const useHosted = () => useLoad<HostedView>('/api/hosted', ['settings'])

/** A page's address as people read it: `kelve.ai/@sam`. */
export const shortPage = (page: string) => page.replace(/^https?:\/\//, '')

/** A file's size: `2.4 KB`. */
export function size(bytes: number) {
  if (bytes < 1000) return `${bytes} bytes`
  return `${(bytes / 1024).toFixed(1)} KB`
}

/** "1 run", "3 runs". */
export const count = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`

/** How each square is said. */
const SAID: Record<TaskSquare, string> = { better: 'Better', same: 'The same', worse: 'Worse', running: 'Running now', waiting: 'Waiting' }

/** A square for each task, in a grid as wide as there are tasks. */
/** How many squares a row has room for at least: a bank's tasks. */
const ROW = 12

export function Squares({ squares, small, result, dim, label }: {
  squares: Array<TaskSquare | ScoreSquare>
  small?: boolean
  /** Coming in one by one, as a result does. */
  result?: boolean
  dim?: boolean
  /** Said for all of them at once, as a picture; without it, each is said. */
  label?: string
}) {
  const cls = ['hs-sqs', small && 'small', result && 'result', dim && 'dim'].filter(Boolean).join(' ')
  // Room for a whole bank's squares, so a run of a few keeps them square-sized.
  const style = { gridTemplateColumns: `repeat(${Math.max(squares.length, ROW)}, minmax(0, 1fr))` }
  if (label) {
    return (
      <div className={cls} style={style} role="img" aria-label={label}>
        {squares.map((s, i) => <span key={i} className={`hs-sq ${s}`} style={result ? { animationDelay: `${i * 60}ms` } : undefined} />)}
      </div>
    )
  }
  return (
    <div className={cls} style={style} role="list">
      {squares.map((s, i) => (
        <span key={i} role="listitem" aria-label={`Task ${i + 1}: ${SAID[s]}`} className={`hs-sq ${s}`}>
          {s === 'running' && <span className="spin" aria-hidden="true" />}
        </span>
      ))}
    </div>
  )
}

/** How many better, the same and worse, said for a row of squares. */
export function tally(squares: Array<TaskSquare | ScoreSquare>) {
  const n = (s: string) => squares.filter((x) => x === s).length
  return { better: n('better'), same: n('same'), worse: n('worse') }
}

/** "10 better, 1 the same and 1 worse than a standard agent". */
export function saidOf(squares: Array<TaskSquare | ScoreSquare>) {
  const t = tally(squares)
  return `${t.better} better, ${t.same} the same and ${t.worse} worse than a standard agent`
}

const GITHUB = (
  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M9 19c-5 1.5-5-2.5-7-3m14 6v-3.9a3.4 3.4 0 0 0-.9-2.6c3.1-.4 6.4-1.5 6.4-7A5.4 5.4 0 0 0 20 4.8 5 5 0 0 0 19.9 1S18.7.6 16 2.5a13.4 13.4 0 0 0-7 0C6.3.6 5.1 1 5.1 1A5 5 0 0 0 5 4.8a5.4 5.4 0 0 0-1.5 3.8c0 5.4 3.3 6.6 6.4 7a3.4 3.4 0 0 0-.9 2.5V22" />
  </svg>
)
const GOOGLE = (
  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M20 12a8 8 0 1 1-2.3-5.7M20 12h-8" />
  </svg>
)
const OUT = (
  <svg className="out" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M14 4h6v6M20 4l-9 9M18 14v6H4V6h6" />
  </svg>
)

/** Starts signing in with GitHub or Google, in a new tab: what went wrong, if anything. */
export async function signIn(provider: 'github' | 'google'): Promise<string | undefined> {
  try {
    const at = await post<HostedSignIn>('/api/hosted/signin', { provider })
    window.open(at.address, '_blank', 'noopener')
    return undefined
  } catch (e) {
    return (e as Error).message
  }
}

/**
 * Signing in to kelve.ai, over the page: GitHub or Google, in a tab of its
 * own, then back here. Once kelve is signed in, `onDone`.
 */
export function SignInSheet({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const hosted = useHosted()
  const h = hosted.data
  const [said, setSaid] = useState<string>()
  const id = useId()
  const sheet = useRef<HTMLElement>(null)
  const close = useRef(onClose)
  useEffect(() => {
    close.current = onClose
  })
  useEffect(() => {
    if (h?.signedIn) onDone()
  }, [h?.signedIn, onDone])
  // Focus goes in, Escape closes it, and focus goes back where it was.
  useEffect(() => {
    const before = document.activeElement as HTMLElement | null
    sheet.current?.querySelector<HTMLElement>('button')?.focus()
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close.current()
    }
    window.addEventListener('keydown', key)
    return () => {
      window.removeEventListener('keydown', key)
      before?.focus()
    }
  }, [])
  const go = (p: 'github' | 'google') => void signIn(p).then((s) => (s ? setSaid(s) : hosted.reload()))
  const stop = () => void post('/api/hosted/signin/stop').then(hosted.reload, () => {})
  return (
    <div className="scrim" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <section className="sheet hs-sheet" role="dialog" aria-modal="true" aria-labelledby={id} ref={sheet}>
        <div className="hs-sheet-top">
          <Character size={64} move="wave" />
          <h2 id={id}>Sign in to kelve.ai</h2>
        </div>
        <div className="hs-why">
          <p>For a verified score, and a page for your agent.</p>
          <p>Your work never leaves, and nothing is public until you publish.</p>
        </div>
        {h?.waiting ? (
          <div className="hs-row">
            <Spinner>Finish signing in, in the browser tab that opened.</Spinner>
            <Btn kind="quiet" onClick={stop}>Cancel</Btn>
          </div>
        ) : (
          <div className="hs-ways">
            <button type="button" className="btn btn-s wide" onClick={() => go('github')}>{GITHUB}Continue with GitHub{OUT}</button>
            <button type="button" className="btn btn-s wide" onClick={() => go('google')}>{GOOGLE}Continue with Google{OUT}</button>
            <Btn kind="quiet" onClick={onClose}>Not now</Btn>
          </div>
        )}
        {said && <p className="hs-alert" role="alert">{said}</p>}
      </section>
    </div>
  )
}

/** Where getting a verified score starts. A page's address is needed only to publish one. */
export const START = 'agent/score/new'

/** Choosing a page's address, then back to `then`, a screen in kelve. */
export const addressThen = (then: string) => `agent/address?then=${encodeURIComponent(then)}`
