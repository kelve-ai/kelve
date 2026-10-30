// Code as kelve shows it, through CodeMirror: its languages; quiet
// highlighting, where only strings, constants, comments and definitions have
// colours; the gutter's squares, one on each idea's first line, with a stem
// down to a foot on its last; and a change's lines, with what's as before
// folded away.

import { type EditorState, type Extension, RangeSetBuilder, StateEffect, StateField, type Text } from '@codemirror/state'
import { Decoration, type DecorationSet, EditorView, GutterMarker, gutter, WidgetType } from '@codemirror/view'
import { ensureSyntaxTree, HighlightStyle, StreamLanguage, type StreamParser, syntaxHighlighting } from '@codemirror/language'
import { tags as t } from '@lezer/highlight'
import { rust } from '@codemirror/lang-rust'
import { javascript } from '@codemirror/lang-javascript'
import { python } from '@codemirror/lang-python'
import { go } from '@codemirror/lang-go'
import { markdown } from '@codemirror/lang-markdown'
import { c, cpp, csharp, java, kotlin } from '@codemirror/legacy-modes/mode/clike'
import { css } from '@codemirror/legacy-modes/mode/css'
import { diff } from '@codemirror/legacy-modes/mode/diff'
import { dockerFile } from '@codemirror/legacy-modes/mode/dockerfile'
import { json } from '@codemirror/legacy-modes/mode/javascript'
import { ruby } from '@codemirror/legacy-modes/mode/ruby'
import { shell } from '@codemirror/legacy-modes/mode/shell'
import { standardSQL } from '@codemirror/legacy-modes/mode/sql'
import { swift } from '@codemirror/legacy-modes/mode/swift'
import { toml } from '@codemirror/legacy-modes/mode/toml'
import { xml } from '@codemirror/legacy-modes/mode/xml'
import { yaml } from '@codemirror/legacy-modes/mode/yaml'
import type { Marked } from '../api/types'
import { type Seen, squareLabel } from './Square'

/** The languages kelve names with a simpler grammar, read line by line. */
const SIMPLER: Record<string, StreamParser<unknown>> = {
  ruby, java, kotlin, swift, c, cpp, csharp, css, json, yaml, xml, diff,
  bash: shell,
  sql: standardSQL,
  ini: toml,
  dockerfile: dockerFile,
}

const defined = new Map<string, StreamLanguage<unknown>>()

/** The grammar for a language kelve names (`core::code::language`). */
/** A state with its code parsed, as far as a few milliseconds go, so it's
 * coloured from the first paint rather than a moment after. */
export function parsed(state: EditorState): EditorState {
  ensureSyntaxTree(state, state.doc.length, 50)
  return state
}

export function grammar(lang: string | null | undefined): Extension {
  switch (lang) {
    case 'rust': return rust()
    case 'typescript': return javascript({ typescript: true, jsx: true })
    case 'javascript': return javascript({ jsx: true })
    case 'python': return python()
    case 'go': return go()
    case 'markdown': return markdown()
    default: {
      const simpler = lang ? SIMPLER[lang] : undefined
      if (!simpler || !lang) return []
      let language = defined.get(lang)
      if (!language) defined.set(lang, (language = StreamLanguage.define(simpler)))
      return language
    }
  }
}

/**
 * Highlighting. Full, the default: keywords, types, properties and calls
 * have colours too, as an IDE colours them. Quiet: what you scan for has a
 * colour, and nothing else; keywords are plain, comments stand out a
 * little. No bold or italic either way.
 */
export const colours = syntaxHighlighting(HighlightStyle.define([
  { tag: [t.string, t.special(t.string), t.regexp, t.character, t.inserted], color: 'var(--c-string)' },
  { tag: [t.number, t.bool, t.null, t.atom, t.constant(t.variableName), t.constant(t.name)], color: 'var(--c-const)' },
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], color: 'var(--c-comment)' },
  {
    tag: [t.definition(t.variableName), t.function(t.definition(t.variableName)), t.definition(t.typeName), t.definition(t.propertyName), t.definition(t.className), t.macroName],
    color: 'var(--c-def)',
  },
  { tag: t.heading, color: 'var(--ink)' },
  { tag: t.link, color: 'var(--c-def)' },
  // Full highlighting only: in quiet, these are the code's own colour.
  { tag: [t.keyword, t.controlKeyword, t.operatorKeyword, t.definitionKeyword, t.moduleKeyword, t.modifier, t.self], color: 'var(--hl-keyword)' },
  { tag: [t.typeName, t.className, t.namespace], color: 'var(--hl-type)' },
  { tag: [t.propertyName, t.attributeName, t.labelName], color: 'var(--hl-attr)' },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: 'var(--hl-call)' },
  { tag: [t.operator, t.punctuation, t.bracket], color: 'var(--hl-punct)' },
]))

