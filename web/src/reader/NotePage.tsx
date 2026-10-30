// A note as a page of its own (board N-5): a pane beside the work, with who
// it's for and where it's kept along the top, then room to write. Markdown, its
// marks faint as you type; [[an idea]], [[a file]] and a line like ledger.rs:28
// become links, opened with ⌘ or Ctrl held. Typing [[ finds what you mean as
// you type: ideas in the work, its files, and lines that say the words. It's
// kept as you write.

import { useEffect, useRef, useState } from 'react'
import { autocompletion, type Completion, type CompletionContext, type CompletionResult } from '@codemirror/autocomplete'
import { EditorState } from '@codemirror/state'
import { Decoration, EditorView, MatchDecorator, ViewPlugin, placeholder, type ViewUpdate } from '@codemirror/view'
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands'
import { keymap } from '@codemirror/view'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { markdown } from '@codemirror/lang-markdown'
import { tags as t } from '@lezer/highlight'
import { del, get, post } from '../api/client'
import type { Found, Note, NoteIn, Place } from '../api/types'
import { Choice, Quiet } from '../ui/ui'
import { livePreview, livePreviewTheme } from './livePreview'
import { UseLine } from './Margin'
import { places, stateOf, useNotes, whereItGoes } from './NoteParts'

/** How long typing rests before the note is kept. */
const KEEP_AFTER = 700

const markdownLook = syntaxHighlighting(HighlightStyle.define([
  { tag: t.processingInstruction, color: 'var(--mute)' },
  { tag: t.heading1, fontSize: '24px', lineHeight: '34px', fontWeight: '600', color: 'var(--ink)' },
  { tag: [t.heading2, t.heading3, t.heading4], fontWeight: '600', color: 'var(--ink)' },
  { tag: t.emphasis, fontStyle: 'italic' },
  { tag: t.strong, fontWeight: '600', color: 'var(--ink)' },
  { tag: t.monospace, fontFamily: 'var(--mono)', fontSize: '0.88em', backgroundColor: 'var(--well)', borderRadius: '4px', padding: '1px 4px' },
  { tag: [t.link, t.url], color: 'var(--c-def)' },
  { tag: t.quote, color: 'var(--soft)' },
]))

const IDEA = /\[\[([^[\]\n]{1,80})\]\]/g
const LINE = /\b([\w./-]+\.[A-Za-z0-9]{1,8}):(\d+)(?:[–-]\d+)?/g

/** Links in a note: ideas, and lines of files. */
function links(onIdea: (name: string) => void, onLine: (path: string, line: number) => void) {
  const ideas = new MatchDecorator({ regexp: IDEA, decoration: () => Decoration.mark({ class: 'cm-idea-link' }) })
  const lines = new MatchDecorator({ regexp: LINE, decoration: () => Decoration.mark({ class: 'cm-line-ref' }) })
  const plugin = (m: MatchDecorator) =>
    ViewPlugin.fromClass(
      class {
        decorations
        constructor(view: EditorView) {
          this.decorations = m.createDeco(view)
        }
        update(u: ViewUpdate) {
          this.decorations = m.updateDeco(u, this.decorations)
        }
      },
      { decorations: (v) => v.decorations },
    )
  const at = (view: EditorView, pos: number, re: RegExp) => {
    const line = view.state.doc.lineAt(pos)
    for (const found of line.text.matchAll(new RegExp(re.source, 'g'))) {
      const from = line.from + (found.index ?? 0)
      if (pos >= from && pos <= from + found[0].length) return found
    }
    return null
  }
  return [
    plugin(ideas),
    plugin(lines),
    EditorView.domEventHandlers({
      click(event, view) {
        // Writing comes first: a link opens with ⌘ or Ctrl held, as in an editor.
        if (!event.metaKey && !event.ctrlKey) return false
        const pos = view.posAtCoords({ x: event.clientX, y: event.clientY })
        if (pos == null) return false
        const idea = at(view, pos, IDEA)
        if (idea) {
          onIdea(idea[1])
          return true
        }
        const line = at(view, pos, LINE)
        if (line) {
          onLine(line[1], Number(line[2]))
          return true
        }
        return false
      },
    }),
  ]
}

/** What an empty note says, until you write. */
const HINT = 'Write in markdown. Type [[ to link an idea, a file or a line.'

/** Whether a link's words name a file rather than an idea: a path, or a name with an extension. */
export const isFile = (words: string) => words.includes('/') || /\.[A-Za-z0-9]{1,8}$/.test(words)

/**
 * As you type after [[, what you might mean: ideas in the work first, the
 * most used first; then its files; then lines that say the words. Picking
 * one writes its link: [[an idea]], [[a file]], or file:line.
 */
