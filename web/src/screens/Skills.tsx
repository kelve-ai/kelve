// Your skills (boards Skills, SkillsEmpty, SkillSuggested, CheckConfirm and
// CheckRunning): the list down the side in its four groups, and one skill
// beside it, rendered with a note beside each part's heading, or as its
// SKILL.md with each part's mark beside its `###` line. A part is a `###`
// heading and everything under it. Under a part that needs it: what its check
// found, and what you can do. Checks run as settings say: when you agree,
// with what they take in front of you, or by themselves; never past the
// week's runs, and not at all when they're off, which the page says where a
// check would be offered. Editing is SkillEdit.tsx; Fits you is Fits.tsx.

import { type ReactNode, useEffect, useState } from 'react'
import { ApiError, post } from '../api/client'
import type { CheckRun, PartCheck, SkillChanged, SkillItem, SkillPartView, SkillSaved, SkillView, SkillsView, SuggestedSkill } from '../api/types'
import { Wait } from './SkillsWait'
import { hrefOf, type Route } from '../routes'
import { Character } from '../ui/Character'
import { Btn, Dialog, Segmented, Toast } from '../ui/kit'
import { Markdown } from '../ui/markdown'
import { useLoad } from '../useLoad'
import { DO_WAITING } from '../agentList'
import { Fits } from './Fits'
import { SkillEdit } from './SkillEdit'

export type SkillsRoute = Extract<Route, { screen: 'skills' | 'skill' | 'skill-edit' | 'skill-new' | 'skill-suggested' | 'fits' }>

const OWN: Route['screen'][] = ['skills', 'skill', 'skill-edit', 'skill-new', 'skill-suggested', 'fits']

/** Whether a screen is one of these. */
export const isSkillsRoute = (route: Route): route is SkillsRoute => OWN.includes(route.screen)

export function SkillsScreens({ route, example }: { route: SkillsRoute; example?: boolean }) {
  switch (route.screen) {
    case 'skill-edit':
      return <SkillEdit name={route.name} example={example} />
    case 'skill-new':
      return <SkillEdit example={example} />
    case 'fits':
      return <Fits example={example} />
    default:
      return <SkillsPage route={route} example={example} />
  }
}

/** Whether the address asks for something: `?check` opens the check's dialog. */
function asked(name: string): boolean {
  return new URLSearchParams(location.hash.split('?')[1] ?? '').has(name)
}

/** The words an error says, or a plain apology. */
const said = (e: unknown) => (e instanceof ApiError ? e.message : 'Something went wrong. Try again in a moment.')

/** A toast, with Undo when there's a way back; it goes by itself. */
type Said = { text: string; undo?: () => Promise<unknown> }

function useSaid(): [ReactNode, (s: Said | null) => void] {
  const [now, setNow] = useState<Said | null>(null)
  useEffect(() => {
    if (!now) return
    const t = window.setTimeout(() => setNow(null), 8000)
    return () => window.clearTimeout(t)
  }, [now])
  const node = now ? (
    <Toast
      action={
        now.undo
          ? {
              label: 'Undo',
              run: () => {
                const undo = now.undo
                setNow(null)
                void undo?.().catch((e) => setNow({ text: said(e) }))
              },
            }
          : undefined
      }
    >
      {now.text}
    </Toast>
  ) : null
  return [node, setNow]
}

function SkillsPage({ route, example }: { route: Extract<SkillsRoute, { screen: 'skills' | 'skill' | 'skill-suggested' }>; example?: boolean }) {
  const list = useLoad<SkillsView>('/api/skills', ['skills', 'asks'])
  if (!list.data) return <Wait error={list.error} example={example} />
  const all = [...list.data.like, ...list.data.doing, ...list.data.project]
  if (route.screen === 'skills' && all.length === 0 && list.data.suggested.length === 0) return <Empty example={example} />
  // The skills page opens on your first skill, or what kelve suggests.
  const suggested = route.screen === 'skill-suggested' ? route.id : route.screen === 'skills' && all.length === 0 ? list.data.suggested[0]?.id : undefined
  const current = route.screen === 'skill' ? route.name : suggested ? `suggested/${suggested}` : (all[0]?.id ?? '')
  return (
    <div className="sk">
      <Side list={list.data} current={current} example={example} />
      {suggested ? <Suggested key={suggested} id={suggested} example={example} /> : <SkillDoc key={current} id={current} example={example} />}
    </div>
  )
}

