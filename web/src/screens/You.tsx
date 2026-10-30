// Settings (boards O-6 to O-9, N-6): a list on the left, a page on the right,
// at #/you/<section>. Your agent, the shared graph, reading, what kelve marks,
// notes, and what's shared. Each choice says what it does: a row each, its line
// under it, the choice in plain words on the right (board L-9).

import { type ReactNode, useEffect, useState } from 'react'
import { changed, del, post } from '../api/client'
import type { Added, AgentSeen, CountsView, GraphAccount, GraphSignIn, LessonsSettings, Me, Picked, SendsHeld, SentToday, Settings, SettingsIn, Size, UpdateView } from '../api/types'
import { Loading } from '../Loader'
import { getTheme, onTheme, setTheme, type Theme } from '../theme'
import { Button, Choice, CopyWell, Quiet, Switch } from '../ui/ui'
import { useLoad } from '../useLoad'
import { agentKindName, agentName, ago, and } from '../words'
import { AGENTS, agentChoice, INSTALLED } from './agents'
import { holdWords, LearnedItem, LessonSquare, standing } from '../reader/Lessons'
import { Rich } from '../rich'
import { MarksSection } from './Marks'
import { type Provider, SignInButtons } from './SignIn'
import { ProblemsSection } from './Problems'

const SECTIONS: Array<[string, string]> = [
  ['agent', 'Your agent'],
  ['graph', 'The shared graph'],
  ['lessons', 'Lessons'],
  ['reading', 'Reading'],
  ['marks', 'What kelve marks'],
  ['notes', 'Notes'],
  ['shared', 'What’s shared'],
]

/** One setting: what it is, a line on what it does, and the choice. */
function Opt({ title, what, children }: { title: string; what?: ReactNode; children?: ReactNode }) {
  return (
    <div className="opt">
      <div>
        <b>{title}</b>
        {what && <span>{what}</span>}
      </div>
      {children}
    </div>
  )
}

type Set = (patch: SettingsIn) => Promise<void>

/** A newer kelve, when one is out: what it brings, and the two ways to update to it. Nothing when kelve is up to date. */
function UpdateLine({ agent }: { agent: string }) {
  const update = useLoad<UpdateView>('/api/update', ['settings'])
  const u = update.data
  if (!u?.newer) return null
  return (
    <div className="opt stack" aria-label="A newer kelve">
      <div>
        <b>{`kelve ${u.newer.version} is out.`}{u.newer.line && ` ${u.newer.line}`}</b>
        <span>{`You have ${u.version}. Ask ${agent} to update it, or run the command.`}</span>
      </div>
      <CopyWell words text="Update kelve" />
      <CopyWell text="kelve update" />
    </div>
  )
}

/** Each agent seen lately, once, by the name people know it by: the latest first. */
function seenLately(me?: Me): AgentSeen[] {
  const seen = new Set<string>()
  return (me?.agents ?? []).filter((a) => {
    const name = agentName(a.name)
    if (seen.has(name)) return false
    seen.add(name)
    return true
  })
}

function AgentSection({ me, s, set }: { me?: Me; s: Settings; set: Set }) {
  const agents = seenLately(me)
  const choice = agentChoice(s.agent)
  const said = (a: AgentSeen) => `${a.model ? `, on ${a.model}` : ''}. It last asked kelve for something ${ago(a.lastSeenAt)}.`
  const dots = me?.found.length ? 'A dot: installed here. ' : ''
  // Other agents with kelve's hook in, so none is a surprise.
  const hookIn = (too: boolean) => (s.hooksElsewhere.length ? `kelve’s hook is in ${and(s.hooksElsewhere)}’s settings${too ? ' too' : ''}.` : '')
  const elsewhere = s.hooksElsewhere.length ? ` ${hookIn(true)}` : ''
  return (
    <>
      {agents.length <= 1 ? (
        <p className="d">
          {agents[0] ? `${agentName(agents[0].name)} is connected${said(agents[0])}` : 'No agent has connected yet.'}
        </p>
      ) : (
        <>
          <p className="d">These agents asked kelve for something lately:</p>
          <ul className="who-gets" aria-label="Agents connected lately">
            {agents.map((a) => <li key={a.name}><b>{agentName(a.name)}</b>{said(a)}</li>)}
          </ul>
        </>
      )}
      <UpdateLine agent={agentKindName(s.agent)} />
      <Opt title="Your agent" what={`${dots}Named in what kelve says, and in the line that connects it.`}>
        <Choice small label="Your agent" value={s.agent} onChange={(agent) => void set({ agent })} options={AGENTS.map((x) => ({ value: x.kind, label: x.name, dot: me?.found.includes(x.kind) ? INSTALLED : undefined }))} />
      </Opt>
      <div className="opt stack">
        <div><b>Connect it</b><span>{choice.how}</span></div>
        <CopyWell text={choice.line} />
      </div>
      {choice.hooks && (
        <Opt
          title={`Let ${choice.name} pick up your notes, and lessons`}
          what={`When you leave a note for ${choice.name} in kelve, it’s told the next time you message it. As it reads a file, or something fails, it’s given what other agents learned that helps then, from the shared graph. Through one small hook kelve adds to its settings.${elsewhere}`}
        >
          <Switch label={`Let ${choice.name} pick up your notes, and lessons`} on={s.notesHook} onChange={(notesHook) => void set({ notesHook })} />
        </Opt>
      )}
      {!choice.hooks && s.hooksElsewhere.length > 0 && <Opt title="kelve’s hook" what={hookIn(false)} />}
      <Opt title="Who writes entries" what={`${agentKindName(s.agent).replace(/^./, (c) => c.toUpperCase())} writes the entries the shared graph doesn’t have yet, with your model and your plan. kelve has no model of its own.`} />
    </>
  )
}

