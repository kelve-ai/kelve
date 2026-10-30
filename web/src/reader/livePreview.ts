// A note's markdown, drawn as it reads: its marks (#, **, `, [](), >, -)
// hidden, and what they mean shown: headings large, bold, code, links, quotes
// and bullets. The lines you're editing show their markdown, so you can change
// it; with the note not focused, none do.

import { syntaxTree } from '@codemirror/language'
import type { EditorState, Range } from '@codemirror/state'
import { Decoration, type DecorationSet, EditorView, ViewPlugin, type ViewUpdate, WidgetType } from '@codemirror/view'

/** A list's - or * or +, drawn as a bullet. */
class Bullet extends WidgetType {
  eq() {
    return true
  }
  toDOM() {
    const b = document.createElement('span')
    b.className = 'cm-md-bullet'
    b.textContent = '•'
    return b
  }
}

const hide = Decoration.replace({})
const bullet = Decoration.replace({ widget: new Bullet() })
const heading = (level: number) => Decoration.line({ class: `cm-md-h cm-md-h${level}` })
const quote = Decoration.line({ class: 'cm-md-quote' })
/** kelve's own links, [[an idea]]: their brackets hidden like markdown's. */
const IDEA = /\[\[([^[\]\n]{1,80})\]\]/g

/** The lines being edited: those a selection touches, while the note has focus. */
function editing(view: EditorView): Set<number> {
  const lines = new Set<number>()
  if (!view.hasFocus) return lines
  for (const r of view.state.selection.ranges) {
    const a = view.state.doc.lineAt(r.from).number
    const b = view.state.doc.lineAt(r.to).number
    for (let n = a; n <= b; n++) lines.add(n)
  }
  return lines
}

function draw(view: EditorView): DecorationSet {
  const state: EditorState = view.state
  const active = editing(view)
  const on = (pos: number) => active.has(state.doc.lineAt(pos).number)
  const out: Range<Decoration>[] = []
  for (const { from, to } of view.visibleRanges) {
    syntaxTree(state).iterate({
      from,
      to,
      enter: (node) => {
        const name = node.name
        const heading_ = /^ATXHeading(\d)$/.exec(name)
        if (heading_) {
          out.push(heading(Number(heading_[1])).range(state.doc.lineAt(node.from).from))
          return
        }
        if (name === 'Blockquote') {
          for (let pos = node.from; pos <= node.to; ) {
            const line = state.doc.lineAt(pos)
            out.push(quote.range(line.from))
            pos = line.to + 1
          }
          return
        }
        if (on(node.from)) return
        switch (name) {
          case 'HeaderMark': {
            // The #s, and the space after them.
            const end = state.sliceDoc(node.to, node.to + 1) === ' ' ? node.to + 1 : node.to
            out.push(hide.range(node.from, end))
            return
          }
          case 'EmphasisMark':
          case 'CodeMark':
          case 'LinkMark':
          case 'URL': {
            // A bare URL isn't hidden: only one inside a link's ( ).
            if (name === 'URL' && node.node.parent?.name !== 'Link') return
            if (node.to > node.from) out.push(hide.range(node.from, node.to))
            return
          }
          case 'QuoteMark': {
            const end = state.sliceDoc(node.to, node.to + 1) === ' ' ? node.to + 1 : node.to
            out.push(hide.range(node.from, end))
            return
          }
          case 'ListMark': {
            // Bullets for - * +; an ordered list keeps its numbers.
            if (/^[-*+]$/.test(state.sliceDoc(node.from, node.to))) out.push(bullet.range(node.from, node.to))
            return
          }
        }
      },
    })
    // [[an idea]]: its words alone, still a link.
    const text = state.sliceDoc(from, to)
    for (const m of text.matchAll(IDEA)) {
      const at = from + (m.index ?? 0)
      if (on(at)) continue
      out.push(hide.range(at, at + 2), hide.range(at + m[0].length - 2, at + m[0].length))
    }
  }
  return Decoration.set(out, true)
}

/** The plugin: drawn again as you type, move, scroll, or leave the note. */
export const livePreview = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet
    constructor(view: EditorView) {
      this.decorations = draw(view)
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.selectionSet || u.viewportChanged || u.focusChanged) this.decorations = draw(u.view)
    }
  },
  { decorations: (v) => v.decorations },
)

/** Headings by level, quotes with a rule, bullets: the note as it reads. */
export const livePreviewTheme = EditorView.baseTheme({
  '.cm-md-h1': { fontSize: '24px', lineHeight: '34px' },
  '.cm-md-h2': { fontSize: '20px', lineHeight: '30px' },
  '.cm-md-h3': { fontSize: '17px', lineHeight: '27px' },
  '.cm-md-h': { paddingTop: '6px !important' },
  '.cm-md-quote': { borderLeft: '2px solid var(--rule)', paddingLeft: '14px !important', color: 'var(--soft)' },
  '.cm-md-bullet': { display: 'inline-block', width: '1em', color: 'var(--soft)' },
})
