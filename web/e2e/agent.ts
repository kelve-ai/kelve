// The tests' stand-in for the person's own agent: it says hello, takes
// kelve's jobs and hands in answers, through the same calls `kelve mcp`
// makes, with the command line's secret. It answers from what's in each
// job, the way the browser tests need: not well, but always, and at once.
//
// It knows a few ideas by the words that use them (IDEAS), and the way
// down from each to a first principle (DOWN).

/** Words in a file, and the idea they use. */
export const IDEAS: Array<{ words: string; name: string; definition: string }> = [
  { words: 'pg_advisory_xact_lock', name: 'Advisory locks', definition: 'A lock a program names, and the database keeps.' },
  { words: 'released on commit', name: 'Transaction-scoped locks', definition: 'A lock let go when its transaction ends.' },
  { words: 'heat pump', name: 'Heat pump', definition: 'A machine that moves heat from a cold place to a warm one.' },
  { words: 'backoff', name: 'Exponential backoff', definition: 'Waiting longer after each failure before trying again.' },
]

/**
 * What each idea rests on, down to a first principle, which rests on
 * nothing. Any other idea rests on a database transaction.
 */
export const DOWN: Record<string, string> = {
  'Database transaction': 'Atomicity',
  Atomicity: 'State',
  State: '',
  'Heat pump': 'Heat',
  Heat: '',
  'Exponential backoff': 'Retrying',
  Retrying: 'State',
}

/** The page the stand-in agent writes every entry from, and what it says. */
export const PAGE = 'https://www.postgresql.org/docs/current/explicit-locking.html'
export const SAYS = 'PostgreSQL provides a means for creating locks that have application-defined meanings. These are called advisory locks.'

export type Job = { job: number; kind: string; about: string; material: Record<string, unknown>; waiting: number }

/** The text inside a fence, without its tags. */
function unfenced(text: unknown): string {
  const t = typeof text === 'string' ? text : ''
  const body = t.includes('\n') ? t.slice(t.indexOf('\n') + 1) : t
  return body.includes('\n') ? body.slice(0, body.lastIndexOf('\n')) : body
}

function read(job: Job) {
  const passages = (job.material.passages as Array<{ id: string; passage: string }>).map((p) => {
    const marks: unknown[] = []
    for (const line of unfenced(p.passage).split('\n')) {
      const m = /^\s*(\d+)  (.*)$/.exec(line)
      if (!m) continue
      // A line that names its own idea: "// idea: Ledger part 3".
      const named = /idea: ([\w ]+)/.exec(m[2])
      if (named) {
        marks.push({ candidate: '', line: Number(m[1]), quote: named[0], lastLine: 0, name: named[1].trim(), definition: `${named[1].trim()}, as this file says.`, use: 'The file says so.' })
      }
      for (const idea of IDEAS) {
        const at = m[2].toLowerCase().indexOf(idea.words)
        if (at < 0) continue
        marks.push({
          candidate: '',
          line: Number(m[1]),
          quote: m[2].slice(at, at + idea.words.length),
          lastLine: 0,
          name: idea.name,
          definition: idea.definition,
          use: 'It’s what this line does.',
        })
      }
    }
    return { passage: p.id, marks }
  })
  return { passages }
}

function choose(job: Job) {
  const checks = (job.material.marks as string[]).map((m) => {
    const n = Number(/number="(\d+)"/.exec(m)?.[1] ?? 0)
    const name = /name="([^"]*)"/.exec(m)?.[1] ?? ''
    // The graph's idea of the same name, if it offered one.
    const offered = [...m.matchAll(/^(\d+)\. ([^:]+):/gm)]
    const same = offered.find((o) => o[2].trim().toLowerCase() === name.toLowerCase())
    return { mark: n, named: true, instance: true, matters: true, idea: same ? Number(same[1]) : 0 }
  })
  return { checks }
}

