// Notes (boards N-1 to N-8): a note is just for you, with the work for everyone
// on it, or for your agent to pick up the next time you message it. Here: what
// the reader needs to write and show them; a note under the lines it's on; the
// box that writes one; and the Notes tab.

import { createContext, type ReactNode, useContext, useEffect, useRef, useState } from 'react'
import { noteKey, ordinal, useList, whereIs } from '../agentList'
import { post } from '../api/client'
import type { Note, NoteIn, NoteRef, Place } from '../api/types'
import { Button, Choice, Hint, Quiet, State } from '../ui/ui'
import { roughly } from '../words'
import { UseLine } from './Margin'

/** What notes need from the reader around them. */
export type NotesWith = {
  opened: number
  /** Your agent, as people say it: "Claude Code". */
  agent: string
  /** Who a new note is for, until you say otherwise. */
  place: Place
  /** Where your own notes go: `~/Notes`. */
  notesDir: string
  /** Opens a note as a page, or a new one, about these ideas. */
  page: (id: string | null, ideas?: string[]) => void
  /** Goes to a line of a file. */
  goTo: (path: string, line: number) => void
  /** Whether the hint about notes is still to be shown, once. */
  hint: boolean
  /** The hint's been seen. */
  hinted: () => void
}

export const NotesContext = createContext<NotesWith | null>(null)
export const useNotes = () => useContext(NotesContext)

/** Who a note is for, as the choice says it. */
export function places(agent: string): Array<{ value: Place; label: string }> {
  return [
    { value: 'you', label: 'Just you' },
    { value: 'work', label: 'With the work' },
    { value: 'agent', label: `For ${agent}` },
  ]
}

/** "line 5", "lines 5–9". */
export const linesOf = (on: { start: number; end: number }) => (on.start === on.end ? `line ${on.start}` : `lines ${on.start}–${on.end}`)

/** Where a note goes, in a line. */
export function whereItGoes(place: Place, n: NotesWith): string {
  if (place === 'you') return `Saved in ${n.notesDir}, a folder for this project. Only you see it.`
  if (place === 'work') return 'Saved in the project, in .kelve/notes, so it’s committed with the work.'
  return `${n.agent} gets it the next time you message it, and marks it done when it has.`
}

/** How a note for your agent stands, in a few words. */
export function stateOf(note: NoteRef): string | null {
  // Done before, then changed: what it did then stays under it.
  if (note.state === 'waiting' && note.did) return 'Changed since it was done, so waiting for your next message to it'
  if (note.state === 'waiting') return 'Waiting for your next message to it'
  if (note.state === 'done') return note.doneAt ? `Done ${roughly(note.doneAt)}` : 'Done'
  return null
}

/** A note's words without its markdown marks, for a line of a list. */
export const firstWords = (text: string) => text.replace(/^#+\s*/gm, '').replace(/[*_`[\]]/g, '').trim()

/** The first time lines are chosen, one hint beside the Note button (O-3n). */
export function NotesHint({ top, left }: { top: number; left: number }) {
  const n = useNotes()
  if (!n?.hint) return null
  return (
    <Hint side="left" style={{ top: top - 10, left: left + 74 }} onDone={n.hinted}>
      <b>Select lines to leave a note.</b> Just for you, with the work for everyone on it, or for {n.agent} to pick up the next time you message it.
    </Hint>
  )
}

/** A note under the lines it's on (N-2): who it's for and which lines, then the note. */
export function NoteWell({ note, text }: { note: NoteRef; text?: string }) {
  const n = useNotes()
  if (!n) return null
  const who = places(n.agent).find((p) => p.value === note.place)?.label
  const state = stateOf(note)
  return (
    <button type="button" className={`nwell${note.state === 'done' ? ' done' : ''}`} onClick={() => n.page(note.id)}>
      <small>
        {who}{note.on ? ` · ${linesOf(note.on)}` : ''}{state ? ` · ${state}` : ''}
      </small>
      <span>{text !== undefined ? firstWords(text) || note.title : note.title}</span>
      {note.did && <span className="did"><UseLine text={note.did} /></span>}
    </button>
  )
}

/** Your agent, as a place for a note: the list's square, and where on it the note would go. */
function ForAgent({ agent, place }: { agent: string; place: number | null }): ReactNode {
  return (
    <>
      <State state="queued" />
      <span><span className="sr-only">For </span>{agent}</span>
      {place != null && <i className="at">{ordinal(place)}<span className="sr-only"> on its list</span></i>}
    </>
  )
}

/**
 * The box under selected lines (N-1, Q-4): the note, then one line under
 * it: who it's for, your agent's showing where on its list the note would
 * go, and Keep. A note can open as a page of its own, to write more.
 */
export function NoteBox({ path, start, end, ideas, close }: {
  path: string
  start: number
  end: number
  /** The ideas marked on those lines. */
  ideas: string[]
  close: () => void
}) {
  const n = useNotes()
  const [text, setText] = useState('')
  const [place, setPlace] = useState<Place>(n?.place ?? 'you')
  const [said, setSaid] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const area = useRef<HTMLTextAreaElement>(null)
  const list = useList()
  // Ready to type: once it's drawn into the file, which can be a frame after it's made.
  useEffect(() => {
    const focus = () => area.current?.focus({ preventScroll: true })
    focus()
    const later = requestAnimationFrame(focus)
    return () => cancelAnimationFrame(later)
  }, [])
  if (!n) return null
  const save = async (thenPage = false) => {
    setSaving(true)
    try {
      const kept = await post<Note>(`/api/opened/${n.opened}/notes`, {
        id: null,
        place,
        text,
        on: { path, start, end },
        ideas,
      } satisfies NoteIn)
      close()
      if (thenPage) n.page(kept.id)
    } catch (e) {
      setSaid((e as Error).message)
      setSaving(false)
    }
  }
  return (
    <form
      className="nbox"
      onSubmit={(e) => {
        e.preventDefault()
        void save()
      }}
      onKeyDown={(e) => {
        if (e.key === 'Escape') close()
        if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) void save()
        e.stopPropagation()
      }}
    >
      <label className="sr-only" htmlFor={`note-${path}-${start}`}>A note on {linesOf({ start, end })}</label>
      <textarea id={`note-${path}-${start}`} ref={area} rows={3} value={text} placeholder={`A note on ${linesOf({ start, end })}`} onChange={(e) => setText(e.target.value)} />
      {(said || place !== 'agent') && <p className="where">{said ?? whereItGoes(place, n)}</p>}
      <div className="foot">
        <Choice
          small
          label="Who it’s for"
          value={place}
          onChange={setPlace}
          options={places(n.agent).map((o) => (o.value === 'agent' ? { ...o, label: <ForAgent agent={n.agent} place={list ? list.now.length + list.next.length + 1 : null} /> } : o))}
        />
        <span className="sp" />
        <Quiet onClick={() => void save(true)}>Open as a page</Quiet>
        <Quiet onClick={close}>Cancel</Quiet>
        <Button small type="submit" disabled={saving || !text.trim()}>Keep</Button>
      </div>
    </form>
  )
}

