// One entry: the page for a node, read on a trail. Its words are the way
// on: each term it links goes to that node, marked with kelve's red square
// once you've read it (hollow) or read and recapped it (filled). Sources sit
// in the margin beside the text that cites them. It asks nothing.
//
// It reads in two layers: what each part shows, and folds under a part, a
// worked example, a common mistake and the like, closed until opened, then
// open in place.

import { Component, Fragment, type MouseEvent, type ReactNode } from 'react'
import type { Block, Body, ClaimCheck, Entry as EntryView, Fold, IdeaRef, Source } from './api/types'
import { AnswerCard, Bars, Compare, Conversation, Excerpt, Squares, Table, Timeline } from './blocks/Blocks'
import { Diagram } from './blocks/Diagram'
import { Explorable } from './blocks/Explorable'
import { Annotated, BeforeAfter, Funnel, NumberFigure, Quadrant, Sequence, Spectrum, Steps, Trend, Tree } from './blocks/Figures'
import { cut, Rich, refsIn } from './rich'

function siteOf(url: string) {
  try {
    return new URL(url).hostname.replace(/^www\./, '')
  } catch {
    return ''
  }
}

/**
 * Sources in the margin, beside the text that cites them. Only a source's
 * own address is ever a link out: nothing a model wrote is.
 */
function Notes({ refs, sources }: { refs: number[]; sources: Source[] }) {
  const list = sources.filter((s) => refs.includes(s.n))
  if (!list.length) return <aside className="notes" />
  return (
    <aside className="notes">
      {list.map((s) => (
        <p key={s.n} className="note">
          <b>{s.n}</b>
          <span>
            {s.url ? <a href={s.url} target="_blank" rel="noreferrer noopener">{s.title}</a> : s.title}. {s.publisher}{s.published ? `, ${s.published}` : ''}{s.preprint ? ', a preprint' : ''}.
            {(s.url || s.archived) && (
              <small>
                {s.url && siteOf(s.url)}
                {s.archived && <>{s.url ? ' · ' : ''}<a href={s.archived} target="_blank" rel="noreferrer noopener">archived copy</a></>}
              </small>
            )}
          </span>
        </p>
      ))}
    </aside>
  )
}

/**
 * A block that fails to draw is left out, and the rest of the entry, and
 * the page, carry on without it.
 */
class Drawn extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false }
  static getDerivedStateFromError() {
    return { failed: true }
  }
  componentDidCatch(error: unknown) {
    console.warn('A block couldn’t be drawn, so it’s left out.', error)
  }
  render() {
    return this.state.failed ? null : this.props.children
  }
}

/** The text a block holds, with its markup: for the sources it cites. */
function textOf(b: Body): string {
  switch (b.type) {
    case 'paragraph':
    case 'heading':
      return b.text
    case 'list':
      return b.items.join(' ')
    case 'excerpt':
      return b.lines.map((l) => l.text).join(' ')
    case 'diagram':
      return [b.describe, ...b.nodes.map((n) => `${n.label} ${n.note ?? ''}`), ...b.steps.map((s) => s.caption)].join(' ')
    case 'explorable':
      return [b.output, b.describe, ...b.inputs.map((i) => i.label)].join(' ')
    case 'bars':
      return [...b.rows.map((r) => `${r.label} ${r.display}`), b.caption ?? ''].join(' ')
    case 'table':
      return [...b.head, ...b.rows.flat()].join(' ')
    case 'compare':
      return [b.left.title, ...b.left.points, b.right.title, ...b.right.points].join(' ')
    case 'squares':
    case 'timeline':
      return b.rows.map((r) => r.text).join(' ')
    case 'conversation':
      return b.lines.map((l) => `${l.who} ${l.text}`).join(' ')
    case 'answer':
      return `${b.lead} ${b.text}`
    case 'sequence':
      return [b.describe, ...b.actors, ...b.messages.map((m) => m.text)].join(' ')
    case 'steps':
      return b.cards.map((c) => `${c.title} ${c.text}`).join(' ')
    case 'quadrant':
      return [b.describe, ...b.points.map((p) => p.label)].join(' ')
    case 'spectrum':
      return [b.describe, b.low, b.high, ...b.points.map((p) => `${p.label} ${p.note ?? ''}`)].join(' ')
    case 'trend':
      return [b.describe, ...b.series.map((s) => s.label)].join(' ')
    case 'tree':
      return [b.describe, ...b.branches.map((x) => `${x.edge ?? ''} ${x.label}`)].join(' ')
    case 'beforeAfter':
      return [b.describe, b.before.label, ...b.before.lines, b.after.label, ...b.after.lines].join(' ')
    case 'number':
      return `${b.figure} ${b.text}`
    case 'annotated':
      return [...b.lines.map((l) => l.text), ...b.notes.map((n) => n.text)].join(' ')
    case 'funnel':
      return [...b.stages.map((s) => `${s.label} ${s.display}`), b.caption ?? ''].join(' ')
  }
}