function builtOn(job: Job) {
  const listed = unfenced(job.material.ideas).split('\n').filter((l) => /^\d+\./.test(l.trim()))
  return { chosen: listed.slice(0, 3).map((_, i) => ({ n: i + 1, why: 'What its files rest on most.' })) }
}

function relate(job: Job) {
  const idea = /The idea: ([^\n.]+)/.exec(unfenced(job.material.idea))?.[1]?.trim() ?? ''
  const down = DOWN[idea] ?? 'Database transaction'
  return {
    question: `What does ${idea.toLowerCase()} answer?`,
    restsOn: down ? [{ name: down, definition: `What ${idea.toLowerCase()} rests on.`, why: 'It takes its guarantee from this.' }] : [],
    kindOf: [],
    answers: [],
    alternatives: [],
    usedWith: [],
  }
}

/** An entry, its one claim quoting the page as `quote`. */
export function entry(quote: string) {
  return {
    title: 'Locks a program names',
    minutes: 3,
    blocks: [{ id: 'b1', part: 'lead', fold: '', type: 'paragraph', data: '{}', text: 'A program can take a lock by a name it chooses, and the database keeps it^1.' }],
    links: [],
    claims: [{
      text: 'PostgreSQL lets a program take locks with meanings it defines.',
      sources: [1],
      quotes: [{ source: 1, text: quote }],
      block: 'b1',
      words: 'A program can take a lock by a name it chooses',
    }],
    sources: [{ n: 1, url: PAGE, title: 'Explicit locking', publisher: 'PostgreSQL', published: '' }],
    drop: [],
  }
}

/** A fix: the lead given again, its claim quoting the page as `quote`, nothing else touched. */
export function fix(quote: string) {
  const { title: _title, minutes: _minutes, ...rest } = entry(quote)
  return {
    ...rest,
    blocks: rest.blocks.map((b) => ({ ...b, text: 'A program can take a lock by a name it chooses^1, and the database keeps it.' })),
    sources: [],
    remove: [],
  }
}

/** An extend: how it works, shown, with a worked example folded under it, touching nothing there. */
export function extend() {
  return {
    blocks: [
      { id: 'b2', part: 'how', fold: '', follows: 'b1', type: 'paragraph', data: '{}', text: 'Each program names its lock by a number, and waits while another holds it.' },
      { id: 'b3', part: 'how', fold: 'example', follows: 'b2', type: 'paragraph', data: '{}', text: 'Two workers both ask for lock 42. The first gets it; the second waits until it is let go.' },
    ],
    claims: [],
    links: [],
    sources: [],
    drop: [],
  }
}

/** A file read plainly: each run of lines between blank ones is a part, said in a line, resting on the ideas marked on its lines; in a change, what it was where it changed. A code file's last part, of several, is set-up, for Also here. */
export function plainly(job: Job) {
  const count = Number(job.material.lineCount ?? 0)
  const blank = new Set((job.material.blankLines as number[]) ?? [])
  const change = job.material.change as { added?: Array<[number, number]>; takenOutAfter?: number[] } | null
  const added = (change?.added ?? []).flatMap(([a, b]) => Array.from({ length: b - a + 1 }, (_, i) => a + i))
  const outAfter = change?.takenOutAfter ?? []
  const touched = (from: number, to: number) => added.some((n) => n >= from && n <= to) || outAfter.some((k) => k >= from - 1 && k <= to)
  const runs: Array<{ from: number; to: number }> = []
  for (let n = 1; n <= count; n++) {
    if (blank.has(n)) continue
    const last = runs[runs.length - 1]
    if (last && last.to === n - 1) last.to = n
    else runs.push({ from: n, to: n })
  }
  // At most 60 parts: runs side by side are one.
  const each = Math.ceil(runs.length / 60)
  const parts = each <= 1 ? runs : runs.filter((_, i) => i % each === 0).map((r, i) => ({ from: r.from, to: runs[Math.min((i + 1) * each, runs.length) - 1].to }))
  // The ideas marked, as listed: "Exponential backoff, lines 2-3: …".
  const ideas = unfenced(job.material.ideas)
    .split('\n')
    .map((l) => /^(.+?), lines? (\d+)(?:-(\d+))?:/.exec(l.trim()))
    .filter((m): m is RegExpExecArray => !!m)
    .map((m) => ({ name: m[1], from: Number(m[2]), to: Number(m[3] ?? m[2]) }))
  const code = job.material.code !== false
  return {
    what: 'Does what its lines say, as the stand-in agent reads it.',
    ...(change ? { change: 'It changed, as the stand-in agent reads it.' } : {}),
    parts: parts.map((p, i) => {
      const on = ideas.find((d) => d.from <= p.to && d.to >= p.from)
      return {
        ...p,
        goal: `Does lines ${p.from} to ${p.to}`,
        says: `Does what lines ${p.from} to ${p.to} say, as the stand-in agent reads them.${on ? ` It rests on [[${on.name}]].` : ''}`,
        ...(change && touched(p.from, p.to) ? { before: 'Did less, before the change.' } : {}),
        ...(code && parts.length > 1 && i === parts.length - 1 ? { also: true } : {}),
      }
    }),
  }
}