/** A column, counted in characters as kelve counts it, as an offset in a
 * line as JavaScript counts it. */
export function offset(line: string, chars: number): number {
  return Array.from(line).slice(0, chars).join('').length
}

/** A mark, on the document's lines: its first and last. */
export type Placed = { a: number; b: number; mark: Marked }

/**
 * Where each mark sits in a document, by line. `lineOf` turns a line of the
 * file into a line of the document, for a change, whose document has the
 * lines it took out too; a mark on a line it doesn't show isn't placed.
 */
export function place(doc: Text, marks: Marked[], lineOf: (n: number) => number | undefined = (n) => n): Placed[] {
  const out: Placed[] = []
  for (const m of marks) {
    const a = lineOf(m.mark.span?.line ?? m.mark.start)
    const b = lineOf(m.mark.span?.line ?? m.mark.end) ?? a
    if (!a || !b || b > doc.lines) continue
    out.push({ a, b: Math.max(a, b), mark: m })
  }
  return out.sort((x, y) => x.a - y.a || x.b - y.b)
}

/** What the gutter draws on one line: a square, the stem through it, its foot. */
class Mark extends GutterMarker {
  constructor(
    readonly square: Placed | null,
    readonly stem: 'down' | 'through' | 'end' | null,
    readonly red: boolean,
    readonly read: boolean,
    readonly label: string,
  ) {
    super()
  }
  eq(o: Mark) {
    return o.square?.mark.key === this.square?.mark.key && o.stem === this.stem && o.red === this.red && o.read === this.read && o.label === this.label
  }
  toDOM() {
    const g = document.createElement('div')
    g.className = 'gm'
    if (this.stem) {
      const s = document.createElement('i')
      s.className = `stem ${this.stem}${this.red ? ' new' : ''}`
      g.append(s)
      if (this.stem === 'end') {
        const f = document.createElement('i')
        f.className = `foot${this.red ? ' new' : ''}`
        g.append(f)
      }
    }
    // Not a button: the gutter is hidden from screen readers, and the
    // margin's notes, and ] and [, reach the same ideas.
    if (this.square) {
      const b = document.createElement('i')
      b.className = `sq${this.red ? ' new' : ''}${this.read ? ' read' : ''}`
      b.dataset.key = this.square.mark.key
      b.title = this.label
      g.append(b)
    }
    return g
  }
}

/**
 * The gutter's squares: one on each idea's first line, and, when it spans
 * lines, a stem down to a short foot on its last, in the square's colour.
 * Clicking a square opens its idea.
 */
export function squares(placed: Placed[], seen: Seen, open: (m: Marked) => void): Extension {
  return gutter({
    class: 'cm-ideas',
    lineMarker(view, line) {
      const n = view.state.doc.lineAt(line.from).number
      const starts = placed.find((p) => p.a === n) ?? null
      const spans = placed.filter((p) => p.b > p.a && p.a <= n && p.b >= n)
      let stem: Mark['stem'] = null
      let by: Placed | undefined
      for (const p of spans) {
        const here = p.a === n ? 'down' : p.b === n ? 'end' : 'through'
        // A stem going on beats one ending; one ending beats one starting.
        const rank = { through: 3, end: 2, down: 1 }
        if (!stem || rank[here] > rank[stem]) {
          stem = here
          by = p
        }
      }
      if (!starts && !stem) return null
      const key = (starts ?? by)!.mark.key
      const red = seen.isNew(key) || spans.some((p) => seen.isNew(p.mark.key))
      const name = starts ? (starts.mark.mark.idea?.name ?? starts.mark.mark.name) : ''
      return new Mark(starts, stem, red, seen.isRead(key), starts ? squareLabel(name, seen.isNew(key), seen.isRead(key)) : '')
    },
    initialSpacer: () => new Mark(null, null, false, false, ''),
    domEventHandlers: {
      click(view, line, event) {
        const key = (event.target as HTMLElement).closest<HTMLElement>('.sq')?.dataset.key
        const n = view.state.doc.lineAt(line.from).number
        const p = placed.find((x) => x.a === n && (!key || x.mark.key === key))
        if (!p) return false
        open(p.mark)
        return true
      },
    },
  })
}

class Diff extends GutterMarker {
  constructor(readonly kind: string) {
    super()
  }
  eq(o: Diff) {
    return o.kind === this.kind
  }
  toDOM() {
    const s = document.createElement('span')
    s.className = this.kind
    s.textContent = this.kind === 'added' ? '+' : '\u2212'
    return s
  }
}

/** A change's lines: a + for each added, a − for each taken out, in a column of their own. */
export function diffMarks(kinds: Array<string | undefined>): Extension {
  return gutter({
    class: 'cm-diff',
    lineMarker(view, line) {
      const kind = kinds[view.state.doc.lineAt(line.from).number - 1]
      return kind === 'added' || kind === 'removed' ? new Diff(kind) : null
    },
    initialSpacer: () => new Diff('added'),
  })
}

