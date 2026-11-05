// Settings' sections (board Settings), but the account's, which signing in to
// kelve.ai brings: your agent, the agents connected, and notes; skill checks;
// reading, with what kelve marks for you; privacy; and about. A row for each choice, its line under it, the
// choice on the right.

import { type ReactNode, useEffect, useState } from 'react'
import { changed, del, post } from '../../api/client'
import type { AgentSettings, AgentSettingsIn, ChecksWhen, CountsView, DebriefsWhen, Highlighting, MarksView, Picked, Place, Problem, Settings, SettingsIn, Size, UpdateView } from '../../api/types'
import { EXAMPLE_CHARACTER, EXAMPLE_SETTINGS } from '../../example/agent'
import { hrefOf, type Section } from '../../routes'
import { getTheme, onTheme, setTheme, type Theme } from '../../theme'
import { Character, useMyCharacter } from '../../ui/Character'
import { Btn, CopyLine, Segmented, Toggle, useToast } from '../../ui/kit'
import { useLoad } from '../../useLoad'
import { agentKindName, roughly } from '../../words'
import { MarksEditor, writtenBy } from '../Marks'
import { useAgentLoad } from './looks'

/** One setting: what it is, a line on what it does, and the choice. */
function Opt({ title, what, stack, children, label }: { title: ReactNode; what?: ReactNode; stack?: boolean; children?: ReactNode; label?: string }) {
  return (
    <div className={`sopt${stack ? ' stack' : ''}`} aria-label={label}>
      <div className="t">
        <b>{title}</b>
        {what && <span>{what}</span>}
      </div>
      {children}
    </div>
  )
}

const DEBRIEFS: Array<{ value: DebriefsWhen; label: string; what: string }> = [
  { value: 'worthOne', label: 'For work worth one', what: 'Small changes get one line on home instead.' },
  { value: 'every', label: 'For every handover', what: 'Every time your agent hands work over.' },
  { value: 'onlyAsked', label: 'Only when I ask', what: 'Use Debrief it on home, or ask your agent.' },
]

const CHECKS: Array<{ value: ChecksWhen; label: string; what: string }> = [
  { value: 'ask', label: 'Ask me each time', what: 'kelve asks you on home before each one.' },
  { value: 'whenFree', label: 'When my agent is free', what: 'When your agent has nothing else to do.' },
  { value: 'never', label: 'Never', what: 'Parts of your skills stay not checked.' },
]

/** The settings for your agent and for checks, and a way to change them. */
function useAgentSettings(example?: boolean) {
  const loaded = useAgentLoad<AgentSettings>('/api/settings/agent', example ? EXAMPLE_SETTINGS : undefined, ['settings'])
  const [s, setS] = useState<AgentSettings>()
  const [said, setSaid] = useState<string>()
  useEffect(() => {
    if (loaded.data) setS(loaded.data)
  }, [loaded.data])
  const set = async (change: AgentSettingsIn): Promise<boolean> => {
    setSaid(undefined)
    if (example) {
      // The example answers itself: choices show, and go nowhere.
      setS((now) => (now ? { ...now, ...Object.fromEntries(Object.entries(change).filter(([k]) => k !== 'disconnect')), connected: now.connected.filter((a) => a.agent !== change.disconnect) } : now))
      return true
    }
    try {
      setS(await post<AgentSettings>('/api/settings/agent', change))
      changed('settings')
      return true
    } catch (e) {
      setSaid((e as Error).message)
      return false
    }
  }
  return { s, set, said, error: loaded.error }
}

function AgentSection({ example }: { example?: boolean }) {
  const { s, set, said } = useAgentSettings(example)
  const mine = useMyCharacter()
  const c = example ? EXAMPLE_CHARACTER : mine
  const [toast, say] = useToast()
  if (!s) return null
  const debriefs = DEBRIEFS.find((d) => d.value === s.debriefs) ?? DEBRIEFS[0]
  return (
    <>
      <div className="me">
        <Character size={88} move="wave" />
        <div className="me-r">
          <span className="nm">{c?.name ?? 'Your agent'}</span>
          <Btn size="small" href={hrefOf('agent/look', example)}>Change how it looks</Btn>
        </div>
      </div>
      {s.connected.map((a) => (
        <Opt key={a.agent} title={a.name} what={<span className="on"><i aria-hidden="true" />Connected</span>}>
          <Btn
            kind="quiet"
            size="small"
            label={`Disconnect ${a.name}`}
            onClick={() => void set({ disconnect: a.agent }).then((ok) => ok && say(`kelve’s hook is out of ${a.name}’s settings.`))}
          >
            Disconnect
          </Btn>
        </Opt>
      ))}
      {s.connected.length === 0 && <Opt title="No agent is connected" what="kelve needs one to do anything. Connect it below." />}
      <Opt stack title="Debriefs" what={debriefs.what}>
        <Segmented label="Debriefs" options={DEBRIEFS} value={s.debriefs} onChange={(debriefs) => void set({ debriefs })} />
      </Opt>
      <Opt title="Put your skills in each agent’s skills folder" what="So they work even when kelve isn’t running.">
        <Toggle label="Put your skills in each agent’s skills folder" on={s.skillsFolder} onChange={(skillsFolder) => void set({ skillsFolder })} />
      </Opt>
      <Opt stack title="Connect another agent" what="Run this in your terminal.">
        <CopyLine text="kelve setup" />
      </Opt>
      <NotesOpts example={example} />
      {said && <p className="warn" role="alert">{said}</p>}
      {toast}
    </>
  )
}