/** A folder read plainly: its first parts, each said in a line. */
export function plainlyFolder(job: Job) {
  const parts = (job.material.parts as Array<{ path: string }>) ?? []
  return {
    what: 'Holds what its parts do, as the stand-in agent reads it.',
    shape: 'parts',
    parts: parts.slice(0, 7).map((p) => ({ part: p.path, goal: 'Holds a part', says: `What ${p.path} holds.` })),
  }
}

/** A change read plainly: a part for each of its first files. */
export function plainlyChange(job: Job) {
  const files = (job.material.files as Array<{ path: string; movedFrom?: string | null }>) ?? []
  return {
    what: 'Changes what its files do, as the stand-in agent reads it.',
    kind: 'updates',
    parts: files.slice(0, 7).map((f) => ({ mark: f.movedFrom ? 'moved' : 'changed', goal: 'Changes a file', says: `What changed in ${f.path}.`, files: [f.path] })),
  }
}

/** A line for each folder a job asks about. */
function folderLines(job: Job) {
  return ((job.material.folders as string[]) ?? []).map((path) => ({ path, line: `What ${path.split('/').pop()} holds, said by the stand-in agent.` }))
}

/** How the stand-in agent answers a job. `quote` is what its entries quote from the page. */
export function answer(job: Job, quote = 'PostgreSQL provides a means for creating locks that have application-defined meanings.'): unknown {
  switch (job.kind) {
    case 'read': return read(job)
    case 'choose': return choose(job)
    case 'builtOn': return builtOn(job)
    case 'relate': return relate(job)
    case 'write': return entry(quote)
    case 'fix': return fix(quote)
    case 'extend': return extend()
    case 'plainly': return plainly(job)
    case 'plainlyFolder': return plainlyFolder(job)
    case 'plainlyChange': return plainlyChange(job)
    case 'note': return { did: 'Did what the note asked, as the stand-in agent.' }
    case 'folders': return { folders: folderLines(job) }
    case 'way': return { order: ((job.material.files as string[]) ?? []).slice(0, 40).map((path) => ({ path, why: `Why to read ${path} now, said by the stand-in agent.` })), parts: [], folders: folderLines(job) }
    case 'path': return { parts: [] }
    default: throw new Error(`a job the stand-in agent doesn't know: ${job.kind}`)
  }
}

