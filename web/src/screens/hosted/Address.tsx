// Choosing your page's address (board ChooseHandle): kelve.ai/@ and what you
// type, checked as you type, with others like it when someone has it. Saved,
// it goes back to where it was asked for: publishing a score, or Your agent.

import { useEffect, useState } from 'react'
import type { HandleView, HostedView } from '../../api/types'
import { hrefOf } from '../../routes'
import { Character } from '../../ui/Character'
import { Btn } from '../../ui/kit'
import { send, useHosted } from './common'

/** What the rules say before anything is typed. */
const RULES = 'Letters, numbers and hyphens, up to 24.'

/** How long typing pauses before kelve.ai is asked. */
const PAUSE = 250

const OK = 'M5 12l5 5 9-10'
const NO = 'M18 6 6 18M6 6l12 12'

export function Address({ example }: { example?: boolean }) {
  const hosted = useHosted()
  const h = hosted.data
  const [typed, setTyped] = useState<string | null>(null)
  const [check, setCheck] = useState<HandleView | null>(null)
  const [said, setSaid] = useState<string>()
  const [saving, setSaving] = useState(false)
  // What's typed starts as the address they have, if any, or as the example's.
  const shown = example ? (new URLSearchParams(location.hash.split('?')[1] ?? '').get('as') === 'taken' ? 'alex' : 'sam') : (h?.handle ?? '')
  const value = typed ?? shown
  useEffect(() => {
    const v = value.trim()
    if (!v) {
      setCheck(null)
      return
    }
    let gone = false
    const t = window.setTimeout(() => {
      send<HandleView>('/api/hosted/handle/check', { handle: v }).then(
        (c) => {
          if (!gone) {
            setCheck(c)
            setSaid(undefined)
          }
        },
        (e: Error) => !gone && setSaid(e.message),
      )
    }, PAUSE)
    return () => {
      gone = true
      window.clearTimeout(t)
    }
  }, [value])
  const fresh = check && check.handle === value.trim().replace(/^@/, '').toLowerCase()
  const tone = !value.trim() || !fresh ? 'none' : check.available ? 'ok' : 'no'
  // Only ever a screen in kelve's own Your agent.
  const asked = new URLSearchParams(location.hash.split('?')[1] ?? '').get('then')
  const then = asked && /^agent(\/[a-z0-9/-]+)?$/i.test(asked) ? asked : 'agent'
  const save = () => {
    setSaving(true)
    send<HostedView>('/api/hosted/handle', { handle: value }).then(
      () => {
        hosted.reload()
        location.hash = hrefOf(then, example)
      },
      (e: Error) => {
        setSaid(e.message)
        setSaving(false)
      },
    )
  }
  return (
    <div className="hs-screen narrow">
      <div className="hs-head">
        <Character size={112} move={tone === 'ok' ? 'happy' : 'idle'} />
        <div>
          <h1>Choose your page’s address</h1>
          <p>Nothing shows there until you publish a score.</p>
        </div>
      </div>
      <form
        className="hs-addr"
        onSubmit={(e) => {
          e.preventDefault()
          if (tone === 'ok' && !saving) save()
        }}
      >
        <label htmlFor="hs-handle">Address</label>
        <div className={`hs-field ${tone}`}>
          <span className="pre" aria-hidden="true">kelve.ai/@</span>
          <input id="hs-handle" value={value} onChange={(e) => setTyped(e.target.value)} maxLength={30} autoComplete="off" spellCheck={false} aria-describedby="hs-handle-status" autoFocus={!example} />
          {tone !== 'none' && (
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" style={{ margin: '0 16px', color: tone === 'ok' ? 'var(--better)' : 'var(--accent-ink)' }}>
              <path d={tone === 'ok' ? OK : NO} />
            </svg>
          )}
        </div>
        <div id="hs-handle-status" className={`hs-status ${tone}`} role="status" aria-live="polite">
          <b>{tone === 'none' ? RULES : check?.why ?? (check?.available ? 'You can have that one.' : '')}</b>
          {tone === 'no' && check && check.instead.length > 0 && (
            <>
              <span className="try">Try</span>
              {check.instead.map((x) => (
                <button key={x} type="button" className="hs-sug" onClick={() => setTyped(x)}>{x}</button>
              ))}
            </>
          )}
        </div>
        {said && <p className="hs-alert" role="alert">{said}</p>}
        <div className="hs-acts">
          <Btn kind="primary" size="large" type="submit" disabled={tone !== 'ok' || saving}>Save address</Btn>
        </div>
      </form>
    </div>
  )
}