/** What you've added, and how many times others read it. */
function AddedLine() {
  const added = useLoad<Added>('/api/graph/added', ['settings'])
  const a = added.data
  if (!a) return null
  return (
    <div className="big3" aria-label="What you’ve added">
      <div><b>{a.entries.toLocaleString('en-GB')}</b><span>{a.entries === 1 ? 'entry' : 'entries'}</span></div>
      <div><b>{a.relations.toLocaleString('en-GB')}</b><span>{a.relations === 1 ? 'way ideas relate' : 'ways ideas relate'}</span></div>
      <div><b>{a.reads.toLocaleString('en-GB')}</b><span>{a.reads === 1 ? 'time read by others' : 'times read by others'}</span></div>
    </div>
  )
}

/** When the day's limits reset, in your own time: `00:00 tomorrow`. */
function resetsAt(until: string): string {
  const at = new Date(until)
  const time = at.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' })
  const day = new Date()
  if (at.toDateString() === day.toDateString()) return `${time} today`
  day.setDate(day.getDate() + 1)
  if (at.toDateString() === day.toDateString()) return `${time} tomorrow`
  return `${time} on ${at.toLocaleDateString('en-GB', { weekday: 'long' })}`
}

/** Why nothing more goes to the graph today, and when it does. */
function heldLine(h: SendsHeld): string {
  const what = h.what === 'ideas' ? 'as many ideas' : h.what === 'entries' ? 'as many entries' : h.what === 'relations' ? 'as many ways ideas relate' : 'as much'
  const rest = `The rest goes at ${resetsAt(h.until)}.`
  return h.newAccount
    ? `You’ve added ${what} as a new account can today. ${rest} The limit rises once ten of your entries are accepted, or after a week.`
    : `You’ve added ${what} as you can today. ${rest}`
}

/** The entries sent today, by how the graph's check of each stands. */
function todayLine(t: SentToday): string {
  if (t.sent === 0) return 'None yet.'
  const how = ([[t.accepted, 'accepted'], [t.checking, 'being checked'], [t.turnedAway, 'turned away']] as const)
    .filter(([n]) => n > 0)
    .map(([n, w]) => `${n.toLocaleString('en-GB')} ${w}`)
  return `${t.sent.toLocaleString('en-GB')} ${t.sent === 1 ? 'entry' : 'entries'}: ${how.join(', ')}.`
}

/** What went to the graph today, and, once today's limit is reached, when the rest goes. */
function TodayLine() {
  const today = useLoad<SentToday>('/api/graph/today', ['settings'])
  const t = today.data
  if (!t) return null
  return (
    <div className="opt stack" aria-label="Sent today">
      <div>
        <b>Sent today</b>
        <span>{todayLine(t)}</span>
        {t.held && <span className="held">{heldLine(t.held)}</span>}
      </div>
    </div>
  )
}