export type Agent = {
  /** Says hello, as `kelve mcp` does when the agent starts it. */
  hello: () => Promise<void>
  /** Asks kelve to open something, as the `open` tool does: what it says back. */
  open: (what: Record<string, unknown>) => Promise<{ title: string; files: number; folders: number; toRead: number; shown: boolean }>
  /**
   * Does kelve's work until there's none left: the jobs it was given. With
   * `until`, it stops at the first job of that kind, having taken it.
   */
  work: (opts?: { quote?: string; until?: string }) => Promise<Job[]>
  /** Finds the way in, kelve's first job for anything opened with files in it, and takes no other. */
  findWay: () => Promise<void>
  /** The notes waiting for it in the project at `path`, as the notes tool reads them. */
  notes: (path: string) => Promise<Array<{ id: string; version: string; note: string; file: string | null; textNow: string | null; doneBefore: { text: string; did: string } | null; added: string[] }>>
  /** Says a note is done, for the version it read, and what it did, as note_done does. */
  noteDone: (path: string, note: string, version: string, did: string) => Promise<void>
  /** Writes what kelve marks for the person, as set_marks does. */
  setMarks: (spec: Record<string, unknown>) => Promise<void>
}

/**
 * The stand-in agent, calling kelve at `base` with the command line's `key`.
 * `answering` answers its jobs: the tests' own, or the example's agent's.
 */
export function agent(base: string, key: string, name = 'claude-code', answering: (job: Job, quote?: string) => unknown = answer): Agent {
  const caller = { agent: name, model: 'stand-in', plan: 'pro' }
  const call = async <T>(path: string, body: unknown): Promise<T> => {
    const res = await fetch(`${base}${path}`, {
      method: 'POST',
      headers: { authorization: `Bearer ${key}`, 'content-type': 'application/json' },
      body: JSON.stringify(body),
    })
    if (!res.ok) throw new Error(`kelve said no to ${path}: ${await res.text()}`)
    return (await res.json()) as T
  }
  return {
    hello: async () => {
      await call('/api/agent/hello', caller)
    },
    open: (what) => call('/api/agent/open', { ...caller, ...what }),
    findWay: async () => {
      const job = await call<Job | null>('/api/agent/next', caller)
      if (!job) return
      if (job.kind !== 'way') throw new Error(`kelve’s first job wasn’t finding the way in: ${job.about}`)
      const handed: { taken: string; why?: string } = await call('/api/agent/answer', { ...caller, job: job.job, answer: answering(job), more: false })
      if (handed.taken !== 'yes') throw new Error(`kelve didn’t take the way in: ${handed.why}`)
    },
    notes: async (path) => {
      const res = await fetch(`${base}/api/agent/notes?path=${encodeURIComponent(path)}`, { headers: { authorization: `Bearer ${key}` } })
      if (!res.ok) throw new Error(`kelve said no to the notes: ${await res.text()}`)
      return res.json()
    },
    noteDone: async (path, note, version, did) => {
      await call('/api/agent/note-done', { path, note, version, did })
    },
    setMarks: async (spec) => {
      await call('/api/agent/marks', { spec })
    },
    work: async ({ quote, until } = {}) => {
      const given: Job[] = []
      // Some work is scheduled a moment ahead, like following the graph's
      // check: kelve is asked again until it's been quiet for a while.
      let quiet = 0
      // An answer taken comes back with the next job, already the agent's.
      let next: Job | null = null
      while (quiet < 6) {
        if (given.length > 400) throw new Error('kelve’s work never stopped')
        const job: Job | null = next ?? (await call<Job | null>('/api/agent/next', caller))
        next = null
        if (!job) {
          quiet += 1
          await new Promise((done) => setTimeout(done, 400))
          continue
        }
        quiet = 0
        if (job.kind === until) return [...given, job]
        const handed: { taken: string; why?: string; next: Job | null } = await call('/api/agent/answer', { ...caller, job: job.job, answer: answering(job, quote) })
        if (handed.taken !== 'yes') throw new Error(`kelve didn’t take the answer to ${job.about}: ${handed.why}`)
        given.push(job)
        next = handed.next
      }
      return given
    },
  }
}

/** Asks the stand-in graph, started on its own, to do what a test needs. */
export async function graph(url: string, ask: { hold?: boolean; release?: boolean; page?: [string, string]; unagree?: boolean; enough?: string }) {
  const res = await fetch(`${url}/stand-in`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(ask) })
  if (!res.ok) throw new Error(`the stand-in graph said no: ${await res.text()}`)
}
