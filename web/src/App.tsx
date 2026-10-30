// The app: the top of every page, and the screen for the address.
//   #/                 what to open, and what was opened lately
//   #/o/<id>           the reader, on something opened
//   #/you, #/you/…     settings
//   #/adding           adding to the shared graph: who you are, and the terms
//   #/example          the app's own example, answered with no server
//   #/states           every state of every screen, in development only

import { lazy, Suspense, useEffect, useState } from 'react'
import type { Me, Settings } from './api/types'
import { isSignedOut, listen, onSignedOut } from './api/client'
import { Start } from './screens/Start'
import { Adding } from './screens/Adding'
import { SignedOut } from './screens/SignedOut'
import { You } from './screens/You'
import { PageTop } from './Top'
import { useLoad } from './useLoad'
import { Loading } from './Loader'

// The reader brings CodeMirror and its grammars: fetched only when something's open.
const Reader = lazy(() => import('./reader/Reader').then((m) => ({ default: m.Reader })))
const Example = lazy(() => import('./screens/Example').then((m) => ({ default: m.Example })))
const States = lazy(() => import('./screens/States').then((m) => ({ default: m.States })))

function useRoute() {
  const [hash, setHash] = useState(location.hash)
  useEffect(() => {
    const on = () => setHash(location.hash)
    window.addEventListener('hashchange', on)
    return () => window.removeEventListener('hashchange', on)
  }, [])
  return hash.replace(/^#\/?/, '').split('?')[0].split('/').map((p) => decodeURIComponent(p))
}

/** Whether this browser is signed out: the first call that says so turns the whole app to the signed-out page. */
function useSignedOut() {
  const [out, setOut] = useState(isSignedOut)
  useEffect(() => onSignedOut(() => setOut(true)), [])
  return out
}

export function App() {
  // The example answers itself: it needs no kelve, and no signing in.
  if (location.hash.startsWith('#/example')) return <Suspense fallback={<Loading />}><Example /></Suspense>
  if (import.meta.env.DEV && location.hash.startsWith('#/states')) return <Suspense fallback={<Loading />}><States /></Suspense>
  return <Kelve />
}

function Kelve() {
  const out = useSignedOut()
  const me = useLoad<Me>('/api/me', ['settings', 'agent'])
  const settings = useLoad<Settings>('/api/settings', ['settings'])
  // How code and text are drawn, from settings.
  const s = settings.data
  useEffect(() => {
    if (!s) return
    const root = document.documentElement.dataset
    root.code = s.codeSize
    root.text = s.textSize
    root.highlighting = s.highlighting
    try {
      localStorage.setItem('kelve.drawn', JSON.stringify({ code: s.codeSize, text: s.textSize, highlighting: s.highlighting }))
    } catch {
      // Drawn from settings each time instead.
    }
  }, [s])
  // Changes on the server, as they happen, once kelve knows who you are.
  const known = !!me.data
  useEffect(() => (known ? listen() : undefined), [known])
  if (out) return <SignedOut />
  return <Shell me={me.data} />
}

const SCREENS = ['', 'you', 'adding']

function Shell({ me }: { me?: Me }) {
  const [screen, a] = useRoute()
  const settings = useLoad<Settings>('/api/settings', ['settings'])
  // Every page but the reader opens at its top, not where the last one was.
  useEffect(() => {
    window.scrollTo(0, 0)
  }, [screen, a])
  // Someone new meets the first run before the reader, whatever brought
  // them: their agent showing them something, or its sign-in link. It says
  // what was opened, and opens it once they're set up.
  if (screen === 'o' && Number(a) > 0 && me?.agent && !settings.data) return <Loading />
  if (screen === 'o' && Number(a) > 0 && me?.agent && !settings.data?.hints.includes('started'))
    return (
      <>
        <PageTop here="start" />
        <div className="page">
          <Start me={me} opened={Number(a)} />
        </div>
      </>
    )
  if (screen === 'o' && Number(a) > 0)
    return (
      <Suspense fallback={<Loading />}>
        <Reader key={a} id={Number(a)} />
      </Suspense>
    )
  return (
    <>
      <PageTop here={screen === 'you' ? 'settings' : screen === '' ? 'start' : undefined} />
      <div className="page">
        {screen === '' && <Start me={me} />}
        {screen === 'you' && <You focus={a} me={me} />}
        {screen === 'adding' && <Adding idea={new URLSearchParams(location.hash.split('?')[1] ?? '').get('idea') ?? undefined} />}
        {!SCREENS.includes(screen) && (
          <main className="col" style={{ marginTop: 120 }}>
            <h1 className="h1">There’s no page here.</h1>
            <p className="p soft" style={{ marginTop: 14 }}><a className="link" href="#/">Go back</a></p>
          </main>
        )}
      </div>
    </>
  )
}
