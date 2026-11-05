// The stand-in agent's answers to kelve's agent loop: the debrief of work it
// handed over, research on a decision, an explanation of an idea, and parts
// for the person's skills from what they corrected, changed or sent, a past
// task redone with and without their skills, and a drawing for its look. It
// writes from what each job gives it, the way a person's agent would, with
// no model: on the ledger example's change, a debrief of that change, whose
// lines are the ones the change added; on anything else, a plainer one. The
// sandbox's agent (dev/example/agent.ts) and the tests' (web/e2e/agent.ts)
// both use it, so they read the same.

export type LoopJob = { material: Record<string, unknown> }

/** The text inside a fence, without its tags. */
function unfenced(text: unknown): string {
  const t = typeof text === 'string' ? text : ''
  const body = t.includes('\n') ? t.slice(t.indexOf('\n') + 1) : t
  return body.includes('\n') ? body.slice(0, body.lastIndexOf('\n')) : body
}

/** Each file of the change, with the runs of lines it added, as the job's change shows them. */
export function addedRuns(change: unknown): Map<string, Array<[number, number]>> {
  const runs = new Map<string, Array<[number, number]>>()
  let file = ''
  for (const line of unfenced(change).split('\n')) {
    const head = /^--- (\S+) \(/.exec(line)
    if (head) {
      file = head[1]
      runs.set(file, [])
      continue
    }
    const added = /^\s*(\d+) \+ /.exec(line)
    const list = runs.get(file)
    if (!added || !list) continue
    const n = Number(added[1])
    const last = list[list.length - 1]
    if (last && last[1] + 1 === n) last[1] = n
    else list.push([n, n])
  }
  return runs
}

/** A file's lines for a decision: its longest run the change added, at most 12 lines. */
function linesOf(runs: Map<string, Array<[number, number]>>, path: string): string | undefined {
  const list = runs.get(path)
  if (!list?.length) return undefined
  const [from, to] = [...list].sort((a, b) => b[1] - b[0] - (a[1] - a[0]))[0]
  return `${path} ${from}-${Math.min(to, from + 11)}`
}

type Decision = { path: string; title: string; from: string; could: string; restsOn: string; why: string }

/** What the ledger example's change decided, file by file. */
const LEDGER: Decision[] = [
  {
    path: 'src/retry.rs',
    title: 'Wait a random time below a ceiling that doubles',
    from: 'its own call',
    could: 'wait the full ceiling each time; wait a fixed second',
    restsOn: 'Exponential backoff, Jitter',
    why: 'Clients that failed together would otherwise come back together, and knock the bank over again. A random wait spreads them out.',
  },
  {
    path: 'src/bank/breaker.rs',
    title: 'Stop calling the bank after five failures in a row',
    from: 'its own call',
    could: 'keep retrying every call; stop after any failure',
    restsOn: 'Circuit breaker',
    why: 'While the bank is down, every retry only adds to the queue. Letting one call through after a pause tells kelve when it is back.',
  },
  {
    path: 'src/idempotency.rs',
    title: "Claim a request's key before anything goes to the bank",
    from: 'its own call',
    could: 'check the key after the call; trust the shop not to send twice',
    restsOn: 'Idempotence, Advisory locks',
    why: 'Two copies of the same charge can arrive together. Whoever claims the key first goes to the bank; the other is told to ask again.',
  },
]

/** The ideas the ledger's decisions rest on, in a line each. */
const IDEAS: Record<string, string> = {
  'Exponential backoff': 'waiting longer after each failure: twice as long each time, up to a ceiling.',
  Jitter: 'a random part in each wait, so clients that failed together try again apart.',
  'Circuit breaker': 'stopping calls to a service that keeps failing, then letting one through to test it.',
  Idempotence: 'doing something twice has the same effect as doing it once.',
  'Advisory locks': 'a lock a program names, and the database keeps.',
}

/** The person's skills the job gave, by name, with each first part's heading. */
function skillsGiven(skills: unknown): Array<{ name: string; heading: string }> {
  const out: Array<{ name: string; heading: string }> = []
  for (const m of String(skills ?? '').matchAll(/<skill name="([^"]+)">\n([\s\S]*?)\n<\/skill>/g)) {
    const heading = /^### (.+)$/m.exec(m[2])?.[1]
    if (heading) out.push({ name: m[1], heading: heading.trim() })
  }
  return out
}