/** What each kind of fold says it holds, closed. */
const FOLDS: Record<Fold, string> = {
  example: 'A worked example',
  mistake: 'A common mistake',
  lookAlike: 'Looks like it, but isn’t',
  rule: 'The precise rule',
  depth: 'More on this',
}

/** An entry's blocks in reading order: each shown one alone, and each fold, its blocks together. */
type Piece = { block: Block } | { fold: Fold; blocks: Block[] }

function pieces(blocks: Block[]): Piece[] {
  const out: Piece[] = []
  for (const block of blocks) {
    const last = out[out.length - 1]
    if (!block.fold) out.push({ block })
    else if (last && 'fold' in last && last.fold === block.fold && last.blocks[0].part === block.part) last.blocks.push(block)
    else out.push({ fold: block.fold, blocks: [block] })
  }
  return out
}

/** Where an idea's entry is read. */
export const ideaHref = (idea: IdeaRef) => `#/i/${encodeURIComponent(idea.id)}`

/** An id safe in the page's markup, from an idea's id. */
const domId = (id: string) => id.replace(/[^a-zA-Z0-9]/g, '-')

export function Entry({ entry, onIdea, claims, fixer }: {
  entry: EntryView
  /** Opens the entry a link leads to; without it, links go by address. */
  onIdea?: (idea: IdeaRef) => void
  /**
   * While the shared graph checks it: each claim, where it's made, and how
   * its check stands. Those not checked yet are underlined, dotted; those
   * that failed are tinted, with what their pages say under them.
   */
  claims?: ClaimCheck[]
  /** Who's asked to fix what fails, as people say it. */
  fixer?: string
}) {
  /** A term in the prose: a link to the idea it names. */
  const term = (words: string): ReactNode => {
    const link = entry.links.find((l) => l.term.toLowerCase() === words.toLowerCase())
    if (!link || link.idea.id === entry.idea.id) return words
    const go = (e: MouseEvent) => {
      if (!onIdea || e.metaKey || e.ctrlKey) return
      e.preventDefault()
      onIdea(link.idea)
    }
    return <a className="l" href={ideaHref(link.idea)} onClick={go}>{words}</a>
  }
  const cite = (n: number) => {
    const s = entry.sources.find((x) => x.n === n)
    return s && `${s.title}. ${s.publisher}`
  }
  const rich = (text: string) => <Rich text={text} term={term} cite={cite} />
  /** A block's text, its claims marked where they're made. */
  const marked = (text: string, block: string) => {
    const here = (claims ?? []).filter((c) => c.block === block && c.words && c.state !== 'held')
    if (!here.length) return rich(text)
    return cut(text, here.map((c) => c.words)).map((piece, j) => {
      if (piece.at === undefined) return <Fragment key={j}>{rich(piece.text)}</Fragment>
      const c = here[piece.at]
      return (
        <span key={j} className={`claim ${c.state}`} title={c.state === 'failed' ? 'Its page doesn’t say this' : 'Not checked yet'}>
          {rich(piece.text)}
        </span>
      )
    })
  }
  /** Under a block: what each claim it makes that failed was found to lack. */
  const failedIn = (block: string) =>
    (claims ?? [])
      .filter((c) => c.block === block && c.words && c.state === 'failed')
      .map((c) => (
        <div key={`f${c.n}`} className="erow">
          <div className="main">
            <p className="flagged">
              {c.found ? `This doesn’t match its source. ${c.found.replace(/\.?$/, '.')}` : 'This doesn’t match its source.'}{' '}
              {fixer ?? 'Your agent'} will be asked to fix it; if it can’t, the sentence goes.
            </p>
          </div>
        </div>
      ))

  const render = (block: Block): ReactNode => {
    const b = block.body
    const id = `${domId(entry.idea.id)}-${block.id}`
    const row = (node: ReactNode, wide = false) => (
      <div key={block.id} className={`erow${wide ? ' wide' : ''}`}>
        <div className="main">{node}</div>
        {!wide && <Notes refs={refsIn(textOf(b))} sources={entry.sources} />}
      </div>
    )
    const withFailed = (node: ReactNode) => <Fragment key={block.id}>{node}{failedIn(block.id)}</Fragment>
    switch (b.type) {
      case 'paragraph': return withFailed(row(<p className={`p${b.quiet ? ' quiet' : ''}`}>{marked(b.text, block.id)}</p>))
      case 'heading': return withFailed(row(<h2 className="h2">{marked(b.text, block.id)}</h2>))
      case 'list': return withFailed(row(<ul className="list">{b.items.map((x, j) => <li key={j}>{marked(x, block.id)}</li>)}</ul>))
      case 'excerpt': return row(<Excerpt body={b} />)
      case 'diagram': return row(<Diagram id={id} body={b} still />)
      case 'explorable': return row(<Explorable id={id} body={b} />)
      case 'bars': return row(<Bars body={b} />)
      case 'table': return row(<Table body={b} />, b.head.length >= 3)
      case 'compare': return row(<Compare body={b} />)
      case 'squares': return row(<Squares body={b} />)
      case 'timeline': return row(<Timeline body={b} />)
      case 'conversation': return row(<Conversation body={b} />)
      case 'answer': return row(<AnswerCard body={b} />)
      case 'sequence': return row(<Sequence id={id} body={b} still />)
      case 'steps': return row(<Steps body={b} />)
      case 'quadrant': return row(<Quadrant id={id} body={b} still />)
      case 'spectrum': return row(<Spectrum id={id} body={b} still />)
      case 'trend': return row(<Trend id={id} body={b} still />)
      case 'tree': return row(<Tree id={id} body={b} still />)
      case 'beforeAfter': return row(<BeforeAfter id={id} body={b} still />)
      case 'number': return row(<NumberFigure body={b} still />)
      case 'annotated': return row(<Annotated body={b} still />)
      case 'funnel': return row(<Funnel body={b} still />)
    }
  }

  // Sources nothing in the text cites still show, at the end.
  const cited = new Set(entry.blocks.flatMap((b) => refsIn(textOf(b.body))))
  const uncited = entry.sources.filter((s) => !cited.has(s.n)).map((s) => s.n)

  /** A fold, closed until it's opened; open from the start when a claim in it is being checked. */
  const fold = (kind: Fold, blocks: Block[]) => {
    const marked = (claims ?? []).some((c) => c.state !== 'held' && blocks.some((b) => b.id === c.block))
    return (
      <details key={`fold-${blocks[0].id}`} className="fold" open={marked || undefined}>
        <summary>{FOLDS[kind]}</summary>
        {blocks.map((block) => <Drawn key={block.id}>{render(block)}</Drawn>)}
      </details>
    )
  }

  return (
    <article className="entry" aria-labelledby={`t-${domId(entry.idea.id)}`}>
      {pieces(entry.blocks).map((piece) => 'fold' in piece ? fold(piece.fold, piece.blocks) : <Drawn key={piece.block.id}>{render(piece.block)}</Drawn>)}
      {uncited.length > 0 && (
        <div className="erow">
          <div className="main" />
          <Notes refs={uncited} sources={entry.sources} />
        </div>
      )}
    </article>
  )
}
