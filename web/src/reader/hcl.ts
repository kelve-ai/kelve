// HCL, Terraform's language, read line by line for the file view's colours:
// CodeMirror has no grammar of its own for it. Comments, strings with their
// `${…}` and `%{…}` parts coloured as code, heredocs, numbers, `true`,
// `false` and `null`, a block's type (`resource`, `variable`) and an
// attribute's name.

import type { StringStream, StreamParser } from '@codemirror/language'

type State = {
  /** Inside a string's quotes, rather than code. */
  string: boolean
  /** For each `${` open, how many of its own braces are open. */
  parts: number[]
  /** Inside a block comment. */
  comment: boolean
  /** Inside a heredoc: the word that ends it. */
  heredoc: string | null
}

const KEYWORDS = new Set(['for', 'in', 'if', 'else', 'endif', 'endfor'])
const ATOMS = new Set(['true', 'false', 'null'])

/** Whether the token just read is the first thing on its line. */
const first = (stream: StringStream) => stream.string.slice(0, stream.start).trim() === ''

/** A string's text, up to its closing quote, or the next `${`. */
function inString(stream: StringStream, state: State): string {
  while (!stream.eol()) {
    if (stream.match(/^[$%]\{/, false)) return 'string'
    const ch = stream.next()
    if (ch === '\\') stream.next()
    else if (ch === '"') {
      state.string = false
      return 'string'
    }
  }
  return 'string'
}

export const hcl: StreamParser<State> = {
  name: 'hcl',
  startState: () => ({ string: false, parts: [], comment: false, heredoc: null }),
  copyState: (s) => ({ ...s, parts: [...s.parts] }),
  token(stream, state) {
    if (state.heredoc) {
      if (stream.sol() && stream.match(new RegExp(`^\\s*${state.heredoc}\\s*$`))) state.heredoc = null
      else stream.skipToEnd()
      return 'string'
    }
    if (state.comment) {
      if (stream.skipTo('*/')) {
        stream.match('*/')
        state.comment = false
      } else stream.skipToEnd()
      return 'comment'
    }
    if (state.string) {
      if (stream.match(/^[$%]\{/)) {
        state.parts.push(0)
        state.string = false
        return 'meta'
      }
      return inString(stream, state)
    }
    if (stream.eatSpace()) return null
    if (stream.match('#') || stream.match('//')) {
      stream.skipToEnd()
      return 'comment'
    }
    if (stream.match('/*')) {
      state.comment = true
      return 'comment'
    }
    if (stream.eat('"')) {
      state.string = true
      return inString(stream, state)
    }
    const heredoc = stream.match(/^<<-?([A-Za-z_][\w-]*)/) as RegExpMatchArray | null
    if (heredoc) {
      state.heredoc = heredoc[1]
      stream.skipToEnd()
      return 'string'
    }
    if (stream.match(/^\d+(\.\d+)?([eE][+-]?\d+)?/)) return 'number'
    if (stream.match(/^[A-Za-z_][\w-]*/)) {
      const word = stream.current()
      if (ATOMS.has(word)) return 'atom'
      if (KEYWORDS.has(word)) return 'keyword'
      if (first(stream) && stream.match(/^\s*=(?!=)/, false)) return 'property'
      if (first(stream) && stream.match(/^\s+("|\{|[A-Za-z_][\w-]*\s*\{)/, false)) return 'keyword'
      if (stream.match(/^\s*\(/, false)) return 'variableName.function'
      return 'variable'
    }
    const ch = stream.next()
    if (ch === '{' || ch === '}') {
      const open = state.parts.length - 1
      if (open >= 0) {
        if (ch === '{') state.parts[open]++
        else if (state.parts[open] > 0) state.parts[open]--
        else {
          // The `}` that closes a string's `${`: back to the string.
          state.parts.pop()
          state.string = true
          return 'meta'
        }
      }
      return 'bracket'
    }
    if (ch && '[]()'.includes(ch)) return 'bracket'
    if (ch && '=!<>&|+-*/%?:'.includes(ch)) {
      stream.eatWhile(/[=<>&|]/)
      return 'operator'
    }
    return null
  },
  languageData: { commentTokens: { line: '#', block: { open: '/*', close: '*/' } } },
}
