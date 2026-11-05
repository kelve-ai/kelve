// Meeting your agent, the first time kelve opens (board Onboarding): three
// looks to start from, a name if you like, and Start, to home. Seeing it is
// the first run: home sends no one back here after.

import { useEffect, useState } from 'react'
import { changed, post } from '../../api/client'
import type { CharacterView } from '../../api/types'
import { hrefOf } from '../../routes'
import { Character, useMyCharacter } from '../../ui/Character'
import { Btn } from '../../ui/kit'
import { NOTHING_MADE } from '../../ui/pixels'
import { met, STARTS } from './looks'

export function Welcome({ example }: { example?: boolean }) {
  const mine = useMyCharacter()
  const [chosen, setChosen] = useState(STARTS[0].key)
  const [name, setName] = useState('')
  const [said, setSaid] = useState<string>()
  const [saving, setSaving] = useState(false)
  // Met: from now on, home is home.
  useEffect(() => {
    met.now = true
    if (!example) void post('/api/settings', { hintSeen: 'met' }).then(() => changed('settings'), () => {})
  }, [example])
  const start = async () => {
    if (example) {
      location.hash = hrefOf('', true)
      return
    }
    setSaving(true)
    setSaid(undefined)
    try {
      const look = STARTS.find((s) => s.key === chosen)!.look
      await post<CharacterView>('/api/character', { look, name })
      changed('settings')
      location.hash = '#/'
    } catch (e) {
      setSaid((e as Error).message)
      setSaving(false)
    }
  }
  const placeholder = mine && !mine.named ? mine.name : 'Your agent'
  return (
    <div className="meet">
      <div className="meet-top">
        <h1>Meet your agent</h1>
        <p>It learns each time you correct it. Pick a look to start.</p>
      </div>
      <div className="meet-looks" role="radiogroup" aria-label="How it looks">
        {STARTS.map((s) => {
          const on = s.key === chosen
          return (
            <button key={s.key} type="button" role="radio" aria-checked={on} className="pick" onClick={() => setChosen(s.key)}>
              <span aria-hidden="true"><Character size={176} move={on ? 'wave' : 'idle'} look={s.look} made={NOTHING_MADE} label={s.label} /></span>
              <span className="meet-label">{s.label}</span>
            </button>
          )
        })}
      </div>
      <form
        className="meet-go"
        onSubmit={(e) => {
          e.preventDefault()
          void start()
        }}
      >
        <div className="field-at">
          <label htmlFor="agent-name">Give it a name, if you like</label>
          <input id="agent-name" className="field big" value={name} placeholder={placeholder} maxLength={24} onChange={(e) => setName(e.target.value)} aria-describedby={said ? 'agent-name-said' : undefined} />
        </div>
        <Btn kind="primary" size="large" type="submit" disabled={saving}>Start</Btn>
      </form>
      {said && <p className="warn" id="agent-name-said" role="alert">{said}</p>}
    </div>
  )
}
