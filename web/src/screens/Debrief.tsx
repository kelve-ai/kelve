// A debrief (boards Debrief, DebriefSmall, DebriefLarge, DebriefWriting,
// DebriefQueued and DebriefSent): what your agent did, how it works and what
// it decided, each decision with your call on it, what the work rests on,
// every file, and what happens when you send it. The margin beside each
// part says where it stands. While it's written, what your agent is doing;
// once it's sent, what happens next.

import { useEffect, useMemo, useState } from 'react'
import type { DebriefLine, DebriefView, DecisionChoice, DecisionStatus, DecisionView, IdeaView, SendLine } from '../api/types'
import { IF_NOT_STARTED, START_NOW } from '../agentList'
import { post } from '../api/client'
import { colours, highlight } from '../ui/highlight'
import { agreeRestIn, decideIn, exampleDebrief, explainIn, knowIn, lookIn, rememberIn, sendIn } from '../example/debriefs'
import { hrefOf } from '../routes'
import { useLoad } from '../useLoad'
import { Character } from '../ui/Character'
import { FileTree } from '../ui/FileTree'
import { Spinner, useToast } from '../ui/kit'
import { Markdown } from '../ui/markdown'

const STATUS: Record<DecisionStatus, string> = {
  yourCall: 'Your call',
  agreed: 'Agreed',
  redoing: 'Redoing',
  researching: 'Researching',
  researchReady: 'Research ready',
}

const params = () => new URLSearchParams(location.hash.split('?')[1] ?? '')

/** Lines added and taken out, as the page counts them: +293 −8. */
const plus = (n: number) => (n ? `+${n.toLocaleString('en-GB')}` : '')
const minus = (n: number) => (n ? `−${n.toLocaleString('en-GB')}` : '')
const filesSaid = (n: number) => (n === 1 ? '1 file' : `${n.toLocaleString('en-GB')} files`)

/**
 * What the page does with what's in it: through kelve, or, in the example,
 * on the page alone.
 */
type Act = {
  decide: (n: number, choice: DecisionChoice | null, note?: string) => Promise<void>
  remember: (n: number, on: boolean) => Promise<void>
  look: (n: number, question: string) => Promise<void>
  agreeRest: () => Promise<void>
  explain: (idea: string) => Promise<void>
  know: (idea: string) => Promise<void>
  send: () => Promise<void>
  again: () => Promise<void>
}

export function Debrief({ id, example }: { id: string; example?: boolean }) {
  const local = example || id === 'example'
  const loaded = useLoad<DebriefView>(local ? null : `/api/debriefs/${id}`, ['debriefs', 'spec'])
  const [mock, setMock] = useState<DebriefView | null>(() => (local ? exampleDebrief(id, params().get('as')) : null))
  const [mine, setMine] = useState<DebriefView | null>(null)
  const [justSent, setJustSent] = useState(params().has('sent'))
  const [toast, say] = useToast(3200)
  // What kelve said last, until what's loaded is newer.
  useEffect(() => setMine(null), [loaded.data])
  const v = local ? mock : (mine ?? loaded.data)

  const through = (path: string, body?: unknown) => async () => {
    try {
      setMine(await post<DebriefView>(`/api/debriefs/${id}/${path}`, body))
    } catch (e) {
      say((e as Error).message)
      throw e
    }
  }
  const act: Act = local
    ? {
        decide: async (n, choice, note) => setMock((m) => m && decideIn(m, n, choice, note)),
        remember: async (n, on) => setMock((m) => m && rememberIn(m, n, on)),
        look: async (n, q) => setMock((m) => m && lookIn(m, n, q)),
        agreeRest: async () => setMock((m) => m && agreeRestIn(m)),
        explain: async (idea) => setMock((m) => m && explainIn(m, idea)),
        know: async (idea) => setMock((m) => m && knowIn(m, idea)),
        send: async () => {
          setMock((m) => m && sendIn(m))
          setJustSent(true)
        },
        again: async () => {},
      }
    : {
        decide: (n, choice, note) => through('decide', { n, choice, note: note ?? null })(),
        remember: (n, on) => through('remember', { n, on })(),
        look: (n, question) => through('look', { n, question })(),
        agreeRest: through('agree-rest'),
        explain: (idea) => through('explain', { idea })(),
        know: (idea) => through('know', { idea })(),
        send: async () => {
          await through('send')()
          setJustSent(true)
        },
        again: through('again'),
      }

  if (!v) {
    if (loaded.error) return <Gone said={loaded.error} example={example} />
    return <div className="db" aria-busy="true" />
  }
  if (v.state === 'waiting' || v.state === 'writing') return <Writing v={v} example={example} />
  if (v.state === 'failed') return <Failed v={v} act={act} example={example} toast={toast} />
  if (v.state === 'sent' && justSent) return <Sent v={v} example={example} back={() => setJustSent(false)} />
  return (
    <>
      <Ready v={v} act={act} example={example} />
      {toast}
    </>
  )
}

