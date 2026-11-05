// Changing how your agent looks (board Customise): it on the left, moving as
// you choose, with its name; on the right, the robot's parts and colours, or
// a whole character; and below them, asking your agent to draw a part or a
// character, which kelve checks and keeps here to try on. Nothing is kept
// until Save.

import { useEffect, useMemo, useState } from 'react'
import { START_NOW } from '../../agentList'
import { changed, post } from '../../api/client'
import type { AgentLook, CharacterMove, CharacterView, DrawingView, PartSlot } from '../../api/types'
import { EXAMPLE_DRAWINGS } from '../../example/agent'
import { Header } from '../../Header'
import { hrefOf } from '../../routes'
import { Character, useMyCharacter } from '../../ui/Character'
import { Btn, Segmented, Spinner, useToast } from '../../ui/kit'
import { defaultLook, library, type Made, madeOf } from '../../ui/pixels'
import { MOVES, useAgentLoad } from './looks'

type Tab = 'character' | 'colours' | PartSlot

const COLOUR_ROWS = [
  { key: 'body', label: 'Body' },
  { key: 'light', label: 'Light and hands' },
  { key: 'screen', label: 'Screen' },
  { key: 'eyes', label: 'Eyes' },
] as const

/** A look with one part changed. */
function wearing(look: AgentLook, slot: PartSlot, key: string): AgentLook {
  return { ...look, character: 'robot', robot: { ...look.robot, parts: { ...look.robot.parts, [slot]: key } } }
}

/** One choice: it drawn small, its name, and a line under it. */
function Pick({ on, onClick, look, made, label, sub }: { on: boolean; onClick: () => void; look: AgentLook; made: Made; label: string; sub?: string }) {
  return (
    <button type="button" className="pick tile" aria-pressed={on} onClick={onClick}>
      <span aria-hidden="true"><Character size={84} move="still" look={look} made={made} label={label} /></span>
      <b>{label}</b>
      {sub && <small>{sub}</small>}
    </button>
  )
}

/** Where a drawing asked for is, in a line: those done are in what it made. */
function DrawingLine({ d }: { d: DrawingView }) {
  if (d.state === 'failed')
    return <li className="drew failed">{`Your agent’s drawing of “${d.asked}” didn’t fit kelve’s grid. Ask again, or say it another way.`}</li>
  if (d.state === 'drawing')
    return <li className="drew"><Spinner>{`Your agent is drawing “${d.asked}”.`}</Spinner></li>
  return <li className="drew">{`Waiting for your agent to draw “${d.asked}”. ${START_NOW}`}</li>
}

