// Getting a verified score (board RunSetup): the kind of work, with a retired
// task of it as an example, the agent and model both runs use, and exactly
// what goes, before anything goes. Starting
// it sends your skills, and kelve.ai says what the run will take: about what
// it costs on your own key for the model's provider, which kelve asks for
// the first time and keeps sealed. Nothing runs until you confirm.

import { useEffect, useState } from 'react'
import { ApiError, del } from '../../api/client'
import type { CredentialKind, EstimateView, ExampleTaskView, OfferView, RunView } from '../../api/types'
import { hrefOf } from '../../routes'
import { Character } from '../../ui/Character'
import { Btn } from '../../ui/kit'
import { useLoad } from '../../useLoad'
import { SignInSheet, count, send, size, useHosted } from './common'

export function RunSetup({ example }: { example?: boolean }) {
  const hosted = useHosted()
  const h = hosted.data
  const offer = useLoad<OfferView>(h?.signedIn ? '/api/hosted/offer' : null, ['settings'])
  const o = offer.data
  const [kind, setKind] = useState<string>()
  const [agent, setAgent] = useState<string>()
  const [model, setModel] = useState<string>()
  const [who, setWho] = useState(false)
  const [files, setFiles] = useState(false)
  const [seeing, setSeeing] = useState(false)
  const [estimate, setEstimate] = useState<EstimateView | null>(null)
  const [busy, setBusy] = useState(false)
  const [said, setSaid] = useState<string>()
  const [signing, setSigning] = useState(false)
  // The example shows what comes after Start the run, too.
  const as = example ? new URLSearchParams(location.hash.split('?')[1] ?? '').get('as') : null
  useEffect(() => {
    if (as === 'confirm' && !estimate) void send<EstimateView>('/api/hosted/runs').then(setEstimate, () => {})
  }, [as, estimate])

  if (h && !h.signedIn) {
    return (
      <div className="hs-screen mid">
        <Head />
        <div className="hs-acts">
          <Btn kind="primary" size="large" onClick={() => setSigning(true)}>Sign in to kelve.ai</Btn>
          <Btn kind="quiet" size="large" href={hrefOf('agent', example)}>Not now</Btn>
        </div>
        {signing && <SignInSheet onClose={() => setSigning(false)} onDone={() => setSigning(false)} />}
      </div>
    )
  }
  const chosenKind = o?.kinds.find((k) => k.kind === kind) ?? o?.kinds[0]
  const chosenAgent = o?.agents.find((a) => a.agent === agent) ?? o?.agents[0]
  const chosenModel = chosenAgent?.models.find((m) => m.model === model) ?? chosenAgent?.models[0]
  const left = o?.runs.left ?? h?.runs?.left ?? 0
  const start = (credential?: CredentialKind) => {
    if (!chosenKind || !chosenAgent || !chosenModel) return
    setBusy(true)
    send<EstimateView>('/api/hosted/runs', { kind: chosenKind.kind, agent: chosenAgent.agent, model: chosenModel.model, credential }).then(
      (e) => {
        setEstimate(e)
        setSaid(undefined)
        setBusy(false)
      },
      (e: Error) => {
        // The estimate before was dropped already: start again from here.
        if (credential) setEstimate(null)
        setSaid(e.message)
        setBusy(false)
      },
    )
  }
  return (
    <div className="hs-screen mid">
      <Head />
      {offer.error && !o && (
        <>
          <p className="hs-alert" role="alert">{offer.error}</p>
          <div className="hs-acts">
            <Btn kind="primary" size="large" onClick={offer.reload}>Try again</Btn>
            <Btn kind="quiet" size="large" href={hrefOf('agent', example)}>Not now</Btn>
          </div>
        </>
      )}
      {o && chosenKind && chosenAgent && chosenModel && (
        <>
          <section className="hs-sec" aria-labelledby="hs-kind-h">
            <h2 id="hs-kind-h">What kind of work</h2>
            <div className="hs-kinds" role="radiogroup" aria-labelledby="hs-kind-h">
              {o.kinds.map((k) => (
                <button key={k.kind} type="button" className="hs-kind" role="radio" aria-checked={k.kind === chosenKind.kind} disabled={!!estimate} onClick={() => setKind(k.kind)}>
                  <b>{k.name}</b>
                  <span>{count(k.tasks, 'task')}</span>
                </button>
              ))}
            </div>
            <p className="hs-quiet">
              {chosenKind.about}
              {chosenKind.examples.length > 0 && (
                <>
                  {' '}
                  <button type="button" className="hs-lkb" aria-expanded={seeing} onClick={() => setSeeing(!seeing)}>{seeing ? 'Hide the example' : 'See an example task'}</button>
                </>
              )}
            </p>
            {seeing && chosenKind.examples.length > 0 && <Examples tasks={chosenKind.examples} />}
          </section>
          <div className="hs-says">
            <p>
              Both run as {chosenAgent.name} on {chosenModel.name}.{' '}
              {!estimate && <button type="button" className="hs-lkb" aria-expanded={who} onClick={() => setWho(!who)}>{who ? 'Done' : 'Change'}</button>}
            </p>
            {who && !estimate && (
              <div className="hs-pick">
                <label>
                  Agent
                  <select value={chosenAgent.agent} onChange={(e) => { setAgent(e.target.value); setModel(undefined) }}>
                    {o.agents.map((a) => <option key={a.agent} value={a.agent}>{a.name}</option>)}
                  </select>
                </label>
                <label>
                  Model
                  <select value={chosenModel.model} onChange={(e) => setModel(e.target.value)}>
                    {chosenAgent.models.map((m) => <option key={m.model} value={m.model}>{m.name}</option>)}
                  </select>
                </label>
              </div>
            )}
            <p>
              kelve sends all your skills, and kelve.ai deletes them after the run.{' '}
              <button type="button" className="hs-lkb" aria-expanded={files} onClick={() => setFiles(!files)}>{files ? 'Hide what goes' : 'See exactly what goes'}</button>
            </p>
            {files && (
              <div className="hs-files">
                {o.skills.map((f) => (
                  <div key={f.path}><span className="p">{f.path}</span><span className="n">{size(f.bytes)}</span></div>
                ))}
                {o.skills.length === 0 && <div><span>Your agent has no skills yet.</span></div>}
                <small>Secrets and paths are taken out first. Nothing else goes.</small>
              </div>
            )}
          </div>
          {estimate ? (
            <Confirm
              key={estimate.run}
              estimate={estimate}
              example={example}
              onBack={() => setEstimate(null)}
              onPay={(c) => {
                // Estimated again for what pays: the one before is dropped,
                // and kelve.ai deletes the skills it was sent.
                void del(`/api/hosted/runs/${encodeURIComponent(estimate.run)}`).catch(() => {})
                start(c)
              }}
            />
          ) : (
            <div className="hs-confirm">
              {left > 0 ? <p className="hs-quiet">{count(left, 'run')} left today.</p> : <p className="hs-quiet">You’ve used today’s runs. More come at midnight UTC.</p>}
              {said && <p className="hs-alert" role="alert">{said}</p>}
              <div className="hs-acts">
                <Btn kind="primary" size="large" disabled={left === 0 || busy} onClick={() => start()}>Start the run</Btn>
                <Btn kind="quiet" size="large" href={hrefOf('agent', example)}>Not now</Btn>
              </div>
            </div>
          )}
        </>
      )}
    </div>
  )
}