function Gone({ said, example }: { said: string; example?: boolean }) {
  return (
    <div className="db db-gone">
      <h1>This debrief isn’t here</h1>
      <p>{said}</p>
      <a className="btn btn-s" href={hrefOf('', example)}>Back to home</a>
    </div>
  )
}

// ---------------------------------------------------------------- while it's written

const ICONS = { done: 'm5 12 5 5 9-10', now: 'M12 6v6l4 2', later: 'M8 12h8' }

function Writing({ v, example }: { v: DebriefView; example?: boolean }) {
  const queued = v.state === 'waiting'
  return (
    <div className="db db-writing">
      <div className="db-writing-main">
        <div className="db-writing-head">
          <h1>{v.title}</h1>
          <Spinner>{queued ? `Your agent is busy with something else. ${START_NOW}` : 'Your agent is writing this.'}</Spinner>
        </div>
        <ol className="db-steps">
          {v.steps.map((s) => {
            const k = s.done ? 'done' : s.now ? 'now' : 'later'
            return (
              <li key={s.text} className={k}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={ICONS[k]} /></svg>
                {s.text}
                {s.done && <span className="sr-only"> (done)</span>}
              </li>
            )
          })}
        </ol>
        <div className={`db-skel${queued ? ' still' : ''}`} aria-hidden="true">
          <i style={{ width: '38%', height: 22 }} />
          <i style={{ width: '96%' }} />
          <i style={{ width: '88%' }} />
          <i style={{ width: '70%' }} />
        </div>
        <p className="db-writing-note">{queued ? 'It’ll be on home when it’s ready.' : 'About a minute. It’ll be on home when it’s ready.'}</p>
        <div><a className="btn btn-s" href={hrefOf('', example)}>Back to home</a></div>
      </div>
      <div className="db-writing-char"><Character size={150} move={queued ? 'idle' : 'work'} /></div>
    </div>
  )
}

function Failed({ v, act, example, toast }: { v: DebriefView; act: Act; example?: boolean; toast: React.ReactNode }) {
  return (
    <div className="db db-writing">
      <div className="db-writing-main">
        <div className="db-writing-head">
          <h1>{v.title}</h1>
          <p className="db-lead">Your agent couldn’t write this debrief.</p>
        </div>
        {v.problems.length > 0 && (
          <ul className="db-problems">{v.problems.map((p) => <li key={p}>{p}</li>)}</ul>
        )}
        <div className="db-acts">
          <button type="button" className="btn btn-p" onClick={() => void act.again().catch(() => {})}>Ask it again</button>
          <a className="btn btn-g" href={hrefOf('', example)}>Back to home</a>
        </div>
      </div>
      <div className="db-writing-char"><Character size={150} move="idle" /></div>
      {toast}
    </div>
  )
}

// ---------------------------------------------------------------- sent

const DOT: Record<SendLine['kind'], string> = { skill: 'skill', newSkill: 'new', research: 'quiet', redo: 'new', open: 'quiet' }

function Sent({ v, example, back }: { v: DebriefView; example?: boolean; back: () => void }) {
  return (
    <div className="db db-sent">
      <Character size={170} move="happy" />
      <div className="db-sent-head">
        <h1>Sent to your agent</h1>
        <p>It gets this the next time you message it.</p>
      </div>
      <ul className="db-sent-rows">
        {v.onSend.map((r) => (
          <li key={r.text}>
            <span className={`db-dot ${DOT[r.kind]}`} aria-hidden="true" />
            <span className="db-sent-t">{r.text}</span>
            {r.action && r.to && (
              r.to === `debrief/${v.id}`
                ? <button type="button" className="btn btn-s btn-sm" onClick={back}>{r.action}</button>
                : <a className="btn btn-s btn-sm" href={hrefOf(r.to, example)}>{r.action}</a>
            )}
          </li>
        ))}
      </ul>
      <a className="btn btn-p" href={hrefOf('', example)}>Back to home</a>
    </div>
  )
}

