// Adding to the shared graph (board O-5), on a page of its own: the terms'
// points as the graph serves them, and who you're signed in as. Signing in
// agrees to them, and sends everything your agent wrote before, so back from
// signing in with an idea to add, it goes back to what was open. Agree is asked
// only when the terms changed since.

import { useEffect, useState } from 'react'
import { changed, post } from '../api/client'
import type { GraphAccount, GraphSignIn, Terms } from '../api/types'
import { lastOpen } from '../lastOpen'
import { SignInButtons } from './SignIn'
import { Button, Quiet } from '../ui/ui'
import { useLoad } from '../useLoad'

/** Where to go once it's done: back to what was open. */
const back = () => {
  location.hash = lastOpen()?.hash ?? '#/'
}

export function Adding({ idea }: { idea?: string }) {
  const account = useLoad<GraphAccount>('/api/graph/account', ['settings'])
  const terms = useLoad<Terms>('/api/graph/terms', [])
  const [full, setFull] = useState(false)
  const [said, setSaid] = useState<string>()
  const [busy, setBusy] = useState(false)
  const a = account.data
  const t = terms.data
  // Signed in to add an idea, and so agreed: it's sent, so back to the work.
  const added = !!idea && a?.from === 'app' && a.agreed === true
  useEffect(() => {
    if (added) back()
  }, [added])
  if (!a || !t) return <main className="adding"><p className="lede">{account.error ?? terms.error}</p></main>

  const agree = () => {
    setBusy(true)
    setSaid(undefined)
    post<GraphAccount>('/api/graph/agree', { version: t.version })
      .then(
        () => {
          changed('settings')
          back()
        },
        (e: Error) => setSaid(e.message),
      )
      .finally(() => setBusy(false))
  }
  const signIn = (provider: 'github' | 'google') =>
    post<GraphSignIn>('/api/graph/signin', { provider }).then(
      (x) => {
        window.open(x.address, '_blank', 'noopener')
        account.reload()
      },
      (e: Error) => setSaid(e.message),
    )

  return (
    <main className="adding">
      <h1>Adding to the shared graph</h1>
      {a.from === 'app' ? (
        <p className="lede">Signed in as {a.handle}. What your agent writes and learns goes to the graph on these terms.</p>
      ) : (
        <>
          <p className="lede">
            {a.waiting ? 'Finish signing in, in the browser tab that opened. This page carries on when you’re back.' : 'Sign in with GitHub or Google. They only tell the graph who you are: it keeps a number for your account, never your name or email.'}
          </p>
          {!a.waiting && <SignInButtons signIn={(p) => void signIn(p)} />}
        </>
      )}
      <ul className="points">
        {t.points.map((p) => <li key={p}>{p}</li>)}
      </ul>
      <p className="d">
        The rest of the terms, and how the graph handles what it keeps about you:{' '}
        <a className="l" href="https://kelve.ai/terms" target="_blank" rel="noopener">Terms</a>,{' '}
        <a className="l" href="https://kelve.ai/privacy" target="_blank" rel="noopener">Privacy</a>.
      </p>
      {a.from === 'app' && a.agreed ? (
        <p className="lede">You agreed to these.</p>
      ) : (
        a.from === 'app' && (
          <>
            <p className="d">The terms changed since you signed in. Using the shared graph free, what your agent writes and learns is added for everyone: entries about ideas, how they relate, and lessons. Never your work. You see everything you’ve added in settings.</p>
            <p className="row">
              <Button disabled={busy} onClick={agree}>{idea ? `Agree and add ${idea}` : 'Agree'}</Button>
              <Quiet onClick={() => setFull((f) => !f)}>{full ? 'Hide the full terms' : 'Read the full terms'}</Quiet>
            </p>
          </>
        )
      )}
      {(full || (a.from === 'app' && a.agreed)) && <pre className="terms-full">{t.text}</pre>}
      {said && <p className="warn">{said}</p>}
    </main>
  )
}