function GraphSection() {
  const account = useLoad<GraphAccount>('/api/graph/account', ['settings'])
  const [said, setSaid] = useState<string>()
  const [deleting, setDeleting] = useState(false)
  const a = account.data
  const signIn = (provider: Provider) =>
    post<GraphSignIn>('/api/graph/signin', { provider }).then(
      (x) => {
        window.open(x.address, '_blank', 'noopener')
        account.reload()
      },
      (e: Error) => setSaid(e.message),
    )
  const signedIn = a?.from === 'app'
  return (
    <>
      <p className="d">Every entry and lesson you read from the graph, someone’s agent wrote and the graph checked. It’s free for anyone signed in; in return, what your agent writes and learns is added too. Never your work. Signed out, kelve works without it, and your agent writes every entry for you alone.</p>
      {!a ? null : !a.graph ? (
        <Opt title="No shared graph" what="This kelve has no shared graph set." />
      ) : a.from === 'env' ? (
        <>
          <Opt title="Signed in with a key" what="kelve adds to the shared graph with the key in KELVE_GRAPH_KEY, so there’s no signing in here." />
          <TodayLine />
        </>
      ) : signedIn ? (
        <>
          <Opt title={`Signed in as ${a.handle}`} what="The name you’re credited as, which the graph made up. It never shows your own name.">
            <Quiet onClick={() => void del('/api/graph/account').then(account.reload, (e: Error) => setSaid(e.message))}>Sign out</Quiet>
          </Opt>
          <div className="opt stack">
            <div><b>What you’ve added</b></div>
            <AddedLine />
          </div>
          <TodayLine />
          <Opt title="The terms" what={a.agreed ? 'You agreed to them as you signed in.' : 'They changed since you signed in, so nothing is added until you agree.'}>
            <a className="quiet-link" href="#/adding">{a.agreed ? 'Read them' : 'Read and agree'}</a>
          </Opt>
          <Opt title="Delete your account" what={deleting ? `The graph forgets the account you signed in with. What others can read already stays, under ${a.handle}, which no longer leads to you. It can’t be undone.` : 'On the shared graph.'}>
            {deleting ? (
              <span className="row">
                <Quiet onClick={() => void post('/api/graph/account/delete').then(() => { setDeleting(false); account.reload() }, (e: Error) => setSaid(e.message))}>Delete it</Quiet>
                <Quiet onClick={() => setDeleting(false)}>Keep it</Quiet>
              </span>
            ) : (
              <Quiet onClick={() => setDeleting(true)}>Delete my account</Quiet>
            )}
          </Opt>
        </>
      ) : (
        <Opt
          title="Join the shared graph"
          what={a.waiting ? 'Finish signing in, in the browser tab that opened.' : 'Sign in, and what your agent writes and learns is added for the next person. GitHub or Google only tells the graph who you are: it keeps a number for your account, never your name or email.'}
        >
          {!a.waiting && <SignInButtons small signIn={(p) => void signIn(p)} />}
        </Opt>
      )}
      {said && <p className="warn">{said}</p>}
    </>
  )
}

const SIZES: Array<{ value: Size; label: string }> = [
  { value: 'small', label: 'Small' },
  { value: 'medium', label: 'Medium' },
  { value: 'large', label: 'Large' },
]

function ReadingSection({ s, set }: { s: Settings; set: Set }) {
  const [theme, setShown] = useState<Theme>(getTheme)
  useEffect(() => onTheme(() => setShown(getTheme())), [])
  return (
    <>
      <p className="d">How code and text look, and where what’s read is kept.</p>
      <p className="lgroup"><b>How it looks</b></p>
      <Opt title="Theme" what="Kept in this browser.">
        <Choice small label="Theme" value={theme} onChange={setTheme} options={[{ value: 'light', label: 'Light' }, { value: 'dark', label: 'Dark' }, { value: 'system', label: 'Your system’s' }]} />
      </Opt>
      <Opt title="Code size">
        <Choice small label="Code size" value={s.codeSize} onChange={(codeSize) => void set({ codeSize })} options={SIZES} />
      </Opt>
      <Opt title="Text size" what="Entries, notes, and text files.">
        <Choice small label="Text size" value={s.textSize} onChange={(textSize) => void set({ textSize })} options={SIZES} />
      </Opt>
      <Opt title="Highlighting" what={s.highlighting === 'full' ? 'Full: keywords, types, properties and calls have colours too, as in an editor.' : s.highlighting === 'none' ? 'None: code in one colour.' : 'Quiet: only strings, constants, comments and the names of what’s defined have colours.'}>
        <Choice small label="Highlighting" value={s.highlighting} onChange={(highlighting) => void set({ highlighting })} options={[{ value: 'full', label: 'Full' }, { value: 'quiet', label: 'Quiet' }, { value: 'none', label: 'None' }]} />
      </Opt>
      <p className="lgroup"><b>What’s read</b></p>
      <Opt title="Keep what’s read in the repo" what="In .kelve/read, committed with the work, so no one on it reads a file twice. Off, it’s kept by kelve alone.">
        <Switch label="Keep what’s read in the repo" on={s.keepReadInRepo} onChange={(keepReadInRepo) => void set({ keepReadInRepo })} />
      </Opt>
    </>
  )
}