// ---------------------------------------------------------------- ready

function Ready({ v, act, example }: { v: DebriefView; act: Act; example?: boolean }) {
  const sent = v.state === 'sent'
  const [sending, setSending] = useState(false)
  const change = hrefOf(`change/${v.id}`, example)
  let group: string | null = null
  return (
    <div className="db">
      {v.example && <p className="db-example">An example to try. Nothing here changes your skills.</p>}
      <main className="db-main">
        <div className="row2">
          <div className="db-col">
            <h1 className="db-title">{v.title}</h1>
            <Markdown text={v.did} size="large" label="What it did" />
          </div>
          <div className="ann">
            <span className="db-counts">{filesSaid(v.files)} <span className="add">{plus(v.added)}</span> <span className="del">{minus(v.removed)}</span></span>
            <a className="go" href={change}>Open the change</a>
          </div>
        </div>

        <div className="row2">
          <div className="db-col">
            <h2 className="db-h2">How it works</h2>
            <Markdown text={v.how} label="How it works" />
          </div>
          <div />
        </div>

        {v.decisions.length > 0 && (
          <section className="db-decisions" aria-labelledby="decisions">
            <div className="row2">
              <div className="db-h2-row">
                <h2 className="db-h2" id="decisions">Decisions</h2>
                {!sent && v.left > 0 && <button type="button" className="btn btn-s btn-sm" onClick={() => void act.agreeRest().catch(() => {})}>Agree with the rest</button>}
              </div>
              <div className="ann"><span className="quiet">{sent ? '' : v.left ? `${v.left} to answer` : 'All answered'}</span></div>
            </div>
            {v.decisions.map((d) => {
              const heading = d.group && d.group !== group ? d.group : null
              group = d.group ?? group
              return (
                <div key={d.n}>
                  {heading && <h3 className="db-group">{heading}</h3>}
                  <Decision d={d} act={act} sent={sent} example={example} />
                </div>
              )
            })}
          </section>
        )}

        {v.ideas.length > 0 && (
          <section className="db-ideas" aria-labelledby="rests-on">
            <div className="row2"><h2 className="db-h2" id="rests-on">What it rests on</h2><div /></div>
            {v.ideas.map((i) => <Idea key={i.name} i={i} act={act} />)}
          </section>
        )}

        <div className="row2">
          <section className="db-files" aria-labelledby="files">
            <h2 className="db-h2" id="files">Files</h2>
            <Files v={v} example={example} />
          </section>
          <div />
        </div>

        <div className="row2 db-send">
          <div className="db-col">
            <h2 className="db-h3">{sent ? 'Sent to your agent' : 'When you send this'}</h2>
            <ul>
              {v.onSend.map((o) => (
                <li key={o.text}>
                  {o.text}
                  {o.action && o.to && o.to !== `debrief/${v.id}` && <> <a className="go" href={hrefOf(o.to, example)}>{o.action}</a></>}
                </li>
              ))}
            </ul>
            {!sent && (
              <div className="db-acts">
                <button
                  type="button"
                  className="btn btn-p"
                  disabled={sending}
                  onClick={() => {
                    setSending(true)
                    void act.send().catch(() => {}).finally(() => setSending(false))
                  }}
                >
                  Send to your agent
                </button>
                <a className="btn btn-g" href={hrefOf('', example)}>Finish later</a>
              </div>
            )}
          </div>
          <div />
        </div>
      </main>
    </div>
  )
}

