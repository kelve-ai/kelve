// The tests' stand-in for the person's own agent: it says hello, takes
// kelve's jobs and hands in answers, through the same calls `kelve mcp`
// makes, with the command line's secret. It answers from what's in each
// job, the way the browser tests need: not well, but always, and at once.

import * as loop from '../../dev/example/loop'

export type Job = { job: number; kind: string; about: string; material: Record<string, unknown>; waiting: number }

/** How the stand-in agent answers a job. */
export function answer(job: Job): unknown {
  switch (job.kind) {
    case 'note': return { did: 'Did what the note asked, as the stand-in agent.' }
    case 'debrief': return loop.debrief(job)
    case 'research': return loop.research(job)
    case 'explain': return loop.explain(job)
    case 'learn': return loop.learn(job)
    case 'compare': return loop.compare(job)
    case 'draw': return loop.draw(job)
    default: throw new Error(`a job the stand-in agent doesn't know: ${job.kind}`)
  }
}

export type Agent = {
  /** Says hello, as `kelve mcp` does when the agent starts it. */
  hello: () => Promise<void>
  /** Asks kelve to open something, as the `open` tool does: what it says back. */
  open: (what: Record<string, unknown>) => Promise<{ id: number; title: string; shown: boolean; debrief?: { debrief: number | null; due: boolean; reason: string; again: boolean } }>
  /**
   * Does kelve's work until there's none left: the jobs it was given. With
   * `until`, it stops at the first job of that kind, having taken it. With
   * `holding`, it goes on from a job it stopped at.
   */
  work: (opts?: { until?: string; holding?: Job }) => Promise<Job[]>
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
export function agent(base: string, key: string, name = 'claude-code', answering: (job: Job) => unknown = answer): Agent {
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
    work: async ({ until, holding } = {}) => {
      const given: Job[] = []
      // Some work is scheduled a moment ahead: kelve is asked again until
      // it's been quiet for a while.
      let quiet = 0
      // An answer taken comes back with the next job, already the agent's.
      let next: Job | null = holding ?? null
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
        const handed: { taken: string; why?: string; next: Job | null } = await call('/api/agent/answer', { ...caller, job: job.job, answer: answering(job) })
        if (handed.taken !== 'yes') throw new Error(`kelve didn’t take the answer to ${job.about}: ${handed.why}`)
        given.push(job)
        next = handed.next
      }
      return given
    },
  }
}
