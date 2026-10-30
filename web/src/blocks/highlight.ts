// Colours for code an entry quotes, in the language kelve says it's in
// (core's `code::LANGUAGES`). Only these languages are bundled. Highlight.js
// escapes what it's given, so its output holds only its own spans.

import hljs from 'highlight.js/lib/core'
import bash from 'highlight.js/lib/languages/bash'
import c from 'highlight.js/lib/languages/c'
import cpp from 'highlight.js/lib/languages/cpp'
import csharp from 'highlight.js/lib/languages/csharp'
import css from 'highlight.js/lib/languages/css'
import diff from 'highlight.js/lib/languages/diff'
import dockerfile from 'highlight.js/lib/languages/dockerfile'
import go from 'highlight.js/lib/languages/go'
import ini from 'highlight.js/lib/languages/ini'
import java from 'highlight.js/lib/languages/java'
import javascript from 'highlight.js/lib/languages/javascript'
import json from 'highlight.js/lib/languages/json'
import kotlin from 'highlight.js/lib/languages/kotlin'
import markdown from 'highlight.js/lib/languages/markdown'
import python from 'highlight.js/lib/languages/python'
import ruby from 'highlight.js/lib/languages/ruby'
import rust from 'highlight.js/lib/languages/rust'
import sql from 'highlight.js/lib/languages/sql'
import swift from 'highlight.js/lib/languages/swift'
import typescript from 'highlight.js/lib/languages/typescript'
import xml from 'highlight.js/lib/languages/xml'
import yaml from 'highlight.js/lib/languages/yaml'

const languages = { bash, c, cpp, csharp, css, diff, dockerfile, go, ini, java, javascript, json, kotlin, markdown, python, ruby, rust, sql, swift, typescript, xml, yaml }
for (const [name, language] of Object.entries(languages)) hljs.registerLanguage(name, language)

/** Whether kelve colours this language. */
export const colours = (lang: string | null | undefined): lang is string => !!lang && !!hljs.getLanguage(lang)

/**
 * Each line coloured as HTML, the lines highlighted together so a comment or
 * string that spans lines stays coloured across them. Spans open at a line's
 * end are closed there and opened again on the next.
 */
export function highlight(lines: string[], lang: string): string[] {
  const html = hljs.highlight(lines.join('\n'), { language: lang, ignoreIllegals: true }).value
  const out: string[] = []
  let open: string[] = []
  for (const line of html.split('\n')) {
    let text = open.join('') + line
    for (const tag of line.match(/<span[^>]*>|<\/span>/g) ?? []) {
      if (tag === '</span>') open.pop()
      else open.push(tag)
    }
    text += '</span>'.repeat(open.length)
    out.push(text)
  }
  open = []
  return out
}
