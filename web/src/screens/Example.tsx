// The app's own example, answered from what the sandbox recorded, with no
// server: the states gallery and the screenshot tests draw every screen from
// it. #/example is the reader, on a change an agent made to a small payments
// service, with its own address after it; #/example/moved a tab whose thing
// was shown again in a newer one. `theme=light` or `theme=dark` after the
// address draws it that way; `as=paused` draws things opened before paused,
// `as=paused-just` one paused a moment ago, and `as=next` several things next
// on your agent's list, your agent not working. `as=update` draws a newer
// kelve out, in settings. kelve's screens under its header have their
// addresses after #/example/ too (routes.ts), home as #/example/home, and
// #/example/kit shows every control; `as=dialog` opens its dialog, and
// `as=toast` shows its toast.

import { useEffect, useState } from 'react'
import { answerFromExample, type Fixture } from '../api/client'
import type { AgentList, ListItem, Settings, UpdateView } from '../api/types'
import { hostedAnswers } from '../example/hosted'
import { skillsExample } from '../example/skills'
import { Loading } from '../Loader'
import { Reader } from '../reader/Reader'
import { routeOf } from '../routes'
import { Kit } from './Kit'
import { Screen } from './Screen'
import { Moved } from './Moved'

/** The example's own screens: the rest are kelve's screens under its header, or its controls. */
const OWN = ['', 'moved', 'kit']

/** The screen the example's address names. */
function screenOf(hash: string): string {
  const [, screen = ''] = hash.split('?')[0].replace(/^#\//, '').split('/')
  return screen
}

export function Example() {
  const [f, setF] = useState<Fixture | null>(null)
  // Drawn again as its address changes: the header's links lead between screens.
  const [hash, setHash] = useState(location.hash)
  useEffect(() => {
    const on = () => setHash(location.hash)
    window.addEventListener('hashchange', on)
    return () => window.removeEventListener('hashchange', on)
  }, [])
  const screen = screenOf(hash)
  useEffect(() => {
    const theme = new URLSearchParams(location.hash.split('?')[1] ?? '').get('theme')
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme
    void import('../example/example.json').then((m) => {
      const fixture = structuredClone(m.default) as unknown as Fixture
      // Code drawn as the example's settings say, as the app draws it from yours.
      const drawn = fixture.answers['/api/settings'] as Settings | undefined
      if (drawn) Object.assign(document.documentElement.dataset, { code: drawn.codeSize, text: drawn.textSize, highlighting: drawn.highlighting })
      // The change is what's open: nothing else was asked for.
      const list = fixture.answers['/api/list'] as AgentList
      list.open ??= fixture.opened
      list.elsewhere ??= []
      const as = new URLSearchParams(location.hash.split('?')[1] ?? '').get('as')
      // Several things next, your agent not working: decisions to redo.
      if (as === 'next') {
        const redo = (n: number, title: string, sub: string, ask: number): ListItem => ({ key: `redo:1:${n}`, kind: 'redo', title, sub, opened: fixture.opened, askedIn: null, asks: [ask] })
        list.working = false
        list.next = [
          redo(1, 'Say what a retry costs the bank', 'On src/retry.rs 12–28', 16),
          redo(2, 'Name the backoff after what it waits for', 'On src/retry.rs 30–34', 17),
        ]
      }
      // kelve.ai: Sam signed in, with a page and scores on it.
      Object.assign(fixture.answers, hostedAnswers(as))
      // kelve is up to date; with `as=update`, a newer one is out.
      const update: UpdateView = as === 'update'
        ? { version: '0.2.1', newer: { version: '0.3.0', line: 'Your agent can update kelve for you, once you say yes.' } }
        : { version: '0.2.1', newer: null }
      fixture.answers['/api/update'] = update
      // Your skills, and Fits you.
      Object.assign(fixture.answers, skillsExample(as))
      answerFromExample(fixture)
      setF(fixture)
    })
  }, [screen])
  // kelve's screens under its header are answered from the example once it's
  // loaded; its controls need nothing from it.
  const route = OWN.includes(screen) ? null : routeOf(hash.split('?')[0].replace(/^#\//, '').split('/').slice(1).map((p) => decodeURIComponent(p)))
  if (route) return f ? <Screen route={route} example /> : <Loading />
  if (screen === 'kit') return <Kit />
  if (!f) return <Loading />
  if (screen === 'moved') return <Moved />
  return <Reader id={f.opened} example />
}
