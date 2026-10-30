// Before anything's open (boards O-1, O-2). Until an agent has connected: one
// step, connect your agent, with the finished example beside it to click around
// in while you do, and where to say what kelve marks for you. When your agent
// says hello, the page moves on by itself: the words to say to it, letting it
// pick up your notes and lessons, and what you opened lately, each with what's
// ready for you in it and how much is still on your agent's list (boards R-3,
// L-3); and, asked once, whether to share anonymous counts (boards Count-1,
// Count-2).

import { useState } from 'react'
import { useAgentList } from '../agentList'
import { changed, post } from '../api/client'
import type { GraphAccount, GraphSignIn, MarksView, Me, Opened, Settings, SettingsIn } from '../api/types'
import { Button, Choice, CopyWell, Quiet } from '../ui/ui'
import { useLoad } from '../useLoad'
import { agentKindName, agentName, roughly } from '../words'
import { AGENTS, agentChoice, INSTALLED, SHOW_ME } from './agents'
import { WRITE_MARKS } from './Marks'
import { ProblemsNotice } from './Problems'
import { type Provider, SignInButtons } from './SignIn'
import { Square } from '../reader/Square'

const STEPS: Array<[string, string]> = [
  ['Ask it to show you its work.', 'When it finishes something, it opens it in kelve, marked with the ideas it rests on.'],
  ['Go as deep as you want.', 'Click an idea for what it is, how it works and why; then what it rests on, down to first principles.'],
  ['Leave notes.', 'For yourself, with the work, or for your agent to pick up.'],
]

function save(patch: SettingsIn) {
  return post<Settings>('/api/settings', patch).then(() => changed('settings'))
}

/** O-1: connect your agent, with the example beside it. */
function FirstRun({ me, settings }: { me?: Me; settings?: Settings }) {
  const found = me?.found ?? []
  const [kind, setKind] = useState(() => settings?.agent ?? found[0] ?? 'claudeCode')
  const a = agentChoice(kind)
  const choose = (k: typeof kind) => {
    setKind(k)
    void save({ agent: k })
  }
  return (
    <main className="ob">
      <div className="ob-l">
        <h1>Understand what your agent did.</h1>
        <p className="lede">kelve opens your agent’s work in plain words, part by part, with each idea down to first principles.</p>
        <ol className="steps">
          <li className="now">
            <span className="n">1</span>
            <div>
              <h2>Connect your agent.</h2>
              <p>{a.how}</p>
              <Choice
                label="Your agent"
                value={kind}
                onChange={choose}
                options={AGENTS.map((x) => ({ value: x.kind, label: x.name, dot: found.includes(x.kind) ? INSTALLED : undefined }))}
              />
              <CopyWell text={a.line} />
              <p className="wait" role="status"><i aria-hidden="true" />Waiting for {a.kind === 'other' ? 'your agent' : a.name}. It connects the next time you start it.</p>
            </div>
          </li>
          {STEPS.map(([title, line], i) => (
            <li key={title} className="later">
              <span className="n">{i + 2}</span>
              <div>
                <h2>{title}</h2>
                <p>{line}</p>
              </div>
            </li>
          ))}
        </ol>
        <div className="opt ob-opt">
          <div>
            <b>What kelve marks for you</b>
            <span>Every idea but the basics, to start. Say how well you know each area in settings, or ask {a.kind === 'other' ? 'your agent' : a.name} once it’s connected: “{WRITE_MARKS}”</span>
          </div>
          <Quiet onClick={() => { location.hash = '#/you/marks' }}>Settings</Quiet>
        </div>
      </div>
      <div className="ob-r">
        <p className="plbl">An example: a change an agent made, in kelve. Click around while you wait.</p>
        <div className="peek">
          <iframe title="An example of kelve" src="/#/example" />
        </div>
      </div>
    </main>
  )
}

/** What each level marks, in words. */
const MARKS: Record<string, string> = {
  new: 'every idea, the basics too',
  learning: 'every idea but the basics',
  working: 'advanced and specialist ideas',
  deep: 'specialist ideas only',
}