/** Where your own notes are kept, and who a new note is for. */
function NotesOpts({ example }: { example?: boolean }) {
  const { s, set, said } = useSettings(example)
  const [failed, setFailed] = useState<string>()
  const agent = agentKindName(s.agent)
  const choose = async () => {
    setFailed(undefined)
    if (example) return
    try {
      const { path } = await post<Picked>('/api/pick', { what: 'folder' })
      if (path) await set({ notesDir: path })
    } catch (e) {
      setFailed((e as Error).message)
    }
  }
  const show = () => {
    setFailed(undefined)
    if (!example) void post('/api/notes/show').catch((e: Error) => setFailed(e.message))
  }
  const places: Array<{ value: Place; label: string }> = [
    { value: 'you', label: 'Just you' },
    { value: 'work', label: 'With the work' },
    { value: 'agent', label: `For ${agent}` },
  ]
  return (
    <>
      <Opt stack title="Your own notes" what={<>Kept in <span className="mono">{s.notesDir ?? '~/Notes'}</span>{s.notesChosen ? '' : ', kelve’s own folder'}, a folder for each project. Changing it moves the notes you have. Notes with the work are kept in the project’s .kelve/notes.</>}>
        <div className="row">
          <Btn size="small" onClick={() => void choose()} disabled={example}>Choose a folder</Btn>
          <Btn kind="quiet" size="small" onClick={show} disabled={example}>Show the folder</Btn>
          {s.notesChosen && <Btn kind="quiet" size="small" onClick={() => void set({ notesDir: '' })}>Use kelve’s own folder</Btn>}
        </div>
      </Opt>
      <Opt stack title="A new note is">
        <Segmented label="Who a new note is for" value={s.newNote ?? 'you'} onChange={(newNote) => void set({ newNote })} options={places} />
      </Opt>
      {(said || failed) && <p className="warn" role="alert">{said ?? failed}</p>}
    </>
  )
}

function ChecksSection({ example }: { example?: boolean }) {
  const { s, set, said } = useAgentSettings(example)
  if (!s) return null
  const when = CHECKS.find((c) => c.value === s.checks) ?? CHECKS[1]
  return (
    <>
      <p className="lede">A check redoes a past task with and without part of a skill.</p>
      <Opt stack title="When to run checks" what={when.what}>
        <Segmented label="When to run checks" options={CHECKS} value={s.checks} onChange={(checks) => void set({ checks })} />
      </Opt>
      {s.checks !== 'never' && (
        <Opt title="Runs a week, at most" what={`${s.checksThisWeek} of ${s.checksAWeek} used this week.`}>
          <Segmented
            label="Runs a week, at most"
            options={['5', '10', '20'].map((n) => ({ value: n, label: n }))}
            value={String(s.checksAWeek)}
            onChange={(n) => void set({ checksAWeek: Number(n) })}
          />
        </Opt>
      )}
      {said && <p className="warn" role="alert">{said}</p>}
    </>
  )
}

/** The person's settings as kelve keeps them, and a way to change them. */
function useSettings(example?: boolean) {
  const loaded = useLoad<Settings>(example ? null : '/api/settings', ['settings'])
  const [s, setS] = useState<Partial<Settings>>(example ? { textSize: 'medium', codeSize: 'medium', highlighting: 'quiet', shareCounts: true, countsOff: null } : {})
  const [said, setSaid] = useState<string>()
  useEffect(() => {
    if (loaded.data) setS(loaded.data)
  }, [loaded.data])
  const set = async (change: SettingsIn) => {
    setSaid(undefined)
    if (example) {
      setS((now) => ({ ...now, ...change }))
      return
    }
    try {
      setS(await post<Settings>('/api/settings', change))
      changed('settings')
    } catch (e) {
      setSaid((e as Error).message)
    }
  }
  return { s, set, said }
}