function linkSearch(opened: number) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    const before = ctx.matchBefore(/\[\[[^[\]\n]{0,80}$/)
    if (!before) return null
    const q = before.text.slice(2)
    const from = before.from
    // What's already after the cursor: a closing ]] is replaced, not doubled.
    const after = ctx.state.sliceDoc(ctx.pos, ctx.pos + 2) === ']]' ? 2 : 0
    const sections = { Ideas: { name: 'Ideas', rank: 0 }, Files: { name: 'Files', rank: 1 }, Lines: { name: 'Lines', rank: 2 } }
    const link = (label: string, insert: string, detail: string, section: keyof typeof sections, boost = 0): Completion => ({
      label,
      detail,
      section: sections[section],
      boost,
      apply: (view, _c, _from, to) => view.dispatch({ changes: { from, to: to + after, insert }, selection: { anchor: from + insert.length } }),
    })
    // From two letters, as kelve's search finds things.
    if (q.trim().length < 2) return null
    const found = await get<Found>(`/api/search?q=${encodeURIComponent(q.trim())}&opened=${opened}`).catch(() => null)
    if (!found || ctx.aborted) return null
    const options: Completion[] = [
      ...found.ideas.slice(0, 6).map((i) => link(i.name, `[[${i.name}]]`, i.files ? `idea, in ${i.files} ${i.files === 1 ? 'file' : 'files'}` : 'idea', 'Ideas', 3)),
      ...found.files.slice(0, 6).map((f) => link(f, `[[${f}]]`, 'file', 'Files', 2)),
      ...found.lines.slice(0, 6).map((l) => link(`${l.path}:${l.line}`, `${l.path}:${l.line}`, l.text.trim(), 'Lines', 1)),
    ]
    return options.length ? { from, options, filter: false } : null
  }
}

function Editor({ text, onChange, onIdea, onLine, label, search }: {
  text: string
  /** Finds what a [[ link might mean, as you type. */
  search: (ctx: CompletionContext) => Promise<CompletionResult | null>
  onChange: (text: string) => void
  onIdea: (name: string) => void
  onLine: (path: string, line: number) => void
  label: string
}) {
  const host = useRef<HTMLDivElement>(null)
  const changeRef = useRef(onChange)
  changeRef.current = onChange
  const idea = useRef(onIdea)
  idea.current = onIdea
  const lineRef = useRef(onLine)
  lineRef.current = onLine
  useEffect(() => {
    if (!host.current) return
    const view = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: text,
        extensions: [
          history(),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          markdown(),
          markdownLook,
          livePreview,
          livePreviewTheme,
          EditorView.lineWrapping,
          placeholder(HINT),
          EditorView.contentAttributes.of({ 'aria-label': label, 'aria-multiline': 'true' }),
          links((n) => (isFile(n) ? lineRef.current(n, 1) : idea.current(n)), (p, l) => lineRef.current(p, l)),
          autocompletion({ override: [search], activateOnTyping: true, icons: false, closeOnBlur: true }),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) changeRef.current(u.state.doc.toString())
          }),
        ],
      }),
    })
    if (!text) view.focus()
    return () => view.destroy()
    // One editor for each note: its text is only what it opens with.
  }, [])
  return <div className="md-view" ref={host} />
}

