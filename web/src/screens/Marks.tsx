// What kelve marks for you (marks.md): how much overall, how well you know
// each area, ideas always and never marked, and your own words. Your agent
// reads it when it explains an idea, so it knows what's new to you. Kept in
// your notes folder, so it can be written here, by hand, or by your agent: the
// same note. Used in settings, under Reading.

import { useEffect, useState } from 'react'
import { changed, post } from '../api/client'
import type { Level, MarksSpec, MarksView } from '../api/types'
import { Choice } from '../ui/ui'
import { useLoad } from '../useLoad'

export const LEVELS: Array<{ value: Level; label: string }> = [
  { value: 'new', label: 'New' },
  { value: 'learning', label: 'Learning' },
  { value: 'working', label: 'Working' },
  { value: 'deep', label: 'Deep' },
]

/** What to say to your agent, to have it write what kelve marks for you. */
export const WRITE_MARKS = 'Write what kelve marks for me.'

/** What kelve marks for you, kept fresh as it changes here, by hand or by your agent. */
export function useMarks() {
  return useLoad<MarksView>('/api/marks', ['spec'])
}

/** Who wrote it and when, in a line: you, or your agent, by name. */
export function writtenBy(view: MarksView, agent: string): string {
  if (!view.kept) return 'Not written yet: kelve marks every idea but the basics.'
  const who = view.spec.by === 'agent' ? agent : 'you'
  return `Written by ${who}${view.spec.at ? ` on ${new Date(view.spec.at).toLocaleDateString('en-GB', { day: 'numeric', month: 'long' })}` : ''}.`
}

/** Fields of a note, not a form to fill: no password manager offers itself, and nothing fills them in. */
const PLAIN = {
  autoComplete: 'off',
  'data-1p-ignore': true,
  'data-lpignore': 'true',
  'data-bwignore': true,
  'data-form-type': 'other',
} as const

/** A one-line field as a textarea: password managers take a page's first text input for a user name, whatever it says, but leave textareas alone. Enter keeps it one line. */
function OneLine({ onEnter, onChange, ...rest }: Omit<React.TextareaHTMLAttributes<HTMLTextAreaElement>, 'onChange'> & { onChange: (text: string) => void; onEnter?: () => void }) {
  return (
    <textarea
      {...PLAIN}
      {...rest}
      rows={1}
      onChange={(e) => onChange(e.target.value.replace(/\n/g, ' '))}
      onKeyDown={(e) => {
        if (e.key !== 'Enter') return
        e.preventDefault()
        if (onEnter) onEnter()
        else e.currentTarget.blur()
      }}
    />
  )
}

const names = (list: string[]) => list.join(', ')
const unnames = (text: string) =>
  text
    .split(',')
    .map((n) => n.trim())
    .filter(Boolean)

/** The spec, to change in place: each change kept at once, as marks.md. */
export function MarksEditor({ view }: { view: MarksView }) {
  const [spec, setSpec] = useState<MarksSpec>(view.spec)
  const [said, setSaid] = useState<string>()
  const [adding, setAdding] = useState('')
  const [always, setAlways] = useState(names(view.spec.always))
  const [never, setNever] = useState(names(view.spec.never))
  const [words, setWords] = useState(view.spec.words)
  useEffect(() => {
    setSpec(view.spec)
    setAlways(names(view.spec.always))
    setNever(names(view.spec.never))
    setWords(view.spec.words)
  }, [view.spec])
  const keep = (next: MarksSpec) => {
    setSpec(next)
    setSaid(undefined)
    void post<MarksView>('/api/marks', next).then(
      () => changed('spec'),
      (e: Error) => setSaid(e.message),
    )
  }
  const addArea = () => {
    const area = adding.trim().toLowerCase()
    setAdding('')
    if (!area || spec.areas.some((a) => a.area === area)) return
    keep({ ...spec, areas: [...spec.areas, { area, level: 'learning' }] })
  }
  return (
    <div className="marks-edit">
      <p className="lgroup"><b>Overall</b></p>
      <div className="opt">
        <div>
          <b>Mark as if I’m</b>
          <span>New marks every idea, the basics of a language too. Deep marks only what’s particular to the work.</span>
        </div>
        <Choice small label="Mark as if I’m" value={spec.mark} onChange={(mark) => keep({ ...spec, mark })} options={LEVELS} />
      </div>
      <p className="lgroup"><b>By area</b></p>
      {spec.areas.map((a, i) => (
        <div key={a.area} className="opt marks-area">
          <div>
            <b>{a.area}</b>
          </div>
          <span className="row">
            <Choice
              small
              label={a.area}
              value={a.level}
              onChange={(level) => keep({ ...spec, areas: spec.areas.map((x, j) => (j === i ? { ...x, level } : x)) })}
              options={LEVELS}
            />
            <button type="button" className="quiet-link" aria-label={`Take out ${a.area}`} onClick={() => keep({ ...spec, areas: spec.areas.filter((_, j) => j !== i) })}>
              Take out
            </button>
          </span>
        </div>
      ))}
      <label className="marks-add">
        <span aria-hidden="true">+</span>
        <OneLine
          value={adding}
          placeholder="An area: rust, databases, distributed systems"
          aria-label="Add an area"
          onChange={setAdding}
          onEnter={addArea}
          onBlur={addArea}
        />
      </label>
      <p className="lgroup"><b>By idea, and in your words</b></p>
      <label className="opt stack">
        <div>
          <b>Always mark</b>
          <span>Ideas, by name, with commas between them.</span>
        </div>
        <OneLine className="marks-in" value={always} placeholder="None yet" aria-label="Always mark" onChange={setAlways} onBlur={() => names(spec.always) !== always && keep({ ...spec, always: unnames(always) })} />
      </label>
      <label className="opt stack">
        <div>
          <b>Never mark</b>
        </div>
        <OneLine className="marks-in" value={never} placeholder="None yet" aria-label="Never mark" onChange={setNever} onBlur={() => names(spec.never) !== never && keep({ ...spec, never: unnames(never) })} />
      </label>
      <label className="opt stack">
        <div>
          <b>In your words</b>
          <span>Anything else on what you want marked. It goes to your agent as it explains.</span>
        </div>
        <textarea {...PLAIN} className="marks-in" rows={1} value={words} placeholder="Nothing yet" aria-label="In your words" onChange={(e) => setWords(e.target.value)} onBlur={() => spec.words !== words && keep({ ...spec, words })} />
      </label>
      {said && <p className="warn">{said}</p>}
    </div>
  )
}