const SIZES: Array<{ value: Size; label: string }> = [
  { value: 'small', label: 'Small' },
  { value: 'medium', label: 'Medium' },
  { value: 'large', label: 'Large' },
]

/** A few lines of code and a sentence, drawn as the choices below draw them. */
function Preview() {
  return (
    <div className="spv" aria-label="How it looks">
      <pre className="spv-code">
        <code>
          <span className="c-com">{'// Wait longer after each failed try.'}</span>{'\n'}
          <span className="c-kw">fn</span> <span className="c-def">backoff</span>(attempt: <span className="c-ty">u32</span>) -&gt; <span className="c-ty">Duration</span> {'{'}{'\n'}
          {'    '}<span className="c-kw">let</span> ms = <span className="c-num">250</span> * <span className="c-num">2u64</span>.<span className="c-call">pow</span>(attempt);{'\n'}
          {'    '}log::<span className="c-call">debug!</span>(<span className="c-str">"waiting {'{ms}'} ms"</span>);{'\n'}
          {'    '}<span className="c-ty">Duration</span>::<span className="c-call">from_millis</span>(ms.<span className="c-call">min</span>(<span className="c-num">8_000</span>)){'\n'}
          {'}'}
        </code>
      </pre>
      <p>It waits longer after each failed try, so a busy server has room to recover.</p>
    </div>
  )
}

function ReadingSection({ example }: { example?: boolean }) {
  const { s, set, said } = useSettings(example)
  const marks = useLoad<MarksView>(example ? null : '/api/marks', ['spec'])
  const [theme, setShown] = useState<Theme>(getTheme)
  const [editing, setEditing] = useState(false)
  useEffect(() => onTheme(() => setShown(getTheme())), [])
  const known = example ? ['Exponential backoff', 'Result and the ? operator', 'async and await'] : (marks.data?.spec.never ?? [])
  const forget = async (idea: string) => {
    if (example || !marks.data) return
    await post<MarksView>('/api/marks', { ...marks.data.spec, never: marks.data.spec.never.filter((i) => i !== idea) }).then(() => changed('spec'), () => {})
  }
  return (
    <>
      <Preview />
      <Opt title="Theme">
        <Segmented label="Theme" value={theme} onChange={setTheme} options={[{ value: 'light', label: 'Light' }, { value: 'dark', label: 'Dark' }, { value: 'system', label: 'Your system’s' }]} />
      </Opt>
      <Opt title="Text size">
        <Segmented label="Text size" value={s.textSize ?? 'medium'} onChange={(size) => void set({ textSize: size, codeSize: size })} options={SIZES} />
      </Opt>
      <Opt title="Code colours">
        <Segmented<Highlighting> label="Code colours" value={s.highlighting ?? 'quiet'} onChange={(highlighting) => void set({ highlighting })} options={[{ value: 'full', label: 'Full' }, { value: 'quiet', label: 'Quiet' }, { value: 'none', label: 'None' }]} />
      </Opt>
      <Opt stack title="Ideas you know" what="kelve doesn’t mark these as new to you.">
        {known.length > 0 ? (
          <ul className="sideas">
            {known.map((i) => (
              <li key={i}>
                <span>{i}</span>
                <Btn kind="quiet" size="small" label={`Take out ${i}`} onClick={() => void forget(i)}>Take out</Btn>
              </li>
            ))}
          </ul>
        ) : (
          <p className="snone">None yet. Press “I know this” on an idea in a debrief.</p>
        )}
        <div className="row"><Btn size="small" pressed={editing} onClick={() => setEditing(!editing)} disabled={example || !marks.data}>{editing ? 'Done' : 'Edit marks.md'}</Btn></div>
      </Opt>
      {editing && marks.data && (
        <>
          <p className="lede">{writtenBy(marks.data, agentKindName(s.agent))} Kept in <span className="mono">{marks.data.file}</span>, so you or your agent can change it there too.</p>
          <MarksEditor view={marks.data} />
        </>
      )}
      {said && <p className="warn" role="alert">{said}</p>}
    </>
  )
}