/** What each kind of hidden check is called. */
const CHECKED: Record<string, string> = { test: 'Hidden test', static: 'Code check', judge: 'The judge' }

/** Retired tasks of a kind, as examples: what the agent was asked, the files it started with, and how it was checked. */
function Examples({ tasks }: { tasks: ExampleTaskView[] }) {
  return (
    <div className="hs-ex" role="region" aria-label="An example task">
      {tasks.map((t) => (
        <div key={t.name} className="hs-ex-task">
          <h3>What the agent was asked</h3>
          <p className="hs-ex-ask">{t.request}</p>
          <h3>The files it started with</h3>
          <ul className="hs-ex-files">
            {t.files.map((f) => <li key={f}>{f}</li>)}
          </ul>
          <h3>How it was checked</h3>
          <ul className="hs-ex-checks">
            {t.checks.map((c, i) => (
              <li key={i}><b>{CHECKED[c.kind] ?? c.kind}</b> {c.about}</li>
            ))}
          </ul>
        </div>
      ))}
      <small>A retired task, shown in full. The tasks a run does are never shown.</small>
    </div>
  )
}

function Head() {
  return (
    <div className="hs-head">
      <Character size={104} move="idle" />
      <div>
        <h1>Get a verified score</h1>
        <p>Your agent and a standard one do the same secret tasks. Hidden tests check both. <a className="go" href="https://kelve.ai/docs/how-verified-scores-work/" target="_blank" rel="noopener">How verified scores work</a></p>
      </div>
    </div>
  )
}

