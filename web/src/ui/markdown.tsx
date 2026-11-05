// Markdown drawn with kelve's own elements, from remark's parser's tree.
// Nothing in it is markup: HTML shows as its text, and images as their words,
// never fetched. The reader's text files (reader/Prose.tsx) and the pages
// written as markdown, the debrief and skills (`Markdown`), both draw it so.

import { Fragment, type ReactNode, useMemo } from 'react'
import { fromMarkdown } from 'mdast-util-from-markdown'
import { gfmFromMarkdown } from 'mdast-util-gfm'
import { gfm } from 'micromark-extension-gfm'
import type { Parents, RootContent } from 'mdast'

/** Only links people expect to follow leave the page. */
const safeHref = (url: string) => (/^(https?:|mailto:)/i.test(url) ? url : undefined)

/** A node's inline parts; `links` lets a link lead out, or else it's only its words, in red. */
export function inline(nodes: RootContent[], key: string, links = true): ReactNode[] {
  return nodes.map((node, i) => {
    const k = `${key}.${i}`
    switch (node.type) {
      case 'text':
        return <Fragment key={k}>{node.value}</Fragment>
      case 'inlineCode':
        return <code key={k}>{node.value}</code>
      case 'emphasis':
        return <em key={k}>{inline(node.children, k, links)}</em>
      case 'strong':
        return <strong key={k}>{inline(node.children, k, links)}</strong>
      case 'delete':
        return <del key={k}>{inline(node.children, k, links)}</del>
      case 'break':
        return <br key={k} />
      case 'link': {
        const href = links ? safeHref(node.url) : undefined
        if (href) return <a key={k} href={href} target="_blank" rel="noreferrer noopener">{inline(node.children, k, links)}</a>
        return <span key={k} className={links ? undefined : 'md-a'}>{inline(node.children, k, links)}</span>
      }
      case 'image':
      case 'imageReference':
        // Never fetched: its words stand in for it.
        return node.alt ? <span key={k} className="alt">[{node.alt}]</span> : null
      case 'html':
        return <code key={k}>{node.value}</code>
      case 'footnoteReference':
        return <sup key={k}>{node.label ?? node.identifier}</sup>
      default:
        return 'children' in node ? <Fragment key={k}>{inline((node as Parents).children as RootContent[], k, links)}</Fragment> : null
    }
  })
}

/** One block: a heading is a level down, as the page has its own title. */
export function block(node: RootContent, key: string, links = true): ReactNode {
  switch (node.type) {
    case 'heading': {
      const H = `h${Math.min(node.depth + 1, 6)}` as 'h2'
      return <H key={key}>{inline(node.children, key, links)}</H>
    }
    case 'paragraph':
      return <p key={key}>{inline(node.children, key, links)}</p>
    case 'blockquote':
      return <blockquote key={key}>{node.children.map((n, i) => block(n, `${key}.${i}`, links))}</blockquote>
    case 'list': {
      const L = node.ordered ? 'ol' : 'ul'
      return (
        <L key={key} start={node.ordered ? (node.start ?? undefined) : undefined}>
          {node.children.map((item, i) => (
            <li key={i}>
              {item.checked != null && <span className="tick" aria-label={item.checked ? 'Done' : 'Not done'}>{item.checked ? '☑' : '☐'} </span>}
              {item.children.map((n, j) => (n.type === 'paragraph' && !node.spread ? <Fragment key={j}>{inline(n.children, `${key}.${i}.${j}`, links)}</Fragment> : block(n, `${key}.${i}.${j}`, links)))}
            </li>
          ))}
        </L>
      )
    }
    case 'code':
      return <pre key={key}><code>{node.value}</code></pre>
    case 'thematicBreak':
      return <hr key={key} />
    case 'table':
      return (
        <div key={key} className="ptable">
          <table>
            <tbody>
              {node.children.map((row, i) => (
                <tr key={i}>
                  {row.children.map((cell, j) => {
                    const Cell = i === 0 ? 'th' : 'td'
                    return <Cell key={j}>{inline(cell.children, `${key}.${i}.${j}`, links)}</Cell>
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )
    case 'html':
      return <pre key={key}><code>{node.value}</code></pre>
    case 'definition':
    case 'footnoteDefinition':
      return null
    default:
      return 'children' in node ? <div key={key}>{((node as Parents).children as RootContent[]).map((n, i) => block(n, `${key}.${i}`, links))}</div> : null
  }
}

/** Markdown's top-level blocks, with the lines each came from. */
export function parse(text: string): RootContent[] {
  return fromMarkdown(text, { extensions: [gfm()], mdastExtensions: [gfmFromMarkdown()] }).children.filter(
    (n) => n.type !== 'definition' && n.type !== 'footnoteDefinition',
  )
}

/** A node's words, without its markup. */
export function wordsOf(node: RootContent): string {
  if ('value' in node && typeof node.value === 'string') return node.value
  if ('children' in node) return ((node as Parents).children as RootContent[]).map(wordsOf).join('')
  return ''
}

/** One block of a page, as its margin is given it. */
export type MdBlock = {
  node: RootContent
  /** Its place among the page's blocks. */
  index: number
  /** Its first and last lines in the text. */
  from: number
  to: number
  /** For a heading: its level, as written (`##` is 2), and its words. */
  heading?: { depth: number; text: string }
}

/**
 * A page written as markdown, the debrief's or a skill's: each block a row,
 * with a margin beside it where `margin` gives one, for a note by a heading.
 * Links are their words only, unless `links` lets them lead out.
 */
export function Markdown({ text, size = 'base', margin, links = false, label }: {
  text: string
  size?: 'small' | 'base' | 'large'
  margin?: (block: MdBlock) => ReactNode
  links?: boolean
  /** What it is, for screen readers: "The debrief". */
  label?: string
}) {
  const blocks = useMemo<MdBlock[]>(
    () =>
      parse(text).map((node, index) => ({
        node,
        index,
        from: node.position?.start.line ?? 0,
        to: node.position?.end.line ?? 0,
        heading: node.type === 'heading' ? { depth: node.depth, text: wordsOf(node) } : undefined,
      })),
    [text],
  )
  return (
    <div className={`md ${size}${margin ? ' with-margin' : ''}`} aria-label={label} role={label ? 'document' : undefined}>
      {blocks.map((b) => {
        const beside = margin?.(b)
        return (
          <div key={b.index} className={`md-row${b.heading ? ` h${b.heading.depth}` : ''}`} data-from={b.from} data-to={b.to}>
            <div className="md-b">{block(b.node, String(b.index), links)}</div>
            {margin && <div className="md-m">{beside}</div>}
          </div>
        )
      })}
    </div>
  )
}