function NotesSection({ s, set }: { s: Settings; set: Set }) {
  const [said, setSaid] = useState<string>()
  const choose = async () => {
    setSaid(undefined)
    try {
      const { path } = await post<Picked>('/api/pick', { what: 'folder' })
      if (path) await set({ notesDir: path })
    } catch (e) {
      setSaid((e as Error).message)
    }
  }
  const agent = agentKindName(s.agent)
  return (
    <>
      <p className="d">Notes are plain markdown files, so any notes app or editor can open them too. Put them with your other notes, in an Obsidian vault for example.</p>
      <div className="opt stack">
        <div>
          <b>Your own notes</b>
          <span>Kept in <span className="mono">{s.notesDir}</span>{s.notesChosen ? '' : ', kelve’s own folder'}, a folder for each project. Changing it moves the notes you have.</span>
        </div>
        <span className="row">
          <Button small onClick={() => void choose()}>Choose a folder</Button>
          <Quiet onClick={() => void post('/api/notes/show').catch((e: Error) => setSaid(e.message))}>Show the folder</Quiet>
          {s.notesChosen && <Quiet onClick={() => void set({ notesDir: '' })}>Use kelve’s own folder</Quiet>}
        </span>
      </div>
      <Opt title="Notes with the work" what="Kept in the project’s .kelve/notes, and committed with it, for everyone on the work." />
      <Opt title={`Notes for ${agent}`} what={`Kept with your own notes until ${agent} has done them.`} />
      <Opt title="A new note is">
        <Choice small label="Who a new note is for" value={s.newNote} onChange={(newNote) => void set({ newNote })} options={[{ value: 'you', label: 'Just you' }, { value: 'work', label: 'With the work' }, { value: 'agent', label: `For ${agent}` }]} />
      </Opt>
      {said && <p className="warn">{said}</p>}
    </>
  )
}

/** Lessons: what your agent learned, held a day (or as kelve's told) so you can stop it, and what it was given. */
function LessonsSection({ s }: { s: Settings }) {
  const lessons = useLoad<LessonsSettings>('/api/lessons', ['lessons', 'settings'])
  const Agent = agentKindName(s.agent).replace(/^./, (c) => c.toUpperCase())
  const l = lessons.data
  return (
    <>
      <p className="d">
        {`What ${Agent} learns as it works, when something was hard to get past or it found a better way, goes to the shared graph ${l?.hold === 0 ? 'as soon as it’s written' : `${holdWords(l?.hold ?? 86_400)} after it’s written, so you can stop it first`}. Only the lesson goes, never your work.`}
      </p>
      {!l ? <Loading error={lessons.error} /> : (
        <>
          <p className="lgroup"><b>What {Agent} learned</b><span>{l.learned.length || ''}</span></p>
          {l.learned.length ? (
            <ul className="rules set-rules">{l.learned.map((x) => <LearnedItem key={x.id} l={x} stopped={() => lessons.reload()} />)}</ul>
          ) : <p className="d">Nothing yet. Most work has no lesson in it.</p>}
          <p className="lgroup"><b>Lessons {Agent} was given</b><span>{l.given.length || ''}</span></p>
          {l.given.length ? (
            <ul className="rules set-rules">
              {l.given.map((g) => (
                <li key={g.id} className="lli"><LessonSquare /><span><Rich text={g.do} /></span><span className="lq">{g.used.find((u) => u.outcome !== 'unknown')?.line ?? standing(g)}</span></li>
              ))}
            </ul>
          ) : <p className="d">None yet. They come as {Agent} reads files kelve has read, or as something fails.</p>}
        </>
      )}
    </>
  )
}

