// What every page read plainly is made of, a file's, a text's, a folder's and a
// change's alike (boards Plainly-1 to -9): the agent's words with the ideas
// they rest on woven in, each a square and its words that open its entry beside
// the work; a mark's word; the kind line; a group's head; a part, its mark, its
// goal, what it does, what it did before, where it is, and the way to it; and
// the one quiet line for what else is here.

import type { ReactNode } from 'react'
import type { ChangeKind, PartMark, PlainIdea } from '../api/types'
import { Rich } from '../rich'
import { Square } from './Square'

/** The agent's words, each `[[term]]` that leads to an idea a link to its entry; one that doesn't, plain words. */
export function Woven({ text, ideas, idea }: { text: string; ideas: PlainIdea[]; idea: (key: string) => void }) {
  const find = (words: string) =>
    ideas.find((i) => i.term === words) ?? ideas.find((i) => i.term.toLowerCase() === words.toLowerCase() || i.name.toLowerCase() === words.toLowerCase())
  return (
    <Rich
      text={text}
      term={(words) => {
        const i = find(words)
        if (!i) return words
        // Inline, not a button's box, so the words wrap and keep their stop.
        return (
          <span
            role="button"
            tabIndex={0}
            className="il"
            title={i.name}
            onClick={() => idea(i.key)}
            onKeyDown={(e) => {
              if (e.key !== 'Enter' && e.key !== ' ') return
              e.preventDefault()
              idea(i.key)
            }}
          >
            <Square ideaKey={i.key} name={i.name} />
            {words}
          </span>
        )
      }}
    />
  )
}

const MARKS: Record<PartMark, [string, string]> = {
  new: ['New', 'new'],
  changed: ['Changed', 'chg'],
  moved: ['Moved', 'same'],
  takenOut: ['Taken out', 'out'],
  asBefore: ['As before', 'same'],
}

const KINDS: Record<ChangeKind, [string, string]> = {
  adds: ['Adds', 'new'],
  fixes: ['Fixes', 'chg'],
  reshapes: ['Reshapes', 'same'],
  takesOut: ['Takes out', 'out'],
  updates: ['Updates', 'chg'],
}

/** A part's mark, in a word, coloured as the change's lines are. */
export function Mark({ mark }: { mark: PartMark }) {
  const [word, cls] = MARKS[mark]
  return <span className={`mkt ${cls}`}>{word}</span>
}

/** The kind line: a change's kind, or "In this change", then before → now. */
export function KindLine({ kind, word, children }: { kind?: ChangeKind | null; word?: string; children: ReactNode }) {
  const [said, cls] = kind ? KINDS[kind] : [word ?? 'In this change', 'chg']
  return (
    <p className="chg">
      <span className={`mkt ${cls}`}>{said}</span>
      <span>{children}</span>
    </p>
  )
}

/** A heading over a group of rows: its name, and how many, quietly. */
export function GroupHead({ name, count }: { name: string; count?: ReactNode }) {
  return (
    <h3 className="gh2">
      <b>{name}</b>
      {count != null && <span>{count}</span>}
    </h3>
  )
}

/** One part: its mark, its goal with where it is, what it does, what it did before, then how to get to it; its lines at the right. */
export function PartRow({ mark, step, goal, where, says, ideas, before, idea, at, atTitle, lit, children }: {
  mark?: PartMark | null
  /** With no mark, its place among the parts, in a colour of its own. */
  step?: number
  goal: string
  /** The part's path, quiet, beside its goal. */
  where?: string
  says: string
  ideas: PlainIdea[]
  before?: string | null
  idea: (key: string) => void
  /** Where it is in its file, at the right: its lines, or a section's number. */
  at?: string
  /** Said of where it is, on a hover. */
  atTitle?: string
  /** The idea whose entry is open rests on it: it's tinted. */
  lit?: boolean
  /** Under it: the way to the part, its code or its files. */
  children?: ReactNode
}) {
  return (
    <section className={`pp${lit ? ' on' : ''}`} aria-label={goal}>
      <span className="mkc">{mark ? <Mark mark={mark} /> : step != null && <span className="mkn">{step}</span>}</span>
      <div>
        <h4 className="lb">
          <Woven text={goal} ideas={ideas} idea={idea} />
          {where && <> <span className="pn">{where}</span></>}
        </h4>
        {says && <p><Woven text={says} ideas={ideas} idea={idea} /></p>}
        {before && (
          <p className="was">
            <b>Before</b>
            <Woven text={before} ideas={ideas} idea={idea} />
          </p>
        )}
        {children}
      </div>
      <span className="pln" title={atTitle}>{at}</span>
    </section>
  )
}

/** The one quiet line for what else is here: set-up, what most parts use, what tools made. */
export function AlsoLine({ name, title, children }: { name: string; title?: string; children: ReactNode }) {
  return (
    <p className="fold3" title={title}>
      <b>{name}</b>
      <span>{children}</span>
    </p>
  )
}

/** "a", "a and b", "a, b and c". */
export function listed(names: ReactNode[]): ReactNode {
  return names.map((n, i) => (
    <span key={i}>
      {i > 0 && (i === names.length - 1 ? ' and ' : ', ')}
      {n}
    </span>
  ))
}

/** A part's name within its folder: `bank/`, `api/v1/charges.rs`. */
export function within(folder: string, path: string, isFolder = false): string {
  const rel = folder && path.startsWith(`${folder}/`) ? path.slice(folder.length + 1) : path
  return `${rel}${isFolder ? '/' : ''}`
}
