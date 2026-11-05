// The example's skills, as the skills pages fetch them: Sam's Rust, which
// has a part of every state, Everywhere, Words, a procedure for adding a
// flag, the fetch project's own, and a suggested skill; and Fits you. With
// `as=empty` there are none; `as=confirm` draws Rust with three new parts
// not checked yet, `as=running` while they're checked; `as=fits-start` and
// `as=fits-waiting` draw Fits you before its first pair is ready.

import type { FitsView, PartCheck, SkillItem, SkillKind, SkillPartView, SkillSectionView, SkillView, SkillsView, SuggestedSkill, WorkKind } from '../api/types'

type Part = { md: string; check?: PartCheck; hint?: string; progress?: [number, number]; failed?: [string, string]; kept?: boolean }
type Section = { title: string; parts: Part[] }

const KINDS: Record<string, 'howYouLikeIt' | 'steps' | 'watchOutFor'> = {
  'How you like it': 'howYouLikeIt',
  Steps: 'steps',
  'Watch out for': 'watchOutFor',
}

/** A heading as kelve knows a part by. */
const key = (name: string, md: string) => `${name}#${md.split('\n')[0].replace(/^### /, '').replace(/^\d+\.\s+/, '').toLowerCase()}`

/** A skill's SKILL.md, written from its parts, with the line each part's heading is on. */
function skill(id: string, name: string, title: string, intro: string, description: string, kind: SkillKind, work: WorkKind | null, sections: Section[], project: string | null = null): SkillView {
  const lines = ['---', `name: ${name}`, `description: ${description}`, '---', '', `# ${title}`, '', intro, '']
  const views: SkillSectionView[] = sections.map((sec) => {
    lines.push(`## ${sec.title}`, '')
    const parts: SkillPartView[] = sec.parts.map((p) => {
      const line = lines.length + 1
      lines.push(...p.md.split('\n'), '')
      return {
        key: key(name, p.md),
        heading: p.md.split('\n')[0].replace(/^### /, ''),
        markdown: p.md,
        line,
        check: p.check ?? 'notChecked',
        kept: p.kept ?? false,
        hint: p.hint ?? '',
        progress: p.progress ? { done: p.progress[0], of: p.progress[1] } : null,
        failed: p.failed ? { what: p.failed[0], wording: p.failed[1] } : null,
        evidence: [],
      }
    })
    return { title: sec.title, kind: KINDS[sec.title] ?? null, parts }
  })
  while (lines[lines.length - 1] === '') lines.pop()
  const unchecked = views.flatMap((s) => s.parts).filter((p) => p.check === 'notChecked')
  return {
    id,
    name,
    title,
    intro,
    description,
    kind,
    work,
    project,
    markdown: `${lines.join('\n')}\n`,
    sections: views,
    lines: lines.length,
    mostLines: 150,
    problems: [],
    offer: { parts: unchecked.map((p) => ({ key: p.key, heading: p.heading })), tasks: 2, runs: unchecked.length * 4, when: 'ask', left: 10, waiting: 0, held: null },
    running: null,
  }
}

const ERRORS = '### Errors are one enum, with a message for each\n\nA string passed around loses what went wrong. `Display` turns each variant into a sentence a person can act on.\n\n```rust\npub enum FetchError {\n    Timeout,\n    Status(u16),\n}\n```'
const EXIT = '### Bad arguments exit 1, the tool’s own failures exit 2\n\nSo scripts can tell “you called it wrong” from “it broke”.'
const DURATION = '### A duration is a Duration, never a bare number\n\nA `u64` doesn’t say whether it means seconds or milliseconds.'
const ASYNC = '### Never block inside async code\n\nA `std::fs` call in an `async fn` stalls every other task on its thread. Use `tokio::fs`.'

function rust(as: string | null): SkillView {
  const intro = 'Rust that fails clearly and reads plainly, in any project.'
  const description = 'How Sam wants Rust written. Use when reading, writing or reviewing Rust.'
  if (as === 'confirm' || as === 'running') {
    const now: Part = as === 'running' ? { md: '', check: 'checking', progress: [1, 2] } : { md: '' }
    const view = skill('rust', 'rust', 'Rust', intro, description, 'like', 'rust', [
      {
        title: 'How you like it',
        parts: [{ md: ERRORS, check: 'works', hint: 'Better on 2 of 2 past tasks' }, { ...now, md: EXIT }, { ...now, md: DURATION }, { ...now, md: ASYNC }],
      },
    ])
    if (as === 'running') view.running = { parts: 3, done: 6, runs: 12 }
    return view
  }
  return skill('rust', 'rust', 'Rust', intro, description, 'like', 'rust', [
    {
      title: 'How you like it',
      parts: [
        { md: ERRORS, check: 'works', hint: 'Better on 2 of 2 past tasks' },
        { md: EXIT, check: 'checking', progress: [1, 2] },
        { md: DURATION },
        { md: '### Tests sit beside the code, named as sentences\n\nLike `retries_stop_after_five_tries`, so a failing test says what broke.', check: 'works', hint: 'Better on 1 of 2 past tasks, the same on 1' },
      ],
    },
    {
      title: 'Watch out for',
      parts: [
        {
          md: '### Don’t silence warnings\n\nCI treats them as errors.',
          check: 'failed',
          hint: 'Not followed on 1 of 2 past tasks',
          failed: [
            'With the part, your agent still didn’t do what it says: it added `#[allow]` to quiet a warning. Clearer wording:',
            '### Fix every warning in the code itself\n\nNever add `#[allow]` to quiet one. If the lint is wrong, say why in a comment.',
          ],
        },
        { md: '### Never unwrap outside tests\n\nReturn the error instead.', check: 'noDifference', hint: 'The same on both past tasks, with or without it' },
      ],
    },
  ])
}

const EVERYWHERE = skill('everywhere', 'everywhere', 'Everywhere', 'How work is done in any project.', 'How Sam likes work done, in any project. Use always.', 'like', 'everywhere', [
  {
    title: 'How you like it',
    parts: [
      { md: '### British English, in short sentences\n\nIn anything people read.', check: 'works', hint: 'Better on 2 of 2 past tasks' },
      { md: '### Say what changed and why before handing over\n\nSo the change can be reviewed without reading every line.', check: 'works', hint: 'Better on 2 of 2 past tasks' },
    ],
  },
])

const WORDS = skill('words', 'words', 'Words', 'For the words people see in an app.', 'How Sam wants app copy written. Use when writing words people see.', 'like', 'words', [
  {
    title: 'How you like it',
    parts: [
      { md: '### Buttons say what they do\n\nLike “Delete note”, never “OK”.', check: 'works', hint: 'Better on 2 of 2 past tasks' },
      { md: '### Errors say what happened and what to do next\n\nIn full sentences.', check: 'works', hint: 'Better on 1 of 2 past tasks, the same on 1' },
    ],
  },
])

const FLAG = skill('adding-a-command-line-flag', 'adding-a-command-line-flag', 'Adding a command-line flag', 'In any language.', 'The steps Sam follows to add a flag to a command-line tool. Use when adding a flag.', 'do', null, [
  {
    title: 'Steps',
    parts: [
      { md: '### 1. Add it to the Args struct, with a doc comment\n\nThe comment is what `--help` shows.', check: 'works', hint: 'Better on 2 of 2 past tasks' },
      { md: '### 2. Parse it, and test a bad value\n\nName the test for what the person sees.', check: 'works', hint: 'Better on 1 of 2 past tasks, the same on 1' },
      { md: '### 3. Add it to the README and the changelog\n\nUnder Unreleased, in a line that says what someone can now do.', check: 'works', hint: 'Better on 2 of 2 past tasks' },
    ],
  },
])

const FETCH = skill(
  '12',
  'fetch',
  'fetch',
  'For anyone who works on fetch, and their agents.',
  'How fetch is worked on. Use in the fetch repository.',
  'project',
  null,
  [
    {
      title: 'Watch out for',
      parts: [
        { md: '### The test server runs on port 4810\n\nNever `8080`, which the docs server uses.', check: 'works', hint: 'Better on 2 of 2 past tasks' },
        { md: '### Big test files are made, never committed\n\n`make fixtures` makes them.' },
      ],
    },
  ],
  'fetch',
)

/** A skill in the list. */
function item(s: SkillView): SkillItem {
  const parts = s.sections.flatMap((x) => x.parts)
  return {
    id: s.id,
    name: s.name,
    title: s.title,
    kind: s.kind,
    work: s.work,
    project: s.project,
    parts: parts.length,
    failed: parts.filter((p) => p.check === 'failed').length,
    checking: parts.some((p) => p.check === 'checking'),
  }
}

const RETRY = `---
name: retrying-network-calls
description: How Sam retries calls over the network. Use when a call over the network can fail.
---

# Retrying network calls

## How you like it

### Retry only failures that might pass

Timeouts, dropped connections and server errors often clear up. A 4xx won’t, so don’t retry it.

### Wait twice as long each time, plus a little at random

About 0.5, 1, 2, 4 and 8 seconds, so clients that failed together don’t come back together.

### Stop after five tries, and say why

Report the last error, so whoever runs it isn’t left waiting.

## Watch out for

### A 429 means wait, not stop

Wait at least as long as \`Retry-After\` asks.

### Only retry what’s safe to send twice

A download is. A payment isn’t.
`

function suggested(): SuggestedSkill {
  const lines = RETRY.split('\n')
  const sections: SkillSectionView[] = []
  let at: SkillSectionView | null = null
  lines.forEach((l, i) => {
    if (l.startsWith('## ')) {
      at = { title: l.slice(3), kind: KINDS[l.slice(3)] ?? null, parts: [] }
      sections.push(at)
    }
    if (l.startsWith('### ') && at) {
      const md = `${l}\n\n${lines[i + 2]}`
      at.parts.push({ key: key('retrying-network-calls', md), heading: l.slice(4), markdown: md, line: i + 1, check: 'notChecked', kept: false, hint: '', progress: null, failed: null, evidence: [] })
    }
  })
  return {
    id: '41',
    name: 'retrying-network-calls',
    title: 'Retrying network calls',
    description: 'How Sam retries calls over the network. Use when a call over the network can fail.',
    kind: 'do',
    why: 'A new skill from five of your decisions.',
    markdown: RETRY,
    sections,
    closeTo: null,
  }
}

const A = `diff --git a/src/fetch.rs b/src/fetch.rs
--- a/src/fetch.rs
+++ b/src/fetch.rs
@@ -1,9 +1,21 @@
-pub fn fetch(url: &str) -> Result<String, String> {
-    let res = get(url).map_err(|e| e.to_string())?;
+#[derive(Debug)]
+pub enum FetchError {
+    Timeout,
+    Status(u16),
+}
+
+impl std::fmt::Display for FetchError {
+    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
+        match self {
+            FetchError::Timeout => write!(f, "The server didn't answer in time. Try again."),
+            FetchError::Status(code) => write!(f, "The server said {code}."),
+        }
+    }
+}
+
+pub fn fetch(url: &str) -> Result<String, FetchError> {
+    let res = get(url)?;
     Ok(res.body)
 }
`

const B = `diff --git a/src/fetch.rs b/src/fetch.rs
--- a/src/fetch.rs
+++ b/src/fetch.rs
@@ -1,9 +1,9 @@
-pub fn fetch(url: &str) -> Result<String, String> {
-    let res = get(url).map_err(|e| e.to_string())?;
+pub fn fetch(url: &str) -> Result<String, Box<dyn std::error::Error>> {
+    let res = get(url)?;
     Ok(res.body)
 }
`

function fits(as: string | null): FitsView {
  const tests = [true, true, true, true, false, true, true, true, true, false, true]
  return {
    pick: as === 'fits-start' || as === 'fits-waiting' ? null : { id: 7, request: 'Make fetch’s errors say what went wrong, so the person running it knows what to do.', left: B, right: A },
    waiting: as === 'fits-waiting' ? 3 : 0,
    tasks: 6,
    tally: { picked: tests.filter(Boolean).length, of: tests.length, tests },
    closeness: { skills: 78, standard: 52 },
  }
}

/** The example's answers for the skills pages and Fits you. */
export function skillsExample(as: string | null): Record<string, unknown> {
  const r = rust(as)
  const all = [EVERYWHERE, r, WORDS, FLAG, FETCH]
  const list: SkillsView =
    as === 'empty'
      ? { suggested: [], like: [], doing: [], project: [] }
      : {
          suggested: [{ id: '41', title: 'Retrying network calls' }],
          like: [EVERYWHERE, r, WORDS].map(item),
          doing: [FLAG].map(item),
          project: [FETCH].map(item),
        }
  const out: Record<string, unknown> = { '/api/skills': list, '/api/suggested/41': suggested(), '/api/fits': fits(as) }
  for (const s of all) out[`/api/skills/${encodeURIComponent(s.id)}`] = s
  return out
}
