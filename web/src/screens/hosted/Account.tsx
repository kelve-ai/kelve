// Settings, Account (boards SettingsAccountOut and SettingsAccountIn): signing
// in to kelve.ai, or who you're signed in as; your page and whether it's
// published; the runs left today; your model provider's key, kept sealed for
// runs; and deleting the account, which takes the page and scores with it.

import { useState } from 'react'
import { del, post } from '../../api/client'
import type { HostedView } from '../../api/types'
import { hrefOf } from '../../routes'
import { Btn, Toggle, useToast } from '../../ui/kit'
import { SignInSheet, count, send, shortPage, useHosted } from './common'

function Row({ title, what, children }: { title: React.ReactNode; what?: React.ReactNode; children?: React.ReactNode }) {
  return (
    <div className="hs-opt">
      <div className="hs-t">
        <b>{title}</b>
        {what && <span>{what}</span>}
      </div>
      {children}
    </div>
  )
}

export function AccountSection({ example }: { example?: boolean }) {
  const hosted = useHosted()
  const h = hosted.data
  const [toast, say] = useToast()
  const [signing, setSigning] = useState(false)
  const [said, setSaid] = useState<string>()
  // Ask first, then do it: changing the account always says what happened.
  const act = (run: () => Promise<HostedView>, done: string) =>
    run().then(
      () => {
        setSaid(undefined)
        hosted.reload()
        say(done)
      },
      (e: Error) => setSaid(e.message),
    )
  if (!h) return null
  return (
    <>
      {h.problem && <p className="hs-alert" role="alert">{h.problem}</p>}
      {said && <p className="hs-alert" role="alert">{said}</p>}
      {!h.available ? (
        <Row title="No kelve.ai to sign in to" what="This kelve runs without it. Everything else works." />
      ) : !h.signedIn ? (
        <SignedOut h={h} signing={signing} setSigning={setSigning} reload={hosted.reload} />
      ) : (
        <SignedIn h={h} act={act} example={example} />
      )}
      {signing && !h.signedIn && (
        <SignInSheet
          onClose={() => setSigning(false)}
          onDone={() => {
            setSigning(false)
            say('Signed in to kelve.ai.')
          }}
        />
      )}
      {toast}
    </>
  )
}

function SignedOut({ h, signing, setSigning, reload }: { h: HostedView; signing: boolean; setSigning: (on: boolean) => void; reload: () => void }) {
  return (
    <div>
      <p className="hs-lede">Only for verified scores and a public page. Everything else works without it.</p>
      {h.waiting && !signing ? (
        <div className="hs-opt">
          <div className="hs-t">
            <div className="busy" role="status"><span className="spin" aria-hidden="true" />Finish signing in, in the browser tab that opened.</div>
          </div>
          <Btn kind="quiet" onClick={() => void post('/api/hosted/signin/stop').then(reload, () => {})}>Cancel</Btn>
        </div>
      ) : (
        <Row title="Sign in to kelve.ai" what="With GitHub or Google.">
          <Btn kind="primary" onClick={() => setSigning(true)}>Sign in to kelve.ai</Btn>
        </Row>
      )}
    </div>
  )
}

function SignedIn({ h, act, example }: { h: HostedView; act: (run: () => Promise<HostedView>, done: string) => void; example?: boolean }) {
  const [sure, setSure] = useState(false)
  const [changing, setChanging] = useState<string | null>(null)
  const [key, setKey] = useState('')
  const runs = h.runs
  return (
    <div>
      <Row title={h.handle ? <>Signed in as <span className="mono">@{h.handle}</span></> : 'Signed in to kelve.ai'} what={h.handle ? undefined : 'Choose your page’s address to get a verified score.'}>
        <div className="hs-row">
          {!h.handle && <Btn size="small" href={hrefOf('agent/address', example)}>Choose an address</Btn>}
          <Btn size="small" onClick={() => act(() => send<HostedView>('/api/hosted/signout'), 'Signed out of kelve.ai.')}>Sign out</Btn>
        </div>
      </Row>
      {h.handle && h.page && (
        <Row
          title={<>Your page, <a className="go" href={h.page} target="_blank" rel="noreferrer">{shortPage(h.page)}</a></>}
          what={h.public ? 'Published. It shows your agent and the scores you choose.' : 'Not published. No one sees it.'}
        >
          <Toggle
            on={h.public}
            label="Publish your page"
            onChange={(on) => act(() => send<HostedView>('/api/hosted/page', { public: on }), on ? 'Your page is published.' : 'Your page is taken down.')}
          />
        </Row>
      )}
      {runs && <Row title="Verified runs" what={`${count(runs.left, 'run')} left today, of ${runs.aDay}.`} />}
      {h.keys.length === 0 && <Row title="What pays for your runs" what="kelve asks the first time a run needs it, and keeps it sealed." />}
      {h.keys.map((k) => {
        const name = k.kind === 'subscription' ? 'Claude subscription token' : `${k.name} key`
        const at = `${k.kind === 'subscription' ? 'subscription' : 'api-key'}/${encodeURIComponent(k.provider)}`
        return (
          <div className="hs-opt" key={at}>
            <div className="hs-t">
              <b>Your {name}</b>
              <span>{k.kind === 'subscription' ? 'Runs use your plan’s usage.' : 'Runs are billed to it.'} Kept sealed, and sent only with a run you start.</span>
            </div>
            {changing === at ? (
              <form
                className="hs-key"
                onSubmit={(e) => {
                  e.preventDefault()
                  act(() => send<HostedView>('/api/hosted/keys', { provider: k.provider, kind: k.kind, key }), `Your ${name} is changed.`)
                  setChanging(null)
                  setKey('')
                }}
              >
                <input className="field" type="password" autoComplete="off" spellCheck={false} aria-label={`Your new ${name}`} value={key} onChange={(e) => setKey(e.target.value)} autoFocus />
                <Btn size="small" kind="primary" type="submit" disabled={!key.trim()}>Save</Btn>
                <Btn size="small" kind="quiet" onClick={() => setChanging(null)}>Cancel</Btn>
              </form>
            ) : (
              <div className="hs-row">
                <Btn size="small" onClick={() => setChanging(at)} label={`Change your ${name}`}>Change</Btn>
                <Btn size="small" kind="quiet" onClick={() => act(() => del<HostedView>(`/api/hosted/keys/${at}`), `Your ${name} is forgotten.`)} label={`Forget your ${name}`}>Forget</Btn>
              </div>
            )}
          </div>
        )
      })}
      <Row title="Delete your account and page" what={sure ? 'It can’t be undone. Your skills stay.' : 'Your scores go too. Your skills and everything in kelve stay.'}>
        {sure ? (
          <div className="hs-row">
            <Btn size="small" onClick={() => act(() => send<HostedView>('/api/hosted/account/delete'), 'Your account and page are deleted.')}><span className="hs-danger">Delete my account and page</span></Btn>
            <Btn size="small" kind="quiet" onClick={() => setSure(false)}>Keep it</Btn>
          </div>
        ) : (
          <Btn size="small" onClick={() => setSure(true)}>Delete</Btn>
        )}
      </Row>
    </div>
  )
}