export function Customise({ example }: { example?: boolean }) {
  const mine = useMyCharacter()
  const drawings = useAgentLoad<DrawingView[]>('/api/character/drawings', example ? EXAMPLE_DRAWINGS : undefined, ['settings', 'asks'])
  const [look, setLook] = useState<AgentLook>()
  const [name, setName] = useState('')
  const [move, setMove] = useState<CharacterMove>('idle')
  const [tab, setTab] = useState<Tab>('character')
  const [asked, setAsked] = useState('')
  const [said, setSaid] = useState<string>()
  const [saving, setSaving] = useState(false)
  const [toast, say] = useToast()
  // What it looks like now, until the person changes it.
  useEffect(() => {
    if (mine && !look) {
      setLook(mine.look)
      setName(mine.named ? mine.name : '')
    }
  }, [mine, look])
  const made = useMemo(() => madeOf(mine?.made ?? []), [mine])
  const l = look ?? mine?.look ?? defaultLook()
  const robot = l.character === 'robot'
  const slots = library.robot.slots
  const tabs: Array<{ value: Tab; label: string }> = [
    { value: 'character', label: 'Character' },
    ...(robot ? slots.map((s) => ({ value: s.slot as Tab, label: s.label })) : []),
    { value: 'colours', label: 'Colours' },
  ]
  const shownTab = tabs.some((t) => t.value === tab) ? tab : 'character'
  const madeParts = (mine?.made ?? []).filter((m) => m.part)

  const save = async () => {
    if (example) {
      location.hash = hrefOf('agent', true)
      return
    }
    setSaving(true)
    setSaid(undefined)
    try {
      await post<CharacterView>('/api/character', { look: l, name })
      changed('settings')
      location.hash = '#/agent'
    } catch (e) {
      setSaid((e as Error).message)
      setSaving(false)
    }
  }
  const ask = async () => {
    setSaid(undefined)
    if (example) {
      say('The example can’t be changed. Ask your own agent in your own kelve.')
      return
    }
    try {
      await post<DrawingView>('/api/character/draw', { asked })
      setAsked('')
      drawings.reload()
      say('Asked. Your agent draws it next time it does kelve’s jobs.')
    } catch (e) {
      setSaid((e as Error).message)
    }
  }
  const tryOn = (key: string) => {
    const m = mine?.made.find((x) => x.key === key)
    if (!m) return
    if (m.slot) {
      setLook(wearing(l, m.slot, key))
      setTab(m.slot)
    } else {
      setLook({ ...l, character: key })
      setTab('character')
    }
  }
  const going = (drawings.data ?? []).filter((d) => d.state !== 'done').slice(0, 3)

  return (
    <div className="app">
      <Header
        here="agent"
        example={example}
        actions={
          <>
            <Btn kind="quiet" href={hrefOf('agent', example)}>Cancel</Btn>
            <Btn kind="primary" onClick={() => void save()} disabled={saving || !mine}>Save</Btn>
          </>
        }
      />
      <main className="look">
        <h1 className="sr-only">Change how it looks</h1>
        <section className="look-now" aria-label="How it looks now">
          <div className="look-stage">
            <Character size={300} move={move} look={l} made={made} label={name.trim() || mine?.name || 'Your agent'} />
          </div>
          <Segmented label="See it move" options={MOVES.map((m) => ({ value: m.key, label: m.label }))} value={move} onChange={setMove} />
          <div className="field-at">
            <label htmlFor="look-name">Name, if you’d like one</label>
            <input id="look-name" className="field big" value={name} placeholder={mine && !mine.named ? mine.name : 'Your agent'} maxLength={24} onChange={(e) => setName(e.target.value)} />
          </div>
          {said && <p className="warn" role="alert">{said}</p>}
        </section>

        <section className="look-choose" aria-label="Choose">
          <div className="look-tabs" role="group" aria-label="Parts">
            {tabs.map((t) => (
              <Btn key={t.value} size="small" pressed={shownTab === t.value} onClick={() => setTab(t.value)}>{t.label}</Btn>
            ))}
          </div>

          {shownTab === 'character' && (
            <div className="look-grid wide" role="group" aria-label="Characters">
              <Pick on={robot} onClick={() => setLook({ ...l, character: 'robot' })} look={{ ...l, character: 'robot' }} made={made} label="Robot" sub="Build it from parts" />
              {library.characters.map((c) => (
                <Pick
                  key={c.key}
                  on={l.character === c.key}
                  onClick={() => setLook({ ...l, character: c.key, robot: { ...l.robot, colours: { ...c.colours } } })}
                  look={{ ...l, character: c.key, robot: { ...l.robot, colours: c.colours } }}
                  made={made}
                  label={c.label}
                />
              ))}
              {(mine?.made ?? []).filter((m) => m.character).map((m) => (
                <Pick key={m.key} on={l.character === m.key} onClick={() => setLook({ ...l, character: m.key })} look={{ ...l, character: m.key }} made={made} label={m.name} sub="Made by your agent" />
              ))}
            </div>
          )}

          {slots.map((s) =>
            shownTab === s.slot ? (
              <div key={s.slot} className="look-grid" role="group" aria-label={s.label}>
                {s.options.map((o) => (
                  <Pick key={o.key} on={l.robot.parts[s.slot] === o.key} onClick={() => setLook(wearing(l, s.slot, o.key))} look={wearing(l, s.slot, o.key)} made={made} label={o.label} />
                ))}
                {madeParts.filter((m) => m.slot === s.slot).map((m) => (
                  <Pick key={m.key} on={l.robot.parts[s.slot] === m.key} onClick={() => setLook(wearing(l, s.slot, m.key))} look={wearing(l, s.slot, m.key)} made={made} label={m.name} sub="Made by your agent" />
                ))}
              </div>
            ) : null,
          )}

          {shownTab === 'colours' && (
            <div className="look-colours">
              {COLOUR_ROWS.map((row) => {
                const now = l.robot.colours[row.key]
                const choose = (c: string) => setLook({ ...l, robot: { ...l.robot, colours: { ...l.robot.colours, [row.key]: c } } })
                return (
                  <div key={row.key} className="look-row" role="group" aria-label={row.label}>
                    <span>{row.label}</span>
                    {library.colours[row.key].map((c, i) => (
                      <button key={c} type="button" className="swatch" aria-label={`${row.label} ${i + 1}`} aria-pressed={now === c} style={{ background: c }} onClick={() => choose(c)} />
                    ))}
                  </div>
                )
              })}
            </div>
          )}

          <div className="look-ask">
            <div className="look-ask-say">
              <b>Ask your agent for a part, or a whole character</b>
              <span>Say “a party hat”, or “a cat”. Your agent draws it on kelve’s grid, kelve checks it, and it shows up here.</span>
            </div>
            <form
              className="look-ask-form"
              onSubmit={(e) => {
                e.preventDefault()
                void ask()
              }}
            >
              <label htmlFor="look-ask" className="sr-only">What to draw</label>
              <input id="look-ask" className="field" value={asked} placeholder="A party hat" maxLength={200} onChange={(e) => setAsked(e.target.value)} />
              <Btn type="submit" disabled={!asked.trim()}>Ask your agent</Btn>
            </form>
            {going.length > 0 && <ul className="drews">{going.map((d) => <DrawingLine key={d.id} d={d} />)}</ul>}
            {(mine?.made.length ?? 0) > 0 && (
              <div className="look-made" role="group" aria-label="Made by your agent">
                {mine!.made.slice(0, 6).map((m) => {
                  const on = m.slot ? l.character === 'robot' && l.robot.parts[m.slot] === m.key : l.character === m.key
                  return (
                    <button key={m.key} type="button" className="pick made" aria-pressed={on} onClick={() => tryOn(m.key)}>
                      <span className="made-at" aria-hidden="true"><Character size={42} move="still" look={m.slot ? wearing(l, m.slot, m.key) : { ...l, character: m.key }} made={made} label={m.name} /></span>
                      <span><b>{m.name}</b><small>Made by your agent</small></span>
                    </button>
                  )
                })}
              </div>
            )}
          </div>
        </section>
      </main>
      {toast}
    </div>
  )
}