function Decision({ d, act, sent, example }: { d: DecisionView; act: Act; sent: boolean; example?: boolean }) {
  const [code, setCode] = useState(false)
  const [asking, setAsking] = useState(false)
  const [question, setQuestion] = useState(d.question)
  const [note, setNote] = useState(d.note)
  useEffect(() => setNote(d.note), [d.note])
  const pick = (c: DecisionChoice) => {
    if (c === 'lookInto') {
      if (d.choice === 'lookInto') return void act.decide(d.n, null).catch(() => {})
      return setAsking(!asking)
    }
    setAsking(false)
    void act.decide(d.n, d.choice === c ? null : c, c === 'change' ? note : undefined).catch(() => {})
  }
  const choices: Array<[DecisionChoice, string]> = [['agree', 'Agree'], ['change', 'Change it'], ['lookInto', 'Look into it']]
  const ready = d.research?.state === 'ready'
  const looking = d.choice === 'lookInto' && d.research && d.research.state === 'asked'
  const failed = d.choice === 'lookInto' && d.research?.state === 'failed'
  return (
    <article className="row2 db-decision" id={`d${d.n}`} aria-labelledby={`d${d.n}-t`}>
      <div className="db-col tight">
        <h3 className="db-dtitle" id={`d${d.n}-t`}>{d.n}. {d.title}</h3>
        <Markdown text={d.why} label={`Why: ${d.title}`} />
        {!sent && (
          <div className="db-choices">
            {choices.map(([c, label]) => {
              const on = d.choice === c || (c === 'lookInto' && asking)
              return (
                <button key={c} type="button" className={`btn btn-sm ${on ? 'btn-on' : 'btn-s'}`} aria-pressed={on} onClick={() => pick(c)}>{label}</button>
              )
            })}
            {d.code.length > 0 && (
              <button type="button" className="btn btn-g btn-sm db-showcode" aria-expanded={code} onClick={() => setCode(!code)}>{code ? 'Hide the code' : 'Show the code'}</button>
            )}
          </div>
        )}
        {sent && d.code.length > 0 && (
          <div className="db-choices">
            <button type="button" className="btn btn-g btn-sm db-showcode" aria-expanded={code} onClick={() => setCode(!code)}>{code ? 'Hide the code' : 'Show the code'}</button>
          </div>
        )}
        {!sent && d.choice === 'change' && (
          <textarea
            className="field db-note"
            aria-label="How you want it"
            placeholder="How you want it"
            rows={2}
            value={note}
            onChange={(e) => setNote(e.target.value)}
            onBlur={() => note !== d.note && void act.decide(d.n, 'change', note).catch(() => {})}
          />
        )}
        {!sent && asking && d.choice !== 'lookInto' && (
          <form
            className="db-ask"
            onSubmit={(e) => {
              e.preventDefault()
              void act.look(d.n, question).then(() => setAsking(false), () => {})
            }}
          >
            <label htmlFor={`q${d.n}`}>What should your agent find out?</label>
            <textarea id={`q${d.n}`} className="field" rows={2} value={question} onChange={(e) => setQuestion(e.target.value)} />
            <div className="db-acts">
              <button type="submit" className="btn btn-p btn-sm" disabled={!question.trim()}>Ask your agent</button>
              <button type="button" className="btn btn-g btn-sm" onClick={() => setAsking(false)}>Cancel</button>
            </div>
          </form>
        )}
        {looking && <Spinner>{`Your agent is looking into it. ${IF_NOT_STARTED}`}</Spinner>}
        {failed && <p className="db-quiet">Your agent couldn’t look into this. Ask it again with Look into it.</p>}
        {d.choice === 'lookInto' && ready && d.research?.markdown && (
          <div className="db-research">
            <Markdown text={d.research.markdown} size="small" links label="What your agent found" />
            {!sent && (
              <div className="db-acts">
                <button type="button" className="btn btn-p btn-sm" onClick={() => void act.decide(d.n, 'change', d.research?.suggests ?? '').catch(() => {})}>Redo it this way</button>
                <button type="button" className="btn btn-g btn-sm" onClick={() => void act.decide(d.n, 'agree').catch(() => {})}>Leave it</button>
              </div>
            )}
          </div>
        )}
        {!sent && d.canRemember && d.skill && (
          <label className="db-remember">
            <input type="checkbox" checked={d.remember} onChange={() => void act.remember(d.n, !d.remember).catch(() => {})} />
            <span>
              <b>Do this every time</b>, {d.skill.new ? 'in a new skill, ' : 'in your '}
              {d.skill.new ? d.skill.title : <><a className="go" href={hrefOf(`skills/${d.skill.name}`, example)}>{d.skill.title}</a> skill</>}
            </span>
          </label>
        )}
        {code && <Code d={d} />}
      </div>
      <div className="ann"><span className={`db-status ${d.status}`}>{STATUS[d.status]}</span></div>
    </article>
  )
}

