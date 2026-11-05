// Each screen of kelve under the header (routes.ts), by its address. Until a
// screen is built, it's a stub: its name, and a line on what it will show,
// so no address is ever a blank page. Each stub is replaced here by its
// screen as it's built.

import { lazy, Suspense, useEffect } from 'react'
import { Header } from '../Header'
import { Loading } from '../Loader'
import { hrefOf, placeOf, type Route, SECTIONS, type Section } from '../routes'
import { Character } from '../ui/Character'
import { Customise } from './agent/Customise'
import { SettingsSection } from './agent/Settings'
import { Welcome } from './agent/Welcome'
import { YourAgent } from './agent/YourAgent'
import { AccountSection } from './hosted/Account'
import { Address } from './hosted/Address'
import { Run } from './hosted/Run'
import { RunSetup } from './hosted/RunSetup'
import { isSkillsRoute, SkillsScreens, type SkillsRoute } from './Skills'
import { Debrief } from './Debrief'
import { Home } from './Home'

// The change view brings CodeMirror and its grammars: fetched only when it's opened.
const Change = lazy(() => import('./Change').then((m) => ({ default: m.Change })))

/** What each settings section is called. */
const SECTION_NAMES: Record<Section, string> = {
  agent: 'Your agent',
  account: 'Account',
  checks: 'Skill checks',
  reading: 'Reading',
  privacy: 'Privacy',
  about: 'About',
}

/** What each settings section will hold. */
const SECTION_LINES: Record<Section, string> = {
  agent: 'Which agents are connected, when debriefs come, and whether your skills go in each agent’s own skills folder.',
  account: 'Sign in to kelve.ai for verified scores and a page of your own. You don’t need an account for anything else.',
  checks: 'When kelve checks each part of your skills, and how many checks it runs in a day.',
  reading: 'How code and text look: their size, how code is coloured, and light or dark.',
  privacy: 'What leaves, what stays, and how to delete what kelve keeps.',
  about: 'Which kelve this is, and how to update it.',
}

/** A stub's name, what it's for, and the address's own part, if it has one. */
function stubOf(route: Exclude<Route, SkillsRoute>): { name: string; line: string; what?: string } {
  switch (route.screen) {
    case 'home':
      return { name: 'Home', line: 'Your agent, what it needs from you, and what happened lately, a day at a time.' }
    case 'debrief':
      return { name: 'Debrief', line: 'What your agent did, how it works and every decision it made. Agree, ask for it another way, or have your agent look into it.', what: `Debrief ${route.id}` }
    case 'change':
    case 'handover':
      return { name: 'The change', line: 'Every file your agent changed, with each decision beside the lines it holds.', what: `Change ${route.id}` }
    case 'agent':
      return { name: 'Your agent', line: 'It keeps what you know as skills, and gets better with each piece of work. What it knows, and how well it fits you.' }
    case 'address':
      return { name: 'Choose your page’s address', line: '' }
    case 'look':
      return { name: 'Change how it looks', line: 'Build your agent from parts, in colours you choose, or ask it to draw itself.' }
    case 'score-new':
      return { name: 'A verified score', line: 'Choose a kind of work, an agent and a model. You see exactly what goes before anything goes.' }
    case 'score':
      return { name: 'Verified score', line: 'Your agent with your skills against a standard agent on the same model, task by task.', what: `Score ${route.id}` }
    case 'settings':
      return { name: SECTION_NAMES[route.section], line: SECTION_LINES[route.section] }
    case 'welcome':
      return { name: 'Meet your agent', line: 'It gets better with each piece of work you go through together. Pick a look to start.' }
  }
}

/** Where the character stands on a stub: the screens that are about your agent. */
const WITH_CHARACTER: Route['screen'][] = ['home', 'agent', 'look', 'welcome']

function Stub({ route }: { route: Exclude<Route, SkillsRoute> }) {
  const { name, line, what } = stubOf(route)
  return (
    <div className="stub" data-stub={route.screen}>
      {WITH_CHARACTER.includes(route.screen) && <Character size={route.screen === 'welcome' ? 176 : 160} move={route.screen === 'welcome' ? 'wave' : 'idle'} />}
      <h1>{name}</h1>
      {what && <p className="stub-what">{what}</p>}
      <p className="stub-line">{line}</p>
      <p className="stub-soon">
        This page is still being built.
      </p>
    </div>
  )
}

function Settings({ section, example }: { section: Section; example?: boolean }) {
  return (
    <div className="sets">
      <nav className="sets-nav" aria-label="Settings">
        <h1 className="sr-only">Settings</h1>
        {SECTIONS.map((s) => (
          <a key={s} className="nv" href={hrefOf(`settings/${s}`, example)} aria-current={s === section ? 'page' : undefined}>
            {SECTION_NAMES[s]}
          </a>
        ))}
      </nav>
      <section className="sets-body" aria-label={SECTION_NAMES[section]} data-stub={`settings-${section}`}>
        <h2>{SECTION_NAMES[section]}</h2>
        {section === 'account' ? <AccountSection example={example} /> : <SettingsSection section={section} example={example} />}
      </section>
    </div>
  )
}

/** The screen for an address: the header, then the page. `example` keeps every link in the example. */
export function Screen({ route, example }: { route: Route; example?: boolean }) {
  const key = JSON.stringify(route)
  // Every screen opens at its top.
  useEffect(() => {
    window.scrollTo(0, 0)
  }, [key])
  // Changing how your agent looks has a header of its own, with Cancel and Save.
  if (route.screen === 'look') return <Customise example={example} />
  return (
    <div className="app">
      <Header here={placeOf(route)} example={example} bare={route.screen === 'welcome'} />
      <main className={route.screen === 'settings' || isSkillsRoute(route) ? 'app-main' : ['home', 'debrief', 'change', 'handover'].includes(route.screen) ? 'app-own' : 'app-main narrow'}>
        {route.screen === 'settings' ? (
          <Settings section={route.section} example={example} />
        ) : isSkillsRoute(route) ? (
          <SkillsScreens route={route} example={example} />
        ) : route.screen === 'home' ? (
          <Home example={example} />
        ) : route.screen === 'debrief' ? (
          <Debrief key={`${route.id}${location.hash.split('?')[1] ?? ''}`} id={route.id} example={example} />
        ) : route.screen === 'change' || route.screen === 'handover' ? (
          <Suspense fallback={<Loading />}>
            <Change key={`${route.screen}:${route.id}`} id={route.id} example={example} handover={route.screen === 'handover'} />
          </Suspense>
        ) : route.screen === 'welcome' ? (
          <Welcome example={example} />
        ) : route.screen === 'address' ? (
          <Address example={example} />
        ) : route.screen === 'score-new' ? (
          <RunSetup example={example} />
        ) : route.screen === 'score' ? (
          <Run id={route.id} example={example} />
        ) : route.screen === 'agent' ? (
          <YourAgent example={example} />
        ) : (
          <Stub route={route} />
        )}
      </main>
    </div>
  )
}
