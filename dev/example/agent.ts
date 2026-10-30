// The example's agent: it does kelve's jobs on the ledger example the way
// a person's agent would, from what it was given to know (dev/example/agent/),
// through the same calls `kelve mcp` makes. Its marks, relations and entries
// are real: every entry quotes its pages exactly. It needs no model, so the
// sandbox, the states gallery and the end-to-end tests all read the same
// believable content.

import { readFileSync } from 'node:fs'
import { join } from 'node:path'

const here = join(import.meta.dirname, 'agent')
const load = <T>(name: string): T => JSON.parse(readFileSync(join(here, name), 'utf8')) as T

type MarkData = { file: string; words?: string; from?: string; through?: string; idea: string; use: string }
type Rel = [string, string]
type Relation = { question: string; restsOn?: Rel[]; kindOf?: Rel[]; answers?: Rel[]; alternatives?: Rel[]; usedWith?: Rel[] }
type BlockData = { id: string; part: string; fold?: string; follows?: string; type: string; text: string }
type ClaimData = { text: string; sources: number[]; quotes: Array<{ source: number; text: string }>; block: string; words: string }
type SourceData = { n: number; url: string; title: string; publisher: string; published: string }
type EntryData = {
  idea: string
  live: boolean
  minutes: number
  blocks: BlockData[]
  claims: ClaimData[]
  sources: SourceData[]
  /** What it adds when a reader asks more of the entry. */
  extend?: { blocks: BlockData[]; claims: ClaimData[]; sources: SourceData[] }
}

const MARKS = load<{ marks: MarkData[]; ideas: Record<string, string> }>('marks.json')
const RELATIONS = load<{ relations: Record<string, Relation>; definitions: Record<string, string> }>('relations.json')
const ENTRIES = load<{ entries: EntryData[] }>('entries.json').entries
const WAY_IN = load<{ order: Array<{ path: string; why: string }>; folders: Array<{ path: string; line: string }> }>('way.json')
type PlainPart = { from: number; to: number; goal: string; says: string; before?: string; also?: boolean }
type PlainFile = { what: string; change?: string; parts: PlainPart[]; out?: Array<{ goal: string; says: string }> }
type FolderPart = { part: string; goal: string; says: string; before?: string }
type PlainFolder = { what: string; change?: string; shape: 'steps' | 'parts'; parts: FolderPart[]; also?: { parts: string[]; says: string }; keeps?: Array<{ says: string; by: string }> }
type ChangePart = { mark: string; goal: string; says: string; before?: string; files: string[] }
type PlainChange = { what: string; fits?: string; kind: string; change?: string; parts: ChangePart[]; touches?: string[] }
const PLAINLY = load<{ files: Record<string, PlainFile>; folders: Record<string, PlainFolder>; change: PlainChange }>('plainly.json')

/** The way in the example's agent gives with its change: the order, and what each folder is for. */
export const WAY = { order: WAY_IN.order, folders: WAY_IN.folders }

/** What an idea is, in a line. */
export function definition(name: string): string {
  return MARKS.ideas[name] ?? RELATIONS.definitions[name] ?? `${name}, as the ledger example uses it.`
}

/** The entries the example graph doesn't have: the agent writes these while you watch. */
export const LIVE = ENTRIES.filter((e) => e.live).map((e) => e.idea)

/** Every idea the example's agent can write an entry for. */
export const WRITTEN = ENTRIES.map((e) => e.idea)

/** Every page an entry quotes, and the words it quotes there: what the stand-in graph checks quotes against. */
export function pages(): Array<[string, string]> {
  const byUrl = new Map<string, string[]>()
  for (const e of ENTRIES) {
    for (const c of [...e.claims, ...(e.extend?.claims ?? [])]) {
      for (const q of c.quotes) {
        const url = [...e.sources, ...(e.extend?.sources ?? [])].find((s) => s.n === q.source)?.url
        if (!url) continue
        const said = byUrl.get(url) ?? []
        said.push(q.text)
        byUrl.set(url, said)
      }
    }
  }
  return [...byUrl].map(([url, quotes]) => [url, quotes.join('\n\n')])
}

export type Job = { job: number; kind: string; about: string; material: Record<string, unknown>; waiting: number }