/** The debrief of the work handed over, on the change's own lines. */
export function debrief(job: LoopJob) {
  const work = (job.material.work ?? {}) as { title?: string; files?: number }
  const runs = addedRuns(job.material.change)
  const ledger = LEDGER.filter((d) => linesOf(runs, d.path))
  const decisions: Decision[] = ledger.length
    ? ledger
    : [...runs.keys()]
        .filter((p) => linesOf(runs, p))
        .slice(0, 2)
        .map((path) => ({
          path,
          title: `Keep the change to ${path} in one place`,
          from: 'its own call',
          could: 'spread it across the files that use it',
          restsOn: 'Separation of concerns',
          why: 'Everything that changed for this lives together, so the next change to it is made in one place.',
        }))
  const title = (work.title || 'The work handed over').slice(0, 100)
  const did = ledger.length
    ? 'Calls to the bank that fail because the network dropped, or the bank was busy, now try again, waiting longer and by a random amount each time. While the bank is down, calls stop for a while. Every charge carries a key, so a retry can never charge twice.'
    : `${title}, as asked, across ${work.files ?? runs.size} files.`
  const how = ledger.length
    ? [
        '1. `with_retries()`, in `src/retry.rs`, tries a call again while its error says a retry might pass.',
        '2. `Breaker`, in `src/bank/breaker.rs`, opens after five failures in a row and lets one call through after a pause.',
        "3. `claim()`, in `src/idempotency.rs`, claims each request's key before the bank is called.",
      ].join('\n')
    : `The change lives in ${decisions.map((d) => `\`${d.path}\``).join(' and ') || 'the files it touched'}.`
  const blocks = decisions.map((d) =>
    [`### ${d.title}`, `- from: ${d.from}`, `- lines: ${linesOf(runs, d.path)}`, `- could have: ${d.could}`, `- rests on: ${d.restsOn}`, '', d.why].join('\n'),
  )
  const used = skillsGiven(job.material.skills).map((s) => `- ${s.name}: ${s.heading}: followed`)
  const ideas = [...new Set(decisions.flatMap((d) => d.restsOn.split(', ')))].map(
    (i) => `- **${i}:** ${IDEAS[i] ?? 'what the change is built around.'}`,
  )
  const text = [
    '---',
    'kelve: debrief/1',
    `title: ${title}`,
    '---',
    '',
    '## What it did',
    '',
    did,
    '',
    '## How it works',
    '',
    how,
    '',
    '## Decisions',
    '',
    blocks.join('\n\n'),
    ...(used.length ? ['', '## What it used from your skills', '', used.join('\n')] : []),
    '',
    '## What it rests on',
    '',
    ideas.join('\n'),
    '',
  ].join('\n')
  return { debrief: text }
}

/** Research on a decision, in its fixed shape. */
export function research(job: LoopJob) {
  const question = unfenced(job.material.question).trim()
  const decision = /^### (.+)$/m.exec(unfenced(job.material.decision))?.[1] ?? 'the decision'
  return {
    research: [
      '#### What your agent found',
      '',
      `Keep it as it is: “${decision}” holds up. ${question ? 'On your question, the sources agree a retry only helps when the failure might pass.' : ''}`.trim(),
      '',
      '| Option | For | Against |',
      '|---|---|---|',
      '| **Keep it as it is** | Nothing to change | A little more code to read |',
      '| Retry every failure | Simpler | Retries what will never pass |',
      '',
      '**It suggests** keeping it as it is.',
      '',
      'Sources: [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110), [AWS on backoff and jitter](https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/)',
      '',
    ].join('\n'),
  }
}

/** An explanation of an idea, from the change's own lines. */
export function explain(job: LoopJob) {
  const idea = String(job.material.idea ?? 'the idea')
  const line = unfenced(job.material.lines)
    .split('\n')
    .find((l) => /^\s*\d+\s{2}\S/.test(l))
    ?.replace(/^\s*\d+\s{2}/, '')
  const what = IDEAS[idea] ?? 'what this part of the change is built around.'
  return {
    explanation: [
      `**${idea}** is ${what}`,
      '',
      line ? `This change uses it here:\n\n\`\`\`\n${line}\n\`\`\`` : 'This change uses it in the lines it added.',
      '',
      `Without it, the code would work on a good day and fail on a bad one: ${idea.toLowerCase()} is what keeps a bad day from getting worse.`,
    ].join('\n'),
  }
}