/** A decision's own lines, coloured as their language is. */
function Code({ d }: { d: DecisionView }) {
  const html = useMemo(() => {
    const lang = d.lang && colours(d.lang) ? d.lang : null
    const shown = d.code.filter((l) => l.kind !== 'fold' && l.kind !== 'removed')
    const lit = lang ? highlight(shown.map((l) => l.text), lang) : null
    const by = new Map<DebriefLine, string>()
    shown.forEach((l, i) => lit && by.set(l, lit[i]))
    return by
  }, [d.code, d.lang])
  return (
    <figure className="db-code">
      <figcaption>{d.place}</figcaption>
      <div className="db-lines code" role="group" aria-label={`The code: ${d.place}`}>
        {d.code.map((l, i) =>
          l.kind === 'fold' ? (
            <div key={i} className="db-fold"><span className="cv-fold"><b>{l.folded} lines</b> as before</span></div>
          ) : (
            <div key={i} className={`db-line ${l.kind}`}>
              <span className="n">{l.n ?? ''}</span>
              <span className="m" aria-hidden="true">{l.kind === 'added' || l.kind === 'yours' ? '+' : l.kind === 'removed' ? '−' : ''}</span>
              {html.get(l) !== undefined ? <span className="t" dangerouslySetInnerHTML={{ __html: html.get(l) ?? '' }} /> : <span className="t">{l.text}</span>}
            </div>
          ),
        )}
      </div>
    </figure>
  )
}

function Idea({ i, act }: { i: IdeaView; act: Act }) {
  const [open, setOpen] = useState(false)
  const e = i.explanation
  const toggle = () => {
    if (!open && !e) void act.explain(i.name).catch(() => {})
    if (!open && e?.state === 'failed') void act.explain(i.name).catch(() => {})
    setOpen(!open)
  }
  return (
    <div className="row2 db-idea">
      <div className="db-col tight">
        <p className="db-idea-t"><b>{i.name}.</b> {i.what}</p>
        <div className="db-choices">
          <button type="button" className={`btn btn-sm ${open ? 'btn-on' : 'btn-s'}`} aria-expanded={open} onClick={toggle}>{open ? 'Hide' : 'Explain'}</button>
          {i.newToYou && <button type="button" className="btn btn-g btn-sm" onClick={() => void act.know(i.name).catch(() => {})}>I know this</button>}
        </div>
        {open && (e?.state === 'ready' && e.markdown
          ? <Markdown text={e.markdown} size="small" label={`${i.name}, explained`} />
          : e?.state === 'failed' ? <p className="db-quiet">Your agent couldn’t explain this. Try again later.</p>
            : <Spinner>{`Your agent is writing this. ${IF_NOT_STARTED}`}</Spinner>)}
      </div>
      <div className="ann">{i.newToYou && <span className="db-new">New to you</span>}</div>
    </div>
  )
}

/** Every file, in its folders. A folder's name ends in a slash; the top folder has none. */
function Files({ v, example }: { v: DebriefView; example?: boolean }) {
  const folderPath = (name: string) => (name.endsWith('/') ? name.slice(0, -1) : '')
  const more = Object.fromEntries(v.folders.filter((f) => f.count > f.files.length).map((f) => [folderPath(f.name), f.count - f.files.length]))
  return (
    <FileTree
      files={v.folders.flatMap((f) => f.files)}
      startsOpen={(path) => v.folders.some((f) => f.open && f.name.startsWith(`${path}/`))}
      more={more}
      note={filesSaid}
      file={(x, name) => (
        <a className="frow" href={`${hrefOf(`change/${v.id}`, example)}?file=${encodeURIComponent(x.path)}`}>
          <span className="frow-l">
            {name}
            {x.yours && <span className="db-yours">your edit</span>}
          </span>
          <span className="mono small"><span className="add">{plus(x.added)}</span> <span className="del">{minus(x.removed)}</span></span>
        </a>
      )}
    />
  )
}