/** The text inside a fence, without its tags. */
function unfenced(text: unknown): string {
  const t = typeof text === 'string' ? text : ''
  const body = t.includes('\n') ? t.slice(t.indexOf('\n') + 1) : t
  return body.includes('\n') ? body.slice(0, body.lastIndexOf('\n')) : body
}

/** The idea a job is about, from its material: "The idea: Jitter". */
function ideaOf(job: Job): string {
  return /The idea: ([^\n]+)/.exec(unfenced(job.material.idea))?.[1]?.trim() ?? ''
}

function read(job: Job) {
  const file = String(job.material.file ?? '')
  const mine = MARKS.marks.filter((m) => m.file === file)
  const marked = new Set<string>()
  const passages = (job.material.passages as Array<{ id: string; passage: string; most?: number }>).map((p) => {
    const lines = unfenced(p.passage)
      .split('\n')
      .map((l) => /^\s*(\d+)  (.*)$/.exec(l))
      .filter((m): m is RegExpExecArray => !!m)
      .map((m) => ({ n: Number(m[1]), text: m[2] }))
    const marks: unknown[] = []
    for (const m of mine) {
      if (marked.has(m.idea) || marks.length >= (p.most ?? 3)) continue
      const base = { candidate: '', name: m.idea, definition: definition(m.idea), use: m.use }
      if (m.words) {
        const at = lines.find((l) => l.text.includes(m.words!))
        if (!at) continue
        marks.push({ ...base, line: at.n, quote: m.words, lastLine: 0 })
      } else if (m.from && m.through) {
        const a = lines.find((l) => l.text.includes(m.from!))
        const b = lines.find((l) => l.text.includes(m.through!))
        if (!a || !b) continue
        marks.push({ ...base, line: a.n, quote: '', lastLine: b.n })
      }
      marked.add(m.idea)
    }
    return { passage: p.id, marks }
  })
  return { passages }
}

function choose(job: Job) {
  const checks = (job.material.marks as string[]).map((m) => {
    const n = Number(/number="(\d+)"/.exec(m)?.[1] ?? 0)
    const name = /name="([^"]*)"/.exec(m)?.[1] ?? ''
    // The graph's idea of the same name, when it offered one.
    const offered = [...m.matchAll(/^(\d+)\. ([^:]+):/gm)]
    const same = offered.find((o) => o[2].trim().toLowerCase() === name.toLowerCase())
    return { mark: n, named: true, instance: true, matters: true, idea: same ? Number(same[1]) : 0 }
  })
  return { checks }
}

function builtOn(job: Job) {
  const listed = unfenced(job.material.ideas).split('\n').filter((l) => /^\d+\./.test(l.trim()))
  return { chosen: listed.slice(0, 5).map((_, i) => ({ n: i + 1, why: 'What the ledger does with its money rests on it.' })) }
}

function relate(job: Job) {
  const idea = ideaOf(job)
  const r = RELATIONS.relations[idea]
  const each = (rels?: Rel[]) => (rels ?? []).map(([name, why]) => ({ name, definition: definition(name), why }))
  return {
    question: r?.question ?? `What is ${idea.toLowerCase()}?`,
    restsOn: each(r?.restsOn),
    kindOf: each(r?.kindOf),
    answers: each(r?.answers),
    alternatives: each(r?.alternatives),
    usedWith: each(r?.usedWith),
  }
}

/** An idea's entry, as the write job's answer has it. */
export function write(job: Job) {
  const idea = ideaOf(job)
  const e = ENTRIES.find((x) => x.idea.toLowerCase() === idea.toLowerCase())
  if (!e) throw new Error(`the example's agent has no entry for ${idea}: add one to dev/example/agent/entries.json`)
  return {
    title: e.idea,
    minutes: e.minutes,
    blocks: e.blocks.map((b) => ({ fold: '', ...b, data: '{}' })),
    links: [],
    claims: e.claims,
    sources: e.sources,
    drop: [],
  }
}

/** What it adds to an idea's entry when readers ask more of it, as the extend job's answer has it. */
export function extend(job: Job) {
  const idea = ideaOf(job)
  const e = ENTRIES.find((x) => x.idea.toLowerCase() === idea.toLowerCase())?.extend
  if (!e) throw new Error(`the example's agent has nothing to add to ${idea}: add an extend to its entry in dev/example/agent/entries.json`)
  return {
    blocks: e.blocks.map((b) => ({ fold: '', follows: '', ...b, data: '{}' })),
    links: [],
    claims: e.claims,
    sources: e.sources,
    drop: [],
  }
}