/** A text's first sentence, with its full stop. */
function firstSentence(text: string) {
  const t = text.trim()
  const end = t.search(/[.!?](\s|$)/)
  return end < 0 ? t : t.slice(0, end + 1)
}

/** Parts for the person's skills, from what they showed they know: one, in the first skill given. */
export function learn(job: LoopJob) {
  const skills = (job.material.skills ?? []) as Array<{ name: string; markdown: string }>
  const skill = skills[0]
  const none = { additions: [], same: [], disagrees: [] }
  if (!skill) return none
  // As a good agent writes them: the decision's own reason, or the
  // person's own words, never a sentence that would fit any part.
  const keptText = unfenced(job.material.kept)
  const kept = /^### (.+)$/m.exec(keptText)?.[1]
  const why = kept ? firstSentence(keptText.split('\n').slice(1).find((l) => l.trim() && !l.startsWith('They ')) ?? '') : ''
  const said = /They said: (.+)/.exec(unfenced(job.material.said))?.[1]
  const saidAs = said ? firstSentence(said).replace(/\.$/, '') : ''
  const [heading, body, evidence] = kept
    ? [kept, why || `So it's done this way without being asked.`, `They sent “${kept}” to be done every time.`]
    : said
      ? [saidAs.charAt(0).toUpperCase() + saidAs.slice(1), 'They said so while the agent worked, after it had done it another way.', said.slice(0, 200)]
      : job.material.edit
        ? ['Keep limits as the person set them', 'They changed a limit the agent chose, after it handed over: theirs is the one that holds.', 'Their edit after the handover.']
        : ['', '', '']
  if (!heading) return none
  const has = unfenced(skill.markdown).split('\n').some((l) => l.trim().toLowerCase() === `### ${heading}`.toLowerCase())
  if (has) return { ...none, same: [{ skill: skill.name, part: heading, evidence }] }
  return {
    ...none,
    additions: [{ skill: skill.name, section: kept ? 'howYouLikeIt' : 'watchOutFor', heading: heading.slice(0, 120), body, evidence }],
  }
}

/**
 * Redoes a past task twice, as two subagents would: with the part, or every
 * skill, it does what the skills say; without, it doesn't.
 */
export function compare(job: LoopJob) {
  const part = 'part' in job.material
  const run = (line: string, followed: boolean) => ({
    diff: `diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1,2 @@\n fn main() {}\n+${line}\n`,
    said: `Done, ${followed ? 'as the skills say' : 'as it would anyway'}.`,
    ...(part ? { followed, shows: line } : {}),
  })
  return { with: run('// with your skills', true), without: run('// as a standard agent', false) }
}

/**
 * A drawing, as the agent hands one in for `draw`: a whole character when
 * it's asked for a cat, else a party hat for the robot.
 */
export function draw(job: LoopJob) {
  const asked = unfenced(job.material.asked).toLowerCase()
  if (asked.includes('cat')) {
    const row = (at: number, text: string) => '.'.repeat(at) + text
    const rows = (pairs: Array<[number, number, string]>) => {
      const out: string[] = new Array(27).fill('')
      for (const [y, x, text] of pairs) out[y] = row(x, text)
      return out
    }
    return {
      drawing: {
        format: 'kelve-character/1',
        name: 'Stand-in cat',
        colours: { o: '#2b2f3a', b: 'body', e: 'eyes' },
        layers: {
          base: rows([[8, 6, 'o..........o'], [9, 6, 'oooooooooooo'], [10, 6, 'obbbbbbbbbbo'], [11, 6, 'obbbbbbbbbbo'], [12, 6, 'obbbbbbbbbbo'], [13, 6, 'oooooooooooo']]),
          eyes: rows([[11, 9, 'e....e']]),
          shut: rows([[12, 9, 'e....e']]),
        },
      },
    }
  }
  return {
    drawing: {
      format: 'kelve-part/1',
      name: 'Party hat',
      slot: 'hat',
      at: [9, 0],
      colours: { a: 'light', g: '#f4c84a', w: '#ffffff' },
      pixels: ['..ww..', '..gg..', '.aaaa.', 'agggga'],
    },
  }
}