/** How each credential is offered, and what it means. */
const KINDS: Record<CredentialKind, { label: string; line: string; field: (provider: string) => string; hint: string }> = {
  subscription: {
    label: 'Your Claude subscription (from claude setup-token)',
    line: 'It uses your plan’s usage, as your agent does at home.',
    field: () => 'The token claude setup-token gave you',
    hint: 'Run claude setup-token in your terminal, then paste what it gives. kelve keeps it sealed.',
  },
  apiKey: {
    label: 'An API key',
    line: 'Anthropic bills you for what the run uses.',
    field: (provider) => `Your ${provider} API key`,
    hint: 'kelve keeps it sealed, and sends it only with a run you start.',
  },
}

/** What the run will take, and what pays for it: then it starts. */
function Confirm({ estimate: e, example, onBack, onPay }: { estimate: EstimateView; example?: boolean; onBack: () => void; onPay: (c: CredentialKind) => void }) {
  // What it was estimated for; choosing another estimates it again.
  const kind = e.paying
  const [key, setKey] = useState('')
  const [askAgain, setAskAgain] = useState(false)
  const [said, setSaid] = useState<string>()
  const [busy, setBusy] = useState(false)
  const ask = askAgain || !e.kept.includes(kind)
  const go = () => {
    setBusy(true)
    send<RunView>(`/api/hosted/runs/${encodeURIComponent(e.run)}/confirm`, ask ? { kind, key } : { kind }).then(
      (r) => {
        location.hash = hrefOf(`agent/score/${encodeURIComponent(r.run)}`, example)
      },
      (err: Error) => {
        setSaid(err.message)
        setBusy(false)
        if (err instanceof ApiError && err.status === 400) setAskAgain(true)
      },
    )
  }
  const back = () => {
    // Dropped: kelve.ai deletes the skills it was sent.
    void del(`/api/hosted/runs/${encodeURIComponent(e.run)}`).catch(() => {})
    onBack()
  }
  const dollars = e.dollars < 1 ? 'Under $1' : `About $${Math.round(e.dollars)}`
  const k = KINDS[kind]
  return (
    <form
      className="hs-confirm"
      aria-label="Start the run"
      onSubmit={(ev) => {
        ev.preventDefault()
        if (!busy && (!ask || key.trim())) go()
      }}
    >
      {e.accepts.length > 1 && (
        <fieldset className="hs-pay">
          <legend>What pays for it</legend>
          {e.accepts.map((c) => (
            <label key={c}>
              <input
                type="radio"
                name="hs-pay"
                checked={kind === c}
                disabled={busy}
                onChange={() => {
                  if (c === kind) return
                  setBusy(true)
                  onPay(c)
                }}
              />
              <span>
                <b>{KINDS[c].label}</b>
                <small>{KINDS[c].line}</small>
              </span>
            </label>
          ))}
        </fieldset>
      )}
      <p className="hs-cost">
        {kind === 'subscription' ? `About ${count(e.agentRuns, 'run')} of your agent on your Claude plan.` : `${dollars} on your ${e.providerName} key.`}
      </p>
      <p className="hs-quiet">
        {count(e.tasks, `${e.kindName} task`)}, each run {e.runsPerTask} times with your skills. It takes about {e.minutes} minutes.
      </p>
      {ask && (
        <div className="field-at">
          <label htmlFor="hs-provider-key">{k.field(e.providerName)}</label>
          <input id="hs-provider-key" className="field" type="password" autoComplete="off" spellCheck={false} value={key} onChange={(ev) => setKey(ev.target.value)} aria-describedby="hs-provider-key-hint" />
          <small id="hs-provider-key-hint">{k.hint}</small>
        </div>
      )}
      {said && <p className="hs-alert" role="alert">{said}</p>}
      <div className="hs-acts">
        <Btn kind="primary" size="large" type="submit" disabled={busy || (ask && !key.trim())}>Start the run</Btn>
        <Btn kind="quiet" size="large" onClick={back}>Not now</Btn>
      </div>
    </form>
  )
}