function PrivacySection({ example }: { example?: boolean }) {
  const { s, set, said } = useSettings(example)
  const counts = useLoad<CountsView>(example ? null : '/api/counts', ['settings'])
  const problems = useLoad<Problem[]>(example ? null : '/api/problems', ['settings'])
  const [open, setOpen] = useState(false)
  const [report, setReport] = useState(false)
  const [sure, setSure] = useState(false)
  const [failed, setFailed] = useState<string>()
  const problem = problems.data?.[0]
  const everything = () =>
    void del('/api/everything').then(
      () => {
        changed()
        location.hash = '#/'
      },
      (e: Error) => setFailed(e.message),
    )
  return (
    <>
      <ul className="swho">
        <li><b>Your agent</b> gets your work, with secrets taken out.</li>
        <li><b>kelve.ai</b> gets your skills only for a verified run you ask for, and your page if you publish it.</li>
        <li><b>No one else</b> gets anything.</li>
      </ul>
      <p className="slinks">
        Read <a className="go" href="https://kelve.ai/privacy" target="_blank" rel="noopener">the privacy policy</a> and{' '}
        <a className="go" href="https://kelve.ai/terms" target="_blank" rel="noopener">the terms</a>.
      </p>
      <Opt
        title="Share anonymous counts"
        what={
          <>
            {s.countsOff ? `${s.countsOff === 'DO_NOT_TRACK' ? 'DO_NOT_TRACK is set' : 'KELVE_TELEMETRY is off'}, so nothing is sent.` : 'Numbers only, once a week. Never your work or your skills.'}
            {counts.data && (
              <>
                <button type="button" className="btn btn-g btn-sm sreport-b" aria-expanded={open} onClick={() => setOpen(!open)}>See what it sends</button>
                {open && <pre className="sreport">{counts.data.thisWeek}</pre>}
              </>
            )}
          </>
        }
      >
        <Toggle label="Share anonymous counts" on={!!s.shareCounts && !s.countsOff} disabled={!!s.countsOff} onChange={(shareCounts) => void set({ shareCounts, hintSeen: 'counts' })} />
      </Opt>
      {problem && (
        <Opt
          title={`${problem.what === 'the page' ? 'Something went wrong on the page' : 'kelve stopped unexpectedly'} ${roughly(problem.at)}`}
          what={
            <>
              Its report holds nothing of your work. Post it if you’d like it fixed.
              <button type="button" className="btn btn-g btn-sm sreport-b" aria-expanded={report} onClick={() => setReport(!report)}>See what it says</button>
              {report && <pre className="sreport">{problem.text}</pre>}
            </>
          }
        >
          <div className="row">
            <Btn size="small" href={problem.issue}>Report it on GitHub</Btn>
            <Btn kind="quiet" size="small" onClick={() => void del(`/api/problems/${encodeURIComponent(problem.id)}`).then(problems.reload, () => {})}>Forget it</Btn>
          </div>
        </Opt>
      )}
      <Opt title="Delete everything" what={sure ? 'It can’t be undone. Your skills and notes stay.' : 'Everything kelve keeps. Your skills and notes stay.'}>
        {sure ? (
          <div className="row">
            <Btn size="small" onClick={example ? () => setSure(false) : everything}><span className="danger">Delete everything</span></Btn>
            <Btn kind="quiet" size="small" onClick={() => setSure(false)}>Keep it</Btn>
          </div>
        ) : (
          <Btn size="small" onClick={() => setSure(true)}>Delete</Btn>
        )}
      </Opt>
      {(said || failed) && <p className="warn" role="alert">{said ?? failed}</p>}
    </>
  )
}

function AboutSection({ example }: { example?: boolean }) {
  const update = useLoad<UpdateView>(example ? null : '/api/update', ['settings'])
  const u = example ? { version: '0.2.1', newer: null } : update.data
  return (
    <>
      <p className="lede">kelve is open source, under the Apache License 2.0.</p>
      {u &&
        (u.newer ? (
          <Opt stack title={`kelve ${u.version}`} what={`${u.newer.version} is out. Run this, then restart your agent.`} label="A newer kelve">
            <CopyLine text="kelve update" />
          </Opt>
        ) : (
          <Opt title={`kelve ${u.version}`} what="Up to date." />
        ))}
      <Opt title="Docs"><a className="go" href="https://kelve.ai/docs" target="_blank" rel="noopener">kelve.ai/docs</a></Opt>
      <Opt title="Source"><a className="go" href="https://github.com/kelve-ai/kelve" target="_blank" rel="noopener">github.com/kelve-ai/kelve</a></Opt>
    </>
  )
}

/** A section of settings, but the account's. */
export function SettingsSection({ section, example }: { section: Exclude<Section, 'account'>; example?: boolean }) {
  switch (section) {
    case 'agent':
      return <AgentSection example={example} />
    case 'checks':
      return <ChecksSection example={example} />
    case 'reading':
      return <ReadingSection example={example} />
    case 'privacy':
      return <PrivacySection example={example} />
    case 'about':
      return <AboutSection example={example} />
  }
}