/** Where a note is, in a few words: `retry.rs` 22–24, or a page. */
function whereNote(note: NoteRef): ReactNode {
  if (!note.on) return 'A page'
  const lines = note.on.start === note.on.end ? `${note.on.start}` : `${note.on.start}–${note.on.end}`
  return <><code>{note.on.path.split('/').pop()}</code> {lines}{note.on.found ? '' : ', its lines changed since'}</>
}

/**
 * The Notes tab (N-3, L-2): for your agent, with the work, just you, each
 * with how many. A note for your agent carries the list's square: hollow
 * while it waits, red while it's being done, filled once done; the rest a
 * quiet dash.
 */
export function NotesTab({ notes, filter }: { notes: NoteRef[]; filter: string }) {
  const n = useNotes()
  const list = useList()
  if (!n) return null
  const q = filter.trim().toLowerCase()
  const shown = notes.filter((x) => !q || x.title.toLowerCase().includes(q) || x.on?.path.toLowerCase().includes(q))
  const groups = (['agent', 'work', 'you'] as Place[])
    .map((p) => ({ place: p, label: places(n.agent).find((x) => x.value === p)!.label, notes: shown.filter((x) => x.place === p) }))
    .filter((g) => g.notes.length)
  const open = (note: NoteRef) => {
    if (note.on?.found) n.goTo(note.on.path, note.on.start)
    n.page(note.id)
  }
  const row = (note: NoteRef) => {
    if (note.place !== 'agent') {
      return (
        <button type="button" className="lrow top" onClick={() => open(note)}>
          <State state="dash" />
          <span className="lt wrap">{note.title}</span>
          <small>{whereNote(note)}</small>
        </button>
      )
    }
    const at = whereIs(list, noteKey(note.id))
    const state = note.state === 'done' ? 'fin' : at?.state === 'now' ? 'now' : 'wait'
    const stands =
      state === 'fin' ? (note.doneAt ? `done ${roughly(note.doneAt)}` : 'done')
      : state === 'now' ? 'being done'
      : at?.state === 'next' ? `${ordinal(at.place)} on the list`
      : 'waiting'
    return (
      <button type="button" className="lrow top" onClick={() => open(note)}>
        <State state={state === 'fin' ? 'done' : state === 'now' ? 'now' : 'queued'} label={state === 'fin' ? 'Done' : state === 'now' ? 'Being done' : 'Waiting'} />
        <span className="lt wrap">{note.title}</span>
        <small>{whereNote(note)} · {stands}</small>
      </button>
    )
  }
  return (
    <div className="side-list notes-tab">
      {!groups.length && (
        <p className="side-none">
          {q ? 'No note goes by that name.' : 'No notes yet. Select lines to leave one, or start a page.'}
        </p>
      )}
      {groups.map((g) => (
        <section key={g.place} aria-label={g.label}>
          <p className="lgroup"><b>{g.label}</b><span>{g.notes.length}</span></p>
          <ul>
            {g.notes.map((note) => <li key={note.id}>{row(note)}</li>)}
          </ul>
        </section>
      ))}
      <button type="button" className="ladd" onClick={() => n.page(null)}>New note</button>
    </div>
  )
}
