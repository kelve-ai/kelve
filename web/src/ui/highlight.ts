// Colours for code an entry quotes, in the language kelve says it's in
// (core's `code::LANGUAGES`). Only these languages are bundled; HCL,
// Terraform's language, is kelve's own, as highlight.js has none. Highlight.js
// escapes what it's given, so its output holds only its own spans.

import hljs from 'highlight.js/lib/core'
import bash from 'highlight.js/lib/languages/bash'
import c from 'highlight.js/lib/languages/c'
import clojure from 'highlight.js/lib/languages/clojure'
import cmake from 'highlight.js/lib/languages/cmake'
import coffeescript from 'highlight.js/lib/languages/coffeescript'
import cpp from 'highlight.js/lib/languages/cpp'
import crystal from 'highlight.js/lib/languages/crystal'
import csharp from 'highlight.js/lib/languages/csharp'
import css from 'highlight.js/lib/languages/css'
import d from 'highlight.js/lib/languages/d'
import dart from 'highlight.js/lib/languages/dart'
import diff from 'highlight.js/lib/languages/diff'
import dockerfile from 'highlight.js/lib/languages/dockerfile'
import elm from 'highlight.js/lib/languages/elm'
import erlang from 'highlight.js/lib/languages/erlang'
import fortran from 'highlight.js/lib/languages/fortran'
import fsharp from 'highlight.js/lib/languages/fsharp'
import gherkin from 'highlight.js/lib/languages/gherkin'
import go from 'highlight.js/lib/languages/go'
import groovy from 'highlight.js/lib/languages/groovy'
import haskell from 'highlight.js/lib/languages/haskell'
import ini from 'highlight.js/lib/languages/ini'
import java from 'highlight.js/lib/languages/java'
import javascript from 'highlight.js/lib/languages/javascript'
import json from 'highlight.js/lib/languages/json'
import julia from 'highlight.js/lib/languages/julia'
import kotlin from 'highlight.js/lib/languages/kotlin'
import lisp from 'highlight.js/lib/languages/lisp'
import livescript from 'highlight.js/lib/languages/livescript'
import lua from 'highlight.js/lib/languages/lua'
import markdown from 'highlight.js/lib/languages/markdown'
import objectivec from 'highlight.js/lib/languages/objectivec'
import ocaml from 'highlight.js/lib/languages/ocaml'
import pascal from 'highlight.js/lib/languages/delphi'
import perl from 'highlight.js/lib/languages/perl'
import php from 'highlight.js/lib/languages/php'
import powershell from 'highlight.js/lib/languages/powershell'
import properties from 'highlight.js/lib/languages/properties'
import protobuf from 'highlight.js/lib/languages/protobuf'
import python from 'highlight.js/lib/languages/python'
import r from 'highlight.js/lib/languages/r'
import ruby from 'highlight.js/lib/languages/ruby'
import rust from 'highlight.js/lib/languages/rust'
import scala from 'highlight.js/lib/languages/scala'
import scheme from 'highlight.js/lib/languages/scheme'
import sql from 'highlight.js/lib/languages/sql'
import stylus from 'highlight.js/lib/languages/stylus'
import swift from 'highlight.js/lib/languages/swift'
import tcl from 'highlight.js/lib/languages/tcl'
import typescript from 'highlight.js/lib/languages/typescript'
import vbnet from 'highlight.js/lib/languages/vbnet'
import verilog from 'highlight.js/lib/languages/verilog'
import vhdl from 'highlight.js/lib/languages/vhdl'
import xml from 'highlight.js/lib/languages/xml'
import yaml from 'highlight.js/lib/languages/yaml'
import type { HLJSApi, Language, LanguageFn } from 'highlight.js'

/** HCL, Terraform's language: comments, strings with their `${…}` parts, heredocs, a block's type and an attribute's name. */
function hcl(hljs: HLJSApi): Language {
  const code = { keyword: 'for in if else endif endfor', literal: 'true false null' }
  const part = { scope: 'subst', begin: /[$%]\{/, end: /\}/, keywords: code, contains: [hljs.NUMBER_MODE] }
  return {
    name: 'HCL',
    keywords: code,
    contains: [
      hljs.HASH_COMMENT_MODE,
      hljs.C_LINE_COMMENT_MODE,
      hljs.C_BLOCK_COMMENT_MODE,
      { scope: 'string', begin: /<<-?\s*([A-Za-z_][\w-]*)/, end: /^\s*[A-Za-z_][\w-]*\s*$/ },
      { scope: 'string', begin: '"', end: '"', contains: [hljs.BACKSLASH_ESCAPE, part] },
      hljs.NUMBER_MODE,
      { scope: 'attr', match: /(?<=^\s*)[A-Za-z_][\w-]*(?=\s*=(?!=))/ },
      { scope: 'keyword', match: /(?<=^\s*)[A-Za-z_][\w-]*(?=\s+(?:"|\{|[A-Za-z_][\w-]*\s*\{))/ },
      { scope: 'title.function', match: /[A-Za-z_][\w-]*(?=\()/ },
    ],
  }
}

const languages: Record<string, LanguageFn> = {
  bash, c, clojure, cmake, coffeescript, cpp, crystal, csharp, css, d, dart, diff, dockerfile, elm, erlang, fortran, fsharp, gherkin, go, groovy, haskell, ini, java, javascript, json, julia, kotlin, lisp, livescript, lua, markdown, objectivec, ocaml, pascal, perl, php, powershell, properties, protobuf, python, r, ruby, rust, scala, scheme, sql, stylus, swift, tcl, typescript, vbnet, verilog, vhdl, xml, yaml,
  terraform: hcl,
  hcl,
}
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
