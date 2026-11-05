// The app: the top of every page, and the screen for the address.
//   #/, and the rest   kelve's screens under its header (routes.ts)
//   #/o/<id>           what your agent opened: its debrief, the change, or the reader
//   #/example          the app's own example, answered with no server
//   #/states           every state of every screen, in development only

import { lazy, Suspense, useEffect, useState } from 'react'
import type { Me, Settings } from './api/types'
import { isSignedOut, listen, onSignedOut } from './api/client'
import { SignedOut } from './screens/SignedOut'
import { Moved } from './screens/Moved'
import { isMoved, onMoved, showHere } from './tabs'
import { Screen } from './screens/Screen'
import { routeOf } from './routes'
import { met } from './screens/agent/looks'
import { Header } from './Header'
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

/** Whether what this tab showed is open in a newer one. */
function useMoved() {
  const [moved, setMoved] = useState(isMoved)
  useEffect(() => onMoved(() => setMoved(isMoved())), [])
  return moved
}

function Kelve() {
  const out = useSignedOut()
  const moved = useMoved()
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
  if (moved) return <Moved showHere={showHere} />
  return <Shell />
}

/** Where an address kelve no longer has leads now: settings, and home for the start page. */
function movedTo(screen: string, part?: string): string | null {
  if (screen === 'you') return `#/settings/${part === 'reading' || part === 'marks' ? 'reading' : 'agent'}`
  if (screen === 'start' || screen === 'adding') return '#/'
  return null
}

function Shell() {
  const parts = useRoute()
  const [screen, a] = parts
  const route = routeOf(parts)
  const settings = useLoad<Settings>('/api/settings', ['settings'])
  // Every page but the reader opens at its top, not where the last one was.
  useEffect(() => {
    window.scrollTo(0, 0)
  }, [screen, a])
  // Someone new meets their agent before anything else: home sends them there.
  const meet = route?.screen === 'home' && !met.now && !!settings.data && !settings.data.hints.includes('met')
  useEffect(() => {
    // Only from home still: the address may have moved on while settings came.
    if (meet && ['', '#', '#/'].includes(location.hash)) location.replace('#/welcome')
  }, [meet])
  const moved = movedTo(screen, a)
  useEffect(() => {
    if (moved) location.replace(moved)
  }, [moved])
  if (screen === 'o' && Number(a) > 0)
    return (
      <Suspense fallback={<Loading />}>
        <Reader key={a} id={Number(a)} />
      </Suspense>
    )
  // Someone new meets their agent first: home, until they have.
  if (route?.screen === 'home' && !settings.data) return <Loading />
  if (route) return <Screen route={route} />
  if (moved) return <Loading />
  return (
    <div className="app">
      <Header here={null} />
      <main className="app-main narrow">
        <h1 className="h1" style={{ marginTop: 120 }}>There’s no page here.</h1>
        <p className="p soft" style={{ marginTop: 14 }}><a className="link" href="#/">Go back</a></p>
      </main>
    </div>
  )
}