/** Runs of lines that aren't blank, at most `most` of them: a file's parts when the agent has nothing better. */
function runsOf(count: number, blank: Set<number>, most = 60): Array<[number, number]> {
  const runs: Array<[number, number]> = []
  for (let n = 1; n <= count; n++) {
    if (blank.has(n)) continue
    const last = runs[runs.length - 1]
    if (last && last[1] === n - 1) last[1] = n
    else runs.push([n, n])
  }
  const each = Math.ceil(runs.length / most)
  if (each <= 1) return runs
  const merged: Array<[number, number]> = []
  for (let i = 0; i < runs.length; i += each) merged.push([runs[i][0], runs[Math.min(i + each, runs.length) - 1][1]])
  return merged
}

/** A file read plainly: as the example's agent says it, kept to what this job gave; else a part for each run of lines. */
function plainly(job: Job) {
  const m = job.material
  const path = String(m.path ?? '')
  const name = path.split('/').pop() ?? path
  const count = Number(m.lineCount ?? 0)
  const blank = new Set((m.blankLines as number[]) ?? [])
  const change = m.change as { added?: Array<[number, number]>; takenOutAfter?: number[] } | null
  const added = new Set((change?.added ?? []).flatMap(([a, b]) => Array.from({ length: b - a + 1 }, (_, i) => a + i)))
  const outAfter = change?.takenOutAfter ?? []
  // As kelve tells it: lines the change added, or took out at its edges.
  const touched = (from: number, to: number) => [...added].some((n) => n >= from && n <= to) || outAfter.some((k) => k >= from - 1 && k <= to)
  const said = PLAINLY.files[path]
  if (said && Math.max(...said.parts.map((p) => p.to)) <= count) {
    const parts = said.parts.map((p) => ({ ...p }))
    // Lines added since, at the end, are in the last part.
    const last = parts.reduce((a, b) => (b.to > a.to ? b : a))
    last.to = Math.max(last.to, count)
    return {
      what: said.what,
      parts: parts.map(({ before, ...p }) => (change && before && touched(p.from, p.to) ? { ...p, before } : p)),
      ...(change && said.change ? { change: said.change } : {}),
      ...(change && said.out ? { out: said.out } : {}),
    }
  }
  return {
    what: `What ${name} is for, as the example’s agent reads it.`,
    parts: runsOf(count, blank).map(([from, to], i) => ({ from, to, goal: `Part ${i + 1} of ${name}`.split(' ').slice(0, 6).join(' '), says: `What lines ${from} to ${to} do, as the example’s agent reads them.` })),
  }
}

/** A folder read plainly: as the example's agent says it, kept to the parts and changes this job gave. */
function plainlyFolder(job: Job) {
  const m = job.material
  const parts = (m.parts as Array<{ path: string; folder: boolean }>) ?? []
  const uses = (m.uses as Array<[number, number]>) ?? []
  const changed = m.changed as Array<[number, number, boolean]> | null
  const at = (p: string) => parts.findIndex((x) => x.path === p)
  const said = PLAINLY.folders[String(m.path ?? '')]
  const kept = (said?.parts ?? []).filter((p) => at(p.part) >= 0)
  if (said && kept.length > 0) {
    const leads = (a: string, b: string) => uses.some(([u, v]) => (u === at(a) && v === at(b)) || (u === at(b) && v === at(a)))
    const steps = said.shape === 'steps' && kept.every((p, i) => i === 0 || leads(kept[i - 1].part, p.part))
    return {
      what: said.what,
      ...(changed && said.change ? { change: said.change } : {}),
      shape: steps ? 'steps' : 'parts',
      parts: kept.map(({ before, ...p }) => {
        const c = changed?.[at(p.part)]
        return c && before && c[0] + c[1] > 0 ? { ...p, before } : p
      }),
      ...(said.also ? { also: { parts: said.also.parts.filter((p) => at(p) >= 0), says: said.also.says } } : {}),
      keeps: (said.keeps ?? []).filter((k) => at(k.by) >= 0),
    }
  }
  return {
    what: 'What this folder holds, as the example’s agent reads it.',
    shape: 'parts',
    parts: parts.slice(0, 7).map((p) => ({ part: p.path, goal: 'Holds a part of it', says: `What ${p.path} does, as the example’s agent reads it.` })),
  }
}