/** Classes for lines of the document: lines added and taken out, and the lines an open idea is on. */
export function lineClasses(doc: Text, classes: Map<number, string>): Extension {
  const b = new RangeSetBuilder<Decoration>()
  for (const n of [...classes.keys()].sort((x, y) => x - y)) {
    if (n < 1 || n > doc.lines) continue
    const from = doc.line(n).from
    b.add(from, from, Decoration.line({ class: classes.get(n)! }))
  }
  return EditorView.decorations.of(b.finish())
}

/** A run of a change's lines, by the document's lines: its first and last. */
export type Run = { a: number; b: number }

/** The runs of lines a change added, and those it took out, in order. */
export function changedRuns(kinds: Array<string | undefined>): Array<Run & { kind: 'added' | 'removed' }> {
  const out: Array<Run & { kind: 'added' | 'removed' }> = []
  kinds.forEach((k, i) => {
    if (k !== 'added' && k !== 'removed') return
    const last = out[out.length - 1]
    if (last && last.kind === k && last.b === i) last.b = i + 1
    else out.push({ kind: k, a: i + 1, b: i + 1 })
  })
  return out
}

/**
 * What changed, alone: the runs of lines a change didn't touch, beyond
 * `around` lines either side of what it did, to fold away. A run of one
 * line stays: folding it would save nothing.
 */
export function asBefore(kinds: Array<string | undefined>, around = 1): Run[] {
  const shown = new Array<boolean>(kinds.length).fill(false)
  kinds.forEach((k, i) => {
    if (k !== 'added' && k !== 'removed') return
    for (let j = Math.max(0, i - around); j <= Math.min(kinds.length - 1, i + around); j++) shown[j] = true
  })
  const out: Run[] = []
  let from = -1
  for (let i = 0; i <= kinds.length; i++) {
    if (i < kinds.length && !shown[i]) {
      if (from < 0) from = i
    } else if (from >= 0) {
      if (i - from >= 2) out.push({ a: from + 1, b: i })
      from = -1
    }
  }
  return out
}

/** Folds these runs, in place of any folded before. */
export const foldRuns = StateEffect.define<Run[]>()
/** Opens the fold that starts on this line. */
const unfold = StateEffect.define<number>()

/** One run of lines as before, folded to a quiet line; a click opens it. */
class AsBefore extends WidgetType {
  constructor(readonly run: Run) {
    super()
  }
  eq(o: AsBefore) {
    return o.run.a === this.run.a && o.run.b === this.run.b
  }
  toDOM(view: EditorView) {
    const n = this.run.b - this.run.a + 1
    const b = document.createElement('button')
    b.type = 'button'
    b.className = 'cm-asbefore'
    const count = document.createElement('b')
    count.textContent = `${n} lines`
    b.append(count, ' as before')
    b.addEventListener('click', () => view.dispatch({ effects: unfold.of(this.run.a) }))
    // Padded, not spaced by margins, so CodeMirror measures its height whole.
    const row = document.createElement('div')
    row.className = 'cm-asbefore-row'
    row.append(b)
    return row
  }
  ignoreEvent() {
    return true
  }
}

/** The runs folded now. */
const folded = StateField.define<Run[]>({
  create: () => [],
  update(runs, tr) {
    for (const e of tr.effects) {
      if (e.is(foldRuns)) runs = e.value
      else if (e.is(unfold)) runs = runs.filter((r) => r.a !== e.value)
    }
    return runs
  },
  provide: (f) =>
    EditorView.decorations.compute([f], (state): DecorationSet => {
      const doc = state.doc
      const ranges = state.field(f)
        .filter((r) => r.b <= doc.lines)
        .map((r) => Decoration.replace({ widget: new AsBefore(r), block: true }).range(doc.line(r.a).from, doc.line(r.b).to))
      return Decoration.set(ranges, true)
    }),
})

/** Folding what's as before: none folded until `foldRuns` says which. */
export const folding: Extension = folded

/** The fold a line of the document is in, if it's in one. */
export function foldOf(state: EditorState, n: number): Run | undefined {
  return state.field(folded, false)?.find((r) => r.a <= n && n <= r.b)
}

/** Opens the fold a line of the document is in: none, when it isn't in one. */
export function unfoldAt(state: EditorState, n: number): StateEffect<number>[] {
  const run = foldOf(state, n)
  return run ? [unfold.of(run.a)] : []
}

/** Brings a point of the file into view, a third of the way down. */
export function bringIntoView(scroller: HTMLElement | null, el: HTMLElement | null, top: number, smooth = false) {
  if (!scroller || !el) return
  const offset = el.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop
  scroller.scrollTo({ top: Math.max(0, offset + top - scroller.clientHeight / 3), behavior: smooth ? 'smooth' : 'auto' })
}