/** No skills yet: where they come from. */
function Empty({ example }: { example?: boolean }) {
  return (
    <div className="sk-empty">
      <Character size={120} move="idle" />
      <h1>No skills yet</h1>
      <p>Tick “Do this every time” in a debrief, and it becomes a skill.</p>
      <div className="sk-acts">
        <Btn kind="primary" size="large" href={hrefOf('', example)}>Read your first debrief</Btn>
        <Btn kind="quiet" size="large" href={hrefOf('skills/new', example)}>Write one yourself</Btn>
      </div>
    </div>
  )
}

/** Your skills down the side, in their groups. */
function Side({ list, current, example }: { list: SkillsView; current: string; example?: boolean }) {
  const groups: Array<[string, SkillItem[]]> = [
    ['How you like it', list.like],
    ['How you do something', list.doing],
    ['One project', list.project],
  ]
  return (
    <nav className="sk-side" aria-label="Your skills">
      {list.suggested.length > 0 && (
        <div className="sk-group">
          {list.suggested.map((s) => (
            <a key={s.id} className="sk-item" href={hrefOf(`skills/suggested/${s.id}`, example)} aria-current={current === `suggested/${s.id}` ? 'page' : undefined}>
              <span>{s.title}</span>
              <span className="sk-meta red">New</span>
            </a>
          ))}
        </div>
      )}
      {groups
        .filter(([, items]) => items.length > 0)
        .map(([name, items]) => (
          <div key={name} className="sk-group">
            <span className="sk-gh">{name}</span>
            {items.map((s) => (
              <a key={s.id} className={`sk-item${s.kind === 'project' ? ' mono' : ''}`} href={hrefOf(`skills/${s.id}`, example)} aria-current={current === s.id ? 'page' : undefined}>
                <span>{s.title}</span>
                {s.checking ? (
                  <span className="sk-meta sk-busy"><i className="sk-dot sk-dot-spin" aria-hidden="true" />Checking</span>
                ) : s.waiting ? (
                  <span className="sk-meta">Waiting</span>
                ) : s.failed > 0 ? (
                  <span className="sk-meta red">{s.failed} failed</span>
                ) : null}
              </a>
            ))}
          </div>
        ))}
      <Btn kind="quiet" size="small" href={hrefOf('skills/new', example)}>New skill</Btn>
    </nav>
  )
}

/** What each state of a part is called. */
const LABEL: Record<PartCheck, string> = {
  notChecked: 'Not checked yet',
  waiting: 'Waiting for your agent',
  checking: 'Checking now',
  works: 'Works',
  noDifference: 'Makes no difference',
  failed: 'Failed',
}

/** A part's state: a dot, and its word. */
export function PartState({ check, hide }: { check: PartCheck; hide?: boolean }) {
  return (
    <span className={`sk-st ${check}`}>
      <i className={`sk-dot${check === 'checking' ? ' sk-dot-spin' : ''}`} aria-hidden="true" />
      <span className={hide ? 'sk-stw' : undefined}>{LABEL[check]}</span>
    </span>
  )
}

/** The line under a part's state: what its check found, or how far it's got. */
function hintOf(part: SkillPartView): string {
  if (part.check === 'waiting') return part.progress?.done ? `${part.progress.done} of ${part.progress.of} past ${part.progress.of === 1 ? 'task' : 'tasks'} redone` : ''
  if (part.check === 'checking') return part.progress ? `${part.progress.done} of ${part.progress.of} past ${part.progress.of === 1 ? 'task' : 'tasks'} redone` : ''
  if (part.check === 'noDifference' && part.kept) return 'You kept it anyway'
  return part.hint
}

