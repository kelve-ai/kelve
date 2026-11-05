// Editing a skill by hand (board SkillEdit): its SKILL.md on the left, the
// skill as it reads on the right, as you type. kelve checks it before it's
// saved and says what's wrong in sentences; nothing is written till it's
// right. A part you add or reword isn't checked yet, and the toast offers to
// check it. With no name, it's a new skill of your own.

import { useEffect, useState } from 'react'
import { ApiError, post } from '../api/client'
import type { SkillSaved, SkillView } from '../api/types'
import { Wait } from './SkillsWait'
import { hrefOf } from '../routes'
import { Btn, Kbd, Toast } from '../ui/kit'
import { Markdown } from '../ui/markdown'
import { useLoad } from '../useLoad'

/** A new skill, to write over. */
const NEW = `---
name: my-skill
description: What it’s for. Use when …
---

# My skill

## How you like it

### The rule itself, in a few words
A sentence or two on why.
`

/** The text without its front matter: what the preview shows. */
function body(text: string): string {
  const m = text.match(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/)
  return m ? text.slice(m[0].length) : text
}

const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform)

export function SkillEdit({ name, example }: { name?: string; example?: boolean }) {
  const skill = useLoad<SkillView>(name ? `/api/skills/${encodeURIComponent(name)}` : null, [])
  const [draft, setDraft] = useState<string | null>(null)
  const [saved, setSaved] = useState<string | null>(null)
  const [problems, setProblems] = useState<string[]>([])
  const [done, setDone] = useState<SkillSaved | null>(null)
  useEffect(() => {
    if (!done) return
    const t = window.setTimeout(() => setDone(null), 8000)
    return () => window.clearTimeout(t)
  }, [done])
  if (name && !skill.data) return <Wait error={skill.error} example={example} />
  const initial = name ? (skill.data?.markdown ?? '') : NEW
  const text = draft ?? initial
  const title = name ? (skill.data?.title ?? name) : 'A new skill'
  const back = hrefOf(name ? `skills/${skill.data?.id ?? name}` : 'skills', example)
  const dirty = text !== (saved ?? initial)

  const save = async () => {
    try {
      const r = await post<SkillSaved>(name ? `/api/skills/${encodeURIComponent(name)}` : '/api/skills', { markdown: text })
      if (!r.saved) {
        setProblems(r.problems)
        return
      }
      setProblems([])
      setSaved(text)
      if (!name) location.hash = hrefOf(`skills/${r.id}`, example)
      else setDone(r)
    } catch (e) {
      setProblems([e instanceof ApiError ? e.message : 'Something went wrong. Try again in a moment.'])
    }
  }
  const status = dirty ? 'Not saved yet' : saved != null ? 'Saved' : ''
  const toast =
    done && (
      <Toast
        action={
          done.unchecked > 0
            ? { label: done.unchecked === 1 ? 'Check it' : 'Check them', run: () => (location.hash = `${hrefOf(`skills/${done.id}`, example)}?check`) }
            : undefined
        }
      >
        {done.unchecked === 0 ? 'Saved.' : done.unchecked === 1 ? 'Saved. 1 part isn’t checked yet.' : `Saved. ${done.unchecked} parts aren’t checked yet.`}
      </Toast>
    )

  return (
    <div className="sk-edit">
      <div className="sk-edit-top">
        <div>
          <a className="go sk-back" href={back}>
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="m15 18-6-6 6-6" />
            </svg>
            {name ? title : 'Skills'}
          </a>
          <h1>{name ? `Editing ${title}` : title}</h1>
        </div>
        <div className="sk-acts">
          <span role="status" className={`sk-status${dirty ? ' dirty' : ''}`}>{status}</span>
          <Btn kind="quiet" href={back}>Cancel</Btn>
          <Btn kind="primary" onClick={() => void save()}>
            Save <Kbd>{isMac ? '⌘S' : 'Ctrl S'}</Kbd>
          </Btn>
        </div>
      </div>
      {problems.length > 0 && (
        <div className="sk-problems" role="alert">
          <p>Not saved. Change this first:</p>
          <ul>
            {problems.map((p) => <li key={p}>{p}</li>)}
          </ul>
        </div>
      )}
      <div className="sk-split">
        <div className="sk-pane">
          <div className="sk-plabel">
            <label htmlFor="skill-md">Markdown</label>
            <span id="skill-rule" className="sk-rule">Each <code>###</code> heading is one part. kelve checks each part.</span>
          </div>
          <textarea
            id="skill-md"
            className="sk-src-ed"
            spellCheck={false}
            aria-describedby="skill-rule"
            value={text}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') {
                e.preventDefault()
                void save()
              }
            }}
          />
        </div>
        <div className="sk-pane">
          <span className="sk-plabel">Preview</span>
          <div className="sk-prev" role="region" aria-label="Preview">
            <Markdown text={body(text)} />
          </div>
        </div>
      </div>
      {toast}
    </div>
  )
}