/** A change read plainly: as the example's agent says it, kept to the files this job gave. */
function plainlyChange(job: Job) {
  const files = (job.material.files as Array<{ path: string; movedFrom?: string | null }>) ?? []
  const has = (f: string) => files.some((x) => x.path === f)
  const moved = (f: string) => !!files.find((x) => x.path === f)?.movedFrom
  const said = PLAINLY.change
  const parts = said.parts
    .map((p) => ({ ...p, files: p.files.filter(has) }))
    .filter((p) => p.files.length > 0 && (p.mark !== 'moved' || p.files.every(moved)))
  if (parts.length > 0) return { ...said, parts }
  return {
    what: 'What this change does, as the example’s agent reads it.',
    kind: 'updates',
    parts: files.slice(0, 7).map((f) => ({ mark: f.movedFrom ? 'moved' : 'changed', goal: 'Changes a file', says: `What changed in ${f.path}.`, files: [f.path] })),
  }
}

/** The way into what it opened, as the example's agent finds it: its order, kept to the files the job lists, then the rest in the order given; and its folders' lines. */
function way(job: Job) {
  const files = (job.material.files as string[]) ?? []
  const said = WAY.order.filter((s) => files.includes(s.path))
  const rest = files.filter((f) => !said.some((s) => s.path === f)).slice(0, Math.max(0, 40 - said.length))
  return { order: [...said, ...rest.map((path) => ({ path, why: `Where ${path} fits.` }))], parts: [], folders: folders(job).folders }
}

/** What each of the example's folders is for, as it says when it finds the way in. */
function folders(job: Job) {
  const asked = (job.material.folders as string[]) ?? []
  return { folders: WAY.folders.filter((f) => asked.includes(f.path)) }
}

type LessonData = { about: string[]; says: string; do: string; example?: string; holdsWhen: string; seenAs?: string }
/** The example's lessons: other agents', the story its agent meets, and what it learns. */
export const LESSONS = load<{
  theirs: Array<LessonData & { record: { held: number; didnt: number; backed?: boolean }; card?: { cues?: string[]; codes?: string[]; variants?: string[] }; contradicts?: number[] }>
  story: Array<Record<string, unknown>>
  learned: { lessons: LessonData[]; same: string[] }
}>('lessons.json')

/**
 * Other agents' lessons as the stand-in graph is seeded with them, in order,
 * their ideas by the graph's IDs: an empty stand-in numbers them from one,
 * so another way names an earlier one by its place.
 */
export function lessonSeeds(ids: Map<string, string>) {
  const id = (place: number) => `kelve:l${(place + 1).toString(16).padStart(12, '0')}`
  return LESSONS.theirs.map(({ record, about, card, contradicts, ...l }) => ({
    lesson: { lesson: { ...l, about: about.map((name) => ids.get(name) ?? name) }, record: { backed: false, ...record }, card, contradicts: (contradicts ?? []).map(id) },
  }))
}

/** What the example's agent learned: only about the ideas it's offered. */
function lesson(job: Job) {
  const offered = ((job.material.offered as Array<{ name: string }>) ?? []).map((i) => i.name)
  return { lessons: LESSONS.learned.lessons.filter((l) => l.about.every((a) => offered.includes(a))), same: LESSONS.learned.same }
}

/** How the example's agent answers a job. */
export function answer(job: Job): unknown {
  switch (job.kind) {
    case 'read': return read(job)
    case 'choose': return choose(job)
    case 'builtOn': return builtOn(job)
    case 'relate': return relate(job)
    case 'write': return write(job)
    case 'extend': return extend(job)
    case 'folders': return folders(job)
    case 'way': return way(job)
    case 'path': return { parts: [] }
    case 'plainly': return plainly(job)
    case 'plainlyFolder': return plainlyFolder(job)
    case 'plainlyChange': return plainlyChange(job)
    case 'note': return { did: 'Did what the note asked.' }
    case 'lesson': return lesson(job)
    default: throw new Error(`a job the example's agent doesn't know: ${job.kind}`)
  }
}