function SharedSection({ s, set }: { s: Settings; set: Set }) {
  const counts = useLoad<CountsView>('/api/counts', ['settings', 'opened', 'notes'])
  const [sure, setSure] = useState(false)
  const [said, setSaid] = useState<string>()
  const agent = agentKindName(s.agent)
  return (
    <>
      <ul className="who-gets">
        <li><b>{agent.replace(/^./, (c) => c.toUpperCase())}</b> gets the files you open, with secrets taken out, to read them for you.</li>
        <li><b>The shared graph</b> gets the names of ideas to look up and, when you add, entries, how ideas relate, and lessons your agent learned, with whether those it was given held. Never your code, file names, paths or notes, or which ideas your files use.</li>
        <li><b>Which ways down people take:</b> a count for each relation followed, with nothing about you attached.</li>
        <li><b>No one else</b> gets anything.</li>
      </ul>
      <p className="d">
        What the shared graph keeps about you, and your rights:{' '}
        <a className="l" href="https://kelve.ai/privacy" target="_blank" rel="noopener">the privacy policy</a>. What you agree to:{' '}
        <a className="l" href="https://kelve.ai/terms" target="_blank" rel="noopener">the terms</a>.
      </p>
      <Opt
        title="Share anonymous counts"
        what={
          s.countsOff
            ? `${s.countsOff === 'DO_NOT_TRACK' ? 'DO_NOT_TRACK is set' : 'KELVE_TELEMETRY is off'}, so nothing is sent, whatever you choose here.`
            : 'Once a week, numbers only: how many things you opened, entries you read and how many were already written, links you followed, notes you wrote, jobs your agent did, how lessons went, and kelve’s version. With them goes a random ID, made when you say yes and deleted when you stop, so people coming back can be counted. Never your work, your name or your account.'
        }
      >
        <Switch label="Share anonymous counts" on={s.shareCounts && !s.countsOff} onChange={(shareCounts) => void set({ shareCounts, hintSeen: 'counts' })} />
      </Opt>
      {counts.data && (
        <details className="report-d">
          <summary>See exactly what this week’s says so far</summary>
          <pre className="report">{counts.data.thisWeek}</pre>
        </details>
      )}
      <ProblemsSection />
      <Opt title="Delete everything" what={sure ? 'Your settings, what kelve keeps for what you’ve opened, and what’s kept for your agent. Your notes, and what’s read in each project, are your own files, and stay. It can’t be undone.' : 'Everything kelve keeps, but your own files.'}>
        {sure ? (
          <span className="row">
            <Quiet onClick={() => void del('/api/everything').then(() => { changed(); location.hash = '#/' }, (e: Error) => setSaid(e.message))}>Delete everything</Quiet>
            <Quiet onClick={() => setSure(false)}>Keep it</Quiet>
          </span>
        ) : (
          <Quiet onClick={() => setSure(true)}>Delete everything</Quiet>
        )}
      </Opt>
      {said && <p className="warn">{said}</p>}
    </>
  )
}

export function You({ focus, me }: { focus?: string; me?: Me }) {
  const settings = useLoad<Settings>('/api/settings', ['settings'])
  const [s, setS] = useState<Settings>()
  const [failed, setFailed] = useState<string>()
  useEffect(() => {
    if (settings.data) setS(settings.data)
  }, [settings.data])
  if (!s) return <Loading error={settings.error} />
  const here = SECTIONS.some(([id]) => id === focus) ? focus! : 'agent'
  const set: Set = (patch) => {
    setFailed(undefined)
    return post<Settings>('/api/settings', patch).then(
      (n) => {
        setS(n)
        changed('settings')
      },
      (e: Error) => setFailed(e.message),
    )
  }
  const title = SECTIONS.find(([id]) => id === here)![1]
  return (
    <main className="set">
      <nav aria-label="Settings">
        <h1 className="sr-only">Settings</h1>
        {SECTIONS.map(([id, name]) => (
          <a key={id} href={`#/you/${id}`} className={id === here ? 'on' : undefined} aria-current={id === here ? 'page' : undefined}>{name}</a>
        ))}
      </nav>
      <section className="set-body" aria-label={title}>
        <h2>{title}</h2>
        {here === 'agent' && <AgentSection me={me} s={s} set={set} />}
        {here === 'graph' && <GraphSection />}
        {here === 'lessons' && <LessonsSection s={s} />}
        {here === 'reading' && <ReadingSection s={s} set={set} />}
        {here === 'marks' && <MarksSection agent={agentKindName(s.agent)} />}
        {here === 'notes' && <NotesSection s={s} set={set} />}
        {here === 'shared' && <SharedSection s={s} set={set} />}
        {failed && <p className="warn">{failed}</p>}
      </section>
    </main>
  )
}