export function NotePage({ id, ideas, opened, kept, close, onIdea, place: startAt }: {
  /** The note, or null for a new one. */
  id: string | null
  /** Who a new note is for, when it's started for someone: your agent, from its list. */
  place?: Place
  /** What a new note is about. */
  ideas?: string[]
  opened: number
  /** Hears the note's id once a new one is kept, or it moves. */
  kept: (id: string) => void
  close: () => void
  /** Opens an idea by its key. */
  onIdea: (key: string) => void
}) {
  const n = useNotes()
  const [note, setNote] = useState<Note | null>(null)
  const [ready, setReady] = useState(!id)
  const [place, setPlace] = useState<Place>(startAt ?? n?.place ?? 'you')
  const [saving, setSaving] = useState<'saved' | 'saving' | null>(null)
  const [said, setSaid] = useState<string | null>(null)
  const [deleting, setDeleting] = useState(false)
  // Bumped to draw the editor afresh, as when a done note is cleared.
  const [drawn, setDrawn] = useState(0)
  const current = useRef<{ id: string | null; text: string }>({ id, text: '' })
  const pending = useRef<number | null>(null)

  useEffect(() => {
    if (!id) return
    get<Note>(`/api/opened/${opened}/note?note=${encodeURIComponent(id)}`).then(
      (got) => {
        setNote(got)
        setPlace(got.place)
        current.current = { id: got.id, text: got.text }
        setReady(true)
      },
      (e: Error) => setSaid(e.message),
    )
    // The page is for one note: a new id is the same note, kept.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const keep = async (to: Place = place) => {
    setSaving('saving')
    try {
      const body: NoteIn = { id: current.current.id, place: to, text: current.current.text, ...(current.current.id ? {} : { ideas: ideas ?? [] }) }
      const got = await post<Note>(`/api/opened/${opened}/notes`, body)
      setNote(got)
      setSaid(null)
      setSaving('saved')
      if (got.id !== current.current.id) {
        current.current.id = got.id
        kept(got.id)
      }
    } catch (e) {
      setSaid((e as Error).message)
      setSaving(null)
    }
  }
  const changed = (text: string) => {
    current.current.text = text
    if (pending.current) window.clearTimeout(pending.current)
    pending.current = window.setTimeout(() => {
      pending.current = null
      void keep()
    }, KEEP_AFTER)
  }
  // Anything unkept is kept as the page closes.
  useEffect(() => () => {
    if (pending.current) {
      window.clearTimeout(pending.current)
      void keep()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const search = linkSearch(opened)
  // A note links an idea by its name, and a line by a file's name: each is found.
  const ideaNamed = async (name: string) => {
    const found = await get<Found>(`/api/search?q=${encodeURIComponent(name)}&opened=${opened}`).catch(() => null)
    const idea = found?.ideas.find((i) => i.name.toLowerCase() === name.toLowerCase()) ?? found?.ideas[0]
    if (idea) onIdea(idea.key)
  }
  const lineIn = async (path: string, line: number) => {
    const name = path.split('/').pop() ?? path
    const found = await get<Found>(`/api/search?q=${encodeURIComponent(name)}&opened=${opened}`).catch(() => null)
    const file = found?.files.find((f) => f === path || f.endsWith(`/${path}`))
    if (file) n?.goTo(file, line)
  }

  // A note your agent has done, cleared for fresh notes: kept, empty, and
  // waiting for your agent again once you write in it.
  const clear = async () => {
    if (pending.current) {
      window.clearTimeout(pending.current)
      pending.current = null
    }
    current.current.text = ''
    await keep()
    setDrawn((d) => d + 1)
  }

  if (!n) return null
  const state = note ? stateOf(note) : null
  return (
    <article className="np" aria-label={note?.title ?? 'A new note'}>
      <div className="np-top">
        <Choice
          small
          label="Who it’s for"
          value={place}
          onChange={(p) => {
            setPlace(p)
            if (current.current.id || current.current.text) void keep(p)
          }}
          options={places(n.agent)}
        />
        {note && <span className="np-file mono" title={note.file}>{note.file.split('/').pop()}</span>}
        <span className="sp" />
        <span className="np-saved" role="status">{saving === 'saving' ? 'Saving' : saving === 'saved' ? 'Saved' : ''}</span>
        <button type="button" className="np-close" aria-label="Close the note" onClick={close}>
          <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="m3 3 6 6M9 3l-6 6" stroke="currentColor" strokeWidth="1.4" /></svg>
        </button>
      </div>
      {(said || !state) && <p className="np-where">{said ?? whereItGoes(place, n)}</p>}
      {note && state && (
        <div className={`np-state${note.state === 'done' ? ' done' : ''}`}>
          <b>{state}</b>
          {note.did && <p><UseLine text={note.did} /></p>}
          <span className="np-state-do">
            {note.opened != null && <a href={`#/o/${note.opened}`}>See what {n.agent} changed</a>}
            {note.state === 'done' && <Quiet onClick={() => void clear()}>Clear it for new notes</Quiet>}
          </span>
        </div>
      )}
      {ready && <Editor key={drawn} text={note?.text ?? ''} onChange={changed} onIdea={(name) => void ideaNamed(name)} onLine={(p, l) => void lineIn(p, l)} label={`${note?.title ?? 'A new note'}, in markdown`} search={search} />}
      {note && (
        <p className="np-delete">
          {deleting ? (
            <>
              Delete this note? It can’t be brought back.{' '}
              <Quiet onClick={() => void del(`/api/opened/${opened}/notes?note=${encodeURIComponent(note.id)}`).then(close)}>Delete it</Quiet>
              {' '}
              <Quiet onClick={() => setDeleting(false)}>Keep it</Quiet>
            </>
          ) : (
            <Quiet onClick={() => setDeleting(true)}>Delete this note</Quiet>
          )}
        </p>
      )}
    </article>
  )
}