/** A piece of a skill's text: a part, or what's between parts, shown but never checked. */
type Chunk = { part?: SkillPartView; text: string }

/**
 * A skill's body cut into its parts and what's between them, by the lines
 * each part is on. Its front matter, title and the lines under it are left
 * out: the page's heading shows them.
 */
export function chunks(markdown: string, parts: SkillPartView[]): Chunk[] {
  const lines = markdown.split('\n')
  let at = 0
  if (lines[0]?.trim() === '---') {
    const end = lines.findIndex((l, i) => i > 0 && l.trim() === '---')
    at = end < 0 ? 0 : end + 1
  }
  let code = false
  for (; at < lines.length; at++) {
    if (/^\s*(```|~~~)/.test(lines[at])) code = !code
    if (!code && /^#{2,3}\s/.test(lines[at])) break
  }
  const out: Chunk[] = []
  const other = (from: number, to: number) => {
    const text = lines.slice(from, to).join('\n')
    if (text.trim()) out.push({ text })
  }
  for (const part of [...parts].sort((a, b) => a.line - b.line)) {
    const start = part.line - 1
    if (start < at) continue
    other(at, start)
    out.push({ part, text: part.markdown })
    at = start + part.markdown.split('\n').length
  }
  other(at, lines.length)
  return out
}

function SkillDoc({ id, example }: { id: string; example?: boolean }) {
  const at = `/api/skills/${encodeURIComponent(id)}`
  const skill = useLoad<SkillView>(id ? at : null, ['skills', 'asks'])
  const [view, setView] = useState<'doc' | 'md'>('doc')
  const [confirm, setConfirm] = useState(() => asked('check'))
  const [toast, say] = useSaid()
  const [editing, setEditing] = useState<{ key: string; draft: string } | null>(null)
  const s = skill.data
  if (!id) return <Wait error="There’s no skill here." example={example} />
  if (!s) return <Wait error={skill.error} example={example} />
  const parts = s.sections.flatMap((sec) => sec.parts)
  // A part a check can take now: none when checks are off or the week's runs are used.
  const offered = (part: SkillPartView) => s.offer.parts.some((o) => o.heading === part.heading)

  const run = async (go: () => Promise<Said>) => {
    try {
      say(await go())
    } catch (e) {
      say({ text: said(e) })
    }
    skill.reload()
  }
  const undoOf = (c: SkillChanged): Said['undo'] =>
    c.before != null && c.after != null ? () => post(`${at}/undo`, { before: c.before, after: c.after }).then(() => skill.reload()) : undefined
  const act = (part: SkillPartView, what: 'keep' | 'wording' | 'own' | 'remove', markdown?: string) =>
    run(async () => {
      const c = await post<SkillChanged>(`${at}/part`, { key: part.key, act: what, markdown })
      setEditing(null)
      const undo = what === 'keep' ? () => post(`${at}/part`, { key: part.key, act: 'unkeep' }).then(() => skill.reload()) : undoOf(c)
      return { text: c.said, undo }
    })
  const stop = () => run(async () => ({ text: (await post<SkillChanged>(`${at}/check/stop`)).said }))

  const note = (part: SkillPartView) => (
    <div className="sk-note">
      <PartState check={part.check} />
      {hintOf(part) && <span className="sk-hint">{hintOf(part)}</span>}
      {part.check === 'notChecked' && !editing && offered(part) && (
        <button type="button" className="go" onClick={() => setConfirm(true)}>Check it</button>
      )}
      {((part.check === 'noDifference' && !part.kept) || part.check === 'failed') && (
        <button type="button" className="go" onClick={() => void act(part, 'remove')}>Remove</button>
      )}
      {part.check === 'noDifference' && !part.kept && (
        <button type="button" className="go quiet" onClick={() => void act(part, 'keep')}>Keep it anyway</button>
      )}
    </div>
  )

  return (
    <div className="sk-doc">
      <div className="sk-row sk-head">
        <div>
          <h1 className={s.kind === 'project' ? 'mono' : undefined}>{s.title}</h1>
          {s.intro && <p className="sk-intro">{s.intro}</p>}
          <div className="sk-tools">
            <Segmented
              label="Show the skill as"
              value={view}
              onChange={setView}
              options={[
                { value: 'doc', label: 'Rendered' },
                { value: 'md', label: 'Markdown' },
              ]}
            />
            <Btn size="small" href={hrefOf(`skills/${s.id}/edit`, example)}>Edit</Btn>
          </div>
          {s.offer.held && (
            <p className="sk-held" role="note">
              {s.offer.held} <a href={hrefOf('settings/checks', example)}>Open settings</a>
            </p>
          )}
        </div>
        <div>{s.running && <Running run={s.running} onStop={stop} />}</div>
      </div>

      {s.problems.length > 0 && (
        <div className="sk-problems" role="note">
          <p>kelve can’t check this skill as it is:</p>
          <ul>
            {s.problems.map((p) => <li key={p}>{p}</li>)}
          </ul>
        </div>
      )}

      {view === 'doc' ? (
        <div className="sk-body">
          {chunks(s.markdown, parts).map((c, i) => {
            const part = c.part
            if (!part)
              return (
                <div key={i} className="sk-row">
                  <Markdown text={c.text} />
                  <div />
                </div>
              )
            if (editing?.key === part.key)
              return (
                <div key={part.key} className="sk-row sk-part">
                  <div className="sk-own">
                    <label>
                      Your wording
                      <textarea className="sk-ed" rows={4} value={editing.draft} onChange={(e) => setEditing({ key: part.key, draft: e.target.value })} autoFocus />
                    </label>
                    <div className="sk-acts">
                      <Btn kind="primary" size="small" onClick={() => void act(part, 'own', editing.draft)}>Save</Btn>
                      <Btn kind="quiet" size="small" onClick={() => setEditing(null)}>Cancel</Btn>
                    </div>
                  </div>
                  {note(part)}
                </div>
              )
            return (
              <div key={part.key} className="sk-row sk-part">
                <div className="sk-part-l">
                  <Markdown text={part.markdown} />
                  {part.check === 'failed' && part.failed && (
                    <div className="sk-fix">
                      <Markdown text={part.failed.what} size="small" />
                      <Markdown text={part.failed.wording} />
                      <div className="sk-acts">
                        <Btn kind="primary" size="small" onClick={() => void act(part, 'wording')}>Use this wording</Btn>
                        <Btn kind="quiet" size="small" onClick={() => setEditing({ key: part.key, draft: part.markdown })}>Write my own</Btn>
                      </div>
                    </div>
                  )}
                </div>
                {note(part)}
              </div>
            )
          })}
        </div>
      ) : (
        <Source markdown={s.markdown} parts={parts} />
      )}

      {confirm && <CheckDialog skill={s} onClose={() => setConfirm(false)} onDone={(r) => { setConfirm(false); skill.reload(); say(r) }} at={at} />}
      {toast}
    </div>
  )
}

/** A check under way, or only asked for, beside the skill's heading. */
function Running({ run, onStop }: { run: CheckRun; onStop: () => void }) {
  const what = run.parts === 1 ? 'a new part' : `${run.parts} new parts`
  if (run.waiting) {
    return (
      <div className="sk-note sk-running">
        <Character size={56} move="idle" />
        <span className="sk-st waiting" role="status">
          <i className="sk-dot" aria-hidden="true" />
          Waiting for your agent
        </span>
        <span className="sk-hint">
          {run.done > 0 && `${run.done} of ${run.runs} runs done. `}
          {run.agent
            ? `It checks ${what} next time it does kelve’s jobs. To start now, tell it: ${DO_WAITING}`
            : `No agent is connected, so nothing’s started. Connect one in Settings, and it checks ${what} when it does kelve’s jobs.`}
        </span>
        <Btn size="small" onClick={onStop}>Stop the check</Btn>
      </div>
    )
  }
  return (
    <div className="sk-note sk-running">
      <Character size={56} move="work" />
      <span className="sk-st checking" role="status">
        <i className="sk-dot sk-dot-spin" aria-hidden="true" />
        Checking {what}
      </span>
      <span className="sk-hint">
        {run.done} of {run.runs} runs done. You can close this. kelve tells you when it’s done.
      </span>
      <Btn size="small" onClick={onStop}>Stop the check</Btn>
    </div>
  )
}

/** Checking a skill's new parts: what it takes, before it starts. */
function CheckDialog({ skill, at, onClose, onDone }: { skill: SkillView; at: string; onClose: () => void; onDone: (s: Said) => void }) {
  const [error, setError] = useState('')
  const { parts, tasks, runs, held, when } = skill.offer
  if (parts.length === 0 && held)
    return (
      <Dialog title={when === 'never' ? 'Checks are off' : 'This week’s check runs are used'} onClose={onClose} actions={<Btn kind="primary" onClick={onClose}>Close</Btn>}>
        <p>{held}</p>
      </Dialog>
    )
  if (parts.length === 0)
    return (
      <Dialog title="Every part is checked" onClose={onClose} actions={<Btn kind="primary" onClick={onClose}>Close</Btn>}>
        <p>Change a part, or add one, and kelve can check it.</p>
      </Dialog>
    )
  if (tasks === 0)
    return (
      <Dialog title="No past work to check on yet" onClose={onClose} actions={<Btn kind="primary" onClick={onClose}>Close</Btn>}>
        <p>Once your agent hands over work and you keep it, kelve can check each part on it.</p>
      </Dialog>
    )
  const one = parts.length === 1
  const start = async () => {
    try {
      await post(`${at}/check`, {})
      onDone({ text: one ? 'Asked. Your agent checks it next time it does kelve’s jobs.' : 'Asked. Your agent checks them next time it does kelve’s jobs.' })
    } catch (e) {
      setError(said(e))
    }
  }
  return (
    <Dialog
      title={one ? `Check a new part of ${skill.title}?` : `Check ${parts.length} new parts of ${skill.title}?`}
      onClose={onClose}
      actions={
        <>
          <Btn kind="quiet" onClick={onClose}>Not now</Btn>
          <Btn kind="primary" onClick={() => void start()}>{one ? 'Check it' : 'Check them'}</Btn>
        </>
      }
    >
      <p>
        Your agent redoes {tasks === 1 ? 'a past task' : `${tasks} past tasks`}, with and without {one ? 'the part' : 'each part'}.
      </p>
      <ul className="sk-offer">
        {parts.map((p) => (
          <li key={p.key}>
            <i className="sk-dot" aria-hidden="true" />
            {p.heading}
          </li>
        ))}
      </ul>
      <p>About {runs} runs of your agent. Your own work isn’t touched.</p>
      {held && <p>{held}</p>}
      {error && <p className="sk-error" role="alert">{error}</p>}
    </Dialog>
  )
}

/** One line of SKILL.md, coloured as kelve colours markdown. */
type Paint = { fm: number; code: boolean }

function inlineSpans(text: string, key: string): ReactNode[] {
  const out: ReactNode[] = []
  const re = /(\*\*[^*]+\*\*)|(`[^`]+`)|(\[[^\]]+\]\([^)]*\))/g
  let last = 0
  let m: RegExpExecArray | null
  let n = 0
  while ((m = re.exec(text))) {
    if (m.index > last) out.push(text.slice(last, m.index))
    const cls = m[1] ? 'md-b' : m[2] ? 'md-c' : 'md-a'
    out.push(<span key={`${key}.${n++}`} className={cls}>{m[0]}</span>)
    last = m.index + m[0].length
  }
  if (last < text.length) out.push(text.slice(last))
  return out
}

function paint(line: string, ctx: Paint, key: string): ReactNode {
  if (line.trim() === '---' && ctx.fm < 2 && !ctx.code) {
    ctx.fm += 1
    return <span className="md-q">{line}</span>
  }
  if (ctx.fm === 1) {
    const k = line.indexOf(':')
    if (k < 0) return line
    return (
      <>
        <span className="md-k">{line.slice(0, k + 1)}</span>
        {line.slice(k + 1)}
      </>
    )
  }
  if (/^\s*(```|~~~)/.test(line)) {
    ctx.code = !ctx.code
    return <span className="md-q">{line}</span>
  }
  if (ctx.code) return <span className="md-code">{line}</span>
  const h = line.match(/^(#{1,6} )(.*)$/)
  if (h)
    return (
      <>
        <span className="md-hm">{h[1]}</span>
        <span className="md-h">{h[2]}</span>
      </>
    )
  const li = line.match(/^(\s*(?:[-*] |\d+\. ))(.*)$/)
  if (li)
    return (
      <>
        <span className="md-q">{li[1]}</span>
        {inlineSpans(li[2], key)}
      </>
    )
  return inlineSpans(line, key)
}

/** SKILL.md as it is, with each part's mark beside its `###` line, and only there. */
function Source({ markdown, parts }: { markdown: string; parts: SkillPartView[] }) {
  const ctx: Paint = { fm: 0, code: false }
  const by = new Map(parts.map((p) => [p.line, p]))
  return (
    <div className="sk-src" role="region" aria-label="SKILL.md">
      {markdown
        .replace(/\n$/, '')
        .split('\n')
        .map((line, i) => {
          const part = by.get(i + 1)
          return (
            <div key={i} className="sk-ln">
              <span className="sk-n" aria-hidden="true">{i + 1}</span>
              <span className="sk-l">{paint(line, ctx, String(i))}</span>
              <span className="sk-mk">{part && <PartState check={part.check} hide />}</span>
            </div>
          )
        })}
    </div>
  )
}

/** A skill kelve suggests: in full, made only if you say so. */
function Suggested({ id, example }: { id: string; example?: boolean }) {
  const s = useLoad<SuggestedSkill>(`/api/suggested/${encodeURIComponent(id)}`, ['skills'])
  const [error, setError] = useState('')
  const [problems, setProblems] = useState<string[]>([])
  if (!s.data) return <Wait error={s.error} example={example} />
  const d = s.data
  const decide = async (act: 'create' | 'add' | 'not_now') => {
    try {
      const r = await post<SkillSaved>(`/api/suggested/${encodeURIComponent(id)}`, { act, to: act === 'add' ? d.closeTo?.id : undefined })
      if (!r.saved) return setProblems(r.problems)
      location.hash = hrefOf(r.id ? `skills/${r.id}` : 'skills', example)
    } catch (e) {
      setError(said(e))
    }
  }
  return (
    <div className="sk-doc">
      <div className="sk-row sk-head">
        <div>
          <h1>{d.title}</h1>
          <p className="sk-intro">{d.why ? `${d.why} ` : ''}Nothing changes until you choose.</p>
          <div className="sk-acts">
            <Btn kind="primary" onClick={() => void decide('create')}>Create the skill</Btn>
            {d.closeTo && <Btn onClick={() => void decide('add')}>Add to {d.closeTo.title} instead</Btn>}
            <Btn kind="quiet" onClick={() => void decide('not_now')}>Not now</Btn>
          </div>
          {error && <p className="sk-error" role="alert">{error}</p>}
          {problems.length > 0 && (
            <ul className="sk-error" role="alert">
              {problems.map((p) => <li key={p}>{p}</li>)}
            </ul>
          )}
        </div>
        <div />
      </div>
      <div className="sk-body">
        {chunks(d.markdown, d.sections.flatMap((sec) => sec.parts)).map((c, i) =>
          c.part ? (
            <div key={c.part.key} className="sk-row sk-part">
              <Markdown text={c.text} />
              <div className="sk-note">
                <PartState check="notChecked" />
              </div>
            </div>
          ) : (
            <div key={i} className="sk-row">
              <Markdown text={c.text} />
              <div />
            </div>
          ),
        )}
      </div>
    </div>
  )
}