/** One thing the install set up, ticked when it's done, with a word to change it; asked, a question mark. */
function Tick({ done, asked, children, act }: { done: boolean; asked?: boolean; children: React.ReactNode; act?: React.ReactNode }) {
  return (
    <li className={done ? 'done' : asked ? 'asked' : ''}>
      <span className="t" aria-hidden="true">{done ? '✓' : asked ? '?' : ''}</span>
      <div>{children}</div>
      {act}
    </li>
  )
}

/**
 * Sharing anonymous counts (boards Count-1 and
 * Count-2): asked once, as one more line; answered, a tick with Stop, or
 * a line with Share. `DO_NOT_TRACK` or `KELVE_TELEMETRY=off` turns it off
 * whatever was chosen, and the line says so.
 */
function CountsTick({ settings, failed }: { settings: Settings; failed: (e: Error) => void }) {
  const share = (on: boolean) => save({ shareCounts: on, hintSeen: 'counts' }).catch(failed)
  if (settings.countsOff) {
    return (
      <Tick done={false}>
        Doesn’t share counts
        <small>{settings.countsOff === 'DO_NOT_TRACK' ? 'DO_NOT_TRACK is set' : 'KELVE_TELEMETRY is off'}, so nothing is sent.</small>
      </Tick>
    )
  }
  if (!settings.hints.includes('counts')) {
    return (
      <Tick
        done={false}
        asked
        act={
          <span className="row">
            <Button small onClick={() => void share(true)}>Share</Button>
            <Quiet onClick={() => void share(false)}>No thanks</Quiet>
          </span>
        }
      >
        Share anonymous counts of how you use kelve
        <small>Numbers only, once a week: what you open and read, how lessons went. Never your work, your name or your account. Settings shows exactly what goes.</small>
      </Tick>
    )
  }
  return settings.shareCounts ? (
    <Tick done act={<Quiet onClick={() => void share(false)}>Stop</Quiet>}>
      It shares anonymous counts of how you use kelve
      <small>Numbers only, once a week. Settings shows exactly what goes.</small>
    </Tick>
  ) : (
    <Tick done={false} act={<Quiet onClick={() => void share(true)}>Share</Quiet>}>
      Doesn’t share counts
    </Tick>
  )
}

/**
 * Joining the shared graph, the one choice left once kelve is set up: what it
 * means said plainly, the terms in a line, and going without it (Nathan, 26
 * October; board Setup-2).
 */
function JoinGraph({ settings }: { settings: Settings }) {
  const account = useLoad<GraphAccount>('/api/graph/account', ['settings'])
  const [said, setSaid] = useState<string>()
  const a = account.data
  if (!a || !a.graph || a.from !== null || settings.hints.includes('graph')) return null
  const signIn = (provider: Provider) =>
    post<GraphSignIn>('/api/graph/signin', { provider }).then(
      (x) => {
        window.open(x.address, '_blank', 'noopener')
        account.reload()
      },
      (e: Error) => setSaid(e.message),
    )
  return (
    <section className="joinq" aria-label="Join the shared graph">
      <h2>Join the shared graph</h2>
      <p>Most ideas open at once, because someone’s agent has already written them. It’s free: in return, what your agent writes and learns is added for everyone. Never your work.</p>
      {a.waiting ? <p>Finish signing in, in the browser tab that opened.</p> : <SignInButtons signIn={(p) => void signIn(p)} />}
      <p className="fine">
        Everything in it is public domain (CC0). Your reads never say who you are.{' '}
        <Quiet onClick={() => void save({ hintSeen: 'graph' })}>Use kelve without it</Quiet>
      </p>
      {said && <p className="warn">{said}</p>}
    </section>
  )
}

/**
 * Once an agent has connected (board Setup-2): what the install set up, as
 * ticks, each changed in a word; the one choice left, the shared graph; then
 * the words to say, or what the agent opened, to open now. Once something's
 * been opened from here, the ticks have done their job: what was opened
 * lately comes first, and settings keep the rest.
 */
