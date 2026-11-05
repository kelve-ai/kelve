// The example's agent: it does kelve's jobs on the ledger example the way a
// person's agent would, through the same calls `kelve mcp` makes. It needs
// no model, so the sandbox, the states gallery and the end-to-end tests all
// read the same believable content. Its debriefs, research, explanations,
// what it learns, its checks and its drawings are the agent loop's (loop.ts).

import * as loop from './loop'

export type Job = { job: number; kind: string; about: string; material: Record<string, unknown>; waiting: number }

/** How the example's agent answers a job. */
export function answer(job: Job): unknown {
  switch (job.kind) {
    case 'redo': return { did: 'Redid the decision as the person asked.' }
    case 'debrief': return loop.debrief(job)
    case 'research': return loop.research(job)
    case 'explain': return loop.explain(job)
    case 'learn': return loop.learn(job)
    case 'compare': return loop.compare(job)
    case 'draw': return loop.draw(job)
    default: throw new Error(`a job the example's agent doesn't know: ${job.kind}`)
  }
}
