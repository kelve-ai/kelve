// The little markup entries are written in: `code`, **the key sentence**,
// ^1 for a source in the margin, and [[term]] for a term that leads on. A
// term is only ever a link to a node kelve matched; with nothing to link
// it to, it's its words.

import { Fragment, type ReactNode } from 'react'

export function Rich({ text, term, cite }: {
  text: string
  term?: (words: string) => ReactNode
  /** A source, by its number, in words: shown on pointing at its number. */
  cite?: (n: number) => string | undefined
}) {
  const parts = text.split(/(`[^`]+`|\*\*.+?\*\*|\^\d+|\[\[[^[\]]+\]\])/g)
  return (
    <>
      {parts.map((part, i): ReactNode => {
        if (part.startsWith('`') && part.endsWith('`') && part.length > 1) return <span key={i} className="mono">{part.slice(1, -1)}</span>
        if (part.startsWith('**') && part.endsWith('**') && part.length > 4) return <span key={i} className="key"><Rich text={part.slice(2, -2)} term={term} cite={cite} /></span>
        if (/^\^\d+$/.test(part)) return <sup key={i} className="ref" title={cite?.(Number(part.slice(1)))}>{part.slice(1)}</sup>
        if (part.startsWith('[[') && part.endsWith(']]')) {
          const words = part.slice(2, -2)
          return <Fragment key={i}>{term ? term(words) : words}</Fragment>
        }
        return <Fragment key={i}>{part}</Fragment>
      })}
    </>
  )
}

/** The sources a piece of text cites, by number. */
export function refsIn(text: string): number[] {
  return [...text.matchAll(/\^(\d+)/g)].map((m) => Number(m[1]))
}

/** Text without its markup, as words. */
export function plain(text: string): string {
  return text.replace(/\[\[([^[\]]+)\]\]/g, '$1').replace(/\*\*/g, '').replace(/`/g, '').replace(/\^\d+/g, '')
}

const MARKUP = /(`[^`]+`|\*\*.+?\*\*|\^\d+|\[\[[^[\]]+\]\])/g
const WHOLE_MARKUP = /^(`[^`]+`|\*\*.+?\*\*|\^\d+|\[\[[^[\]]+\]\])$/

/**
 * Text cut where each of `words` is, as the reader reads it, without
 * markup: each piece with the index of the words it holds, if it's one.
 * A cut that would fall inside markup takes the whole of it, so every
 * piece still reads as `Rich` does. Words not found, or overlapping words
 * found earlier, are left out.
 */
export function cut(text: string, words: string[]): Array<{ text: string; at?: number }> {
  const spans: Array<{ raw0: number; raw1: number; p0: number; p1: number; markup: boolean }> = []
  let raw = 0
  let flat = ''
  for (const part of text.split(MARKUP)) {
    if (!part) continue
    const markup = WHOLE_MARKUP.test(part)
    const said = markup ? plain(part) : part
    spans.push({ raw0: raw, raw1: raw + part.length, p0: flat.length, p1: flat.length + said.length, markup })
    raw += part.length
    flat += said
  }
  const found: Array<{ from: number; to: number; at: number }> = []
  words.forEach((w, at) => {
    const start = w ? flat.indexOf(w) : -1
    if (start < 0) return
    const end = start + w.length
    const first = spans.find((s) => s.p1 > start)
    const last = [...spans].reverse().find((s) => s.p0 < end)
    if (!first || !last) return
    const from = first.markup ? first.raw0 : first.raw0 + (start - first.p0)
    const to = last.markup ? last.raw1 : last.raw0 + (end - last.p0)
    if (found.some((f) => from < f.to && to > f.from)) return
    found.push({ from, to, at })
  })
  found.sort((a, b) => a.from - b.from)
  const out: Array<{ text: string; at?: number }> = []
  let at = 0
  for (const f of found) {
    if (f.from > at) out.push({ text: text.slice(at, f.from) })
    out.push({ text: text.slice(f.from, f.to), at: f.at })
    at = f.to
  }
  if (at < text.length) out.push({ text: text.slice(at) })
  return out
}