function Connected({ me, settings, opened }: { me: Me; settings?: Settings; opened?: number }) {
  const lately = useLoad<Opened[]>('/api/opened', ['opened', 'idea'])
  const marks = useLoad<MarksView>('/api/marks', ['settings', 'spec'])
  const list = useAgentList().data
  const [said, setSaid] = useState<string>()
  // How much is still on the list for each thing opened.
  const left = (id: number) => (list ? [...list.now, ...list.next, ...list.elsewhere].filter((i) => i.opened === id).length : 0)
  const when = (o: Opened) => roughly(o.openedAt).replace(/^./, (c) => c.toUpperCase())
  // The agent you chose, if it's the one that connected; else the one that did.
  const who = agentKindName(settings?.agent)
  const named = me.agents.some((a) => agentName(a.name) === who) ? who : agentName(me.agent?.name)
  const Named = named.charAt(0).toUpperCase() + named.slice(1)
  const choice = agentChoice(settings?.agent)
  const hook = (on: boolean) => save({ notesHook: on, hintSeen: 'hook' }).catch((e: Error) => setSaid(e.message))
  const level = marks.data?.kept ? marks.data.spec.mark : undefined
  // What the agent opened, when it brought someone new here; opening it, or
  // anything else, ends the first run.
  const first = opened ? lately.data?.find((o) => o.id === opened) : undefined
  const started = () => void save({ hintSeen: 'started' })
  const back = !first && !!settings?.hints.includes('started')
  const recent = !first && lately.data && lately.data.length > 0 && (
    <nav className="recent" aria-label="Opened lately">
      <p className="lgroup"><b>Opened lately</b><span>{lately.data.length}</span></p>
      {lately.data.map((o) => (
        <div key={o.id} className="recent-one">
          <a href={`#/o/${o.id}`} onClick={started}>
            <span className="what">{o.title}</span>
            <span className="when">{when(o)}</span>
            <small>{o.root}</small>
          </a>
          {((o.ready && o.ready.length > 0) || left(o.id) > 0) && (
            <p className="recent-ready">
              {o.ready && o.ready.length > 0 && <span>Ready for you</span>}
              {o.ready?.map((r) => (
                <a key={`e:${r.key}`} href={`#/o/${o.id}?p=${encodeURIComponent(r.key)}`} onClick={started}>
                  <Square ideaKey={r.key} name={r.name} />
                  {r.name}
                </a>
              ))}
              {left(o.id) > 0 && <span className="left">{left(o.id)} on the list</span>}
            </p>
          )}
        </div>
      ))}
    </nav>
  )
  return (
    <main className="ob solo">
      <div className="ob-l">
        {!back && <h1>kelve is set up.</h1>}
        {!back && <ul className="setup">
          <Tick done>{Named} is connected</Tick>
          {choice.hooks && (
            <Tick
              done={!!settings?.notesHook}
              act={settings?.notesHook ? <Quiet onClick={() => void hook(false)}>Turn off</Quiet> : <Button small onClick={() => void hook(true)}>Turn it on</Button>}
            >
              {settings?.notesHook ? 'It picks up your notes, and lessons as it works' : 'Let it pick up your notes, and lessons as it works'}
              {!settings?.notesHook && <small>One hook in its settings.</small>}
            </Tick>
          )}
          <Tick done={!!level} act={<Quiet onClick={() => { location.hash = '#/you/marks' }}>Change</Quiet>}>
            kelve marks {MARKS[level ?? 'learning']}
            <small>{level ? `You’re ${level.charAt(0).toUpperCase() + level.slice(1)}: New, Learning, Working or Deep.` : `For now. Ask ${named}: “${WRITE_MARKS}”`}</small>
          </Tick>
          {settings && <CountsTick settings={settings} failed={(e) => setSaid(e.message)} />}
        </ul>}
        {said && <p className="warn">{said}</p>}
        {back && recent}
        {settings && <JoinGraph settings={settings} />}
        {first ? (
          <div className="then">
            <p>{Named} opened <b>{first.title}</b>.</p>
            <Button onClick={started}>Open {first.title}</Button>
          </div>
        ) : (
          <div className="then">
            <p>{back ? `To open something else, ask ${named}:` : `Then ask ${named}:`}</p>
            <CopyWell text={SHOW_ME} words />
          </div>
        )}
        <ProblemsNotice />
        {!back && recent}
      </div>
    </main>
  )
}

export function Start({ me, opened }: { me?: Me; opened?: number }) {
  const settings = useLoad<Settings>('/api/settings', ['settings'])
  if (!me) return null
  return me.agent ? <Connected me={me} settings={settings.data} opened={opened} /> : <FirstRun me={me} settings={settings.data} />
}
