// Home, debriefs and their change, as the example shows them with no server
// (boards Home, Debrief and ChangeView): fetch retries failed downloads with
// a growing wait. `#/example/home?as=working|empty|attention` draws home's
// other states; `#/example/debrief/1` the debrief, `/2` a small one and `/3`
// a large one, `?as=writing|queued|failed|sent` the debrief's other states;
// `#/example/change/1` its change. In kelve, `#/debrief/example` is the same
// debrief, to try. What's done in them changes only what the page shows.

import type {
  DebriefChangeView, DebriefDiffView, DebriefFolder, DebriefLine, DebriefLineKind, DebriefView, DecisionChoice,
  DecisionStatus, DecisionView, HomeView, IdeaView, SendLine, TimelineItem,
} from '../api/types'

/** A time today, or `days` before, at `hh:mm`, in this browser's time. */
function at(hhmm: string, days = 0): string {
  const [h, m] = hhmm.split(':').map(Number)
  const d = new Date()
  d.setDate(d.getDate() - days)
  d.setHours(h, m, 0, 0)
  return d.toISOString()
}

// ---------------------------------------------------------------- home

const item = (id: string, kind: TimelineItem['kind'], title: string, when: string, act: TimelineItem['act'], to: string | null, squares: TimelineItem['squares'] = []): TimelineItem =>
  ({ id, kind, title, at: when, act, to, squares })

const DAYS: HomeView['days'] = [
  {
    label: 'Today',
    items: [
      item('added:1', 'skill', 'Added to Rust: bad arguments exit 1, failures exit 2', at('10:42'), 'undo', 'skills/rust'),
      item('handover:7', 'small', 'Fixed a typo in the README', at('09:40'), 'debriefIt', null),
      item('debrief:4', 'done', 'Add a --quiet flag to fetch', at('09:15'), 'open', 'debrief/1'),
    ],
  },
  {
    label: 'Yesterday',
    items: [
      item('score:1', 'score', 'Beat a standard agent on 10 of 12 Rust tasks', at('16:30', 1), null, null, [...Array(10).fill('better'), 'same', 'worse']),
      item('part:1', 'part', 'Made itself a party hat', at('14:02', 1), 'tryItOn', 'agent/look'),
      item('made:2', 'skill', 'Started a new skill: Retrying network calls', at('11:20', 1), 'seeIt', 'skills/retrying-network-calls'),
    ],
  },
]

const DEBRIEF_ASK = { id: 'debrief:1', kind: 'debrief', text: 'I finished Retry failed downloads. I made 4 decisions you might want a say in.', action: 'Read the debrief', to: 'debrief/1', main: true, alt: null } as const
const CHECK_ASK = { id: 'check:rust', kind: 'check', text: 'I can check 3 new Rust lines on your past work.', action: 'Check them', to: 'skills/rust', main: false, alt: null } as const

export function exampleHome(as: string | null): HomeView {
  const base: HomeView = { state: 'normal', name: 'Sam’s agent', status: 'Waiting for you', workingSince: null, say: 'Two things for you.', moving: 'idle', asks: [{ ...DEBRIEF_ASK }, { ...CHECK_ASK }], days: DAYS }
  if (as === 'empty')
    return {
      ...base, state: 'empty', status: 'New here', moving: 'wave', days: [],
      say: 'Hello. I’m Sam’s agent. Give me something to do, as you usually would, and I’ll show you what I did here.',
      asks: [{ id: 'example', kind: 'debrief', text: 'Want to see what that looks like first?', action: 'Try the example', to: 'debrief/1', main: true, alt: null }],
    }
  if (as === 'working')
    return {
      ...base, state: 'working', status: 'Working', moving: 'work', workingSince: new Date(Date.now() - 252_000).toISOString(),
      say: 'I’m working on what you asked. When I hand it over, I’ll look into your question.',
      asks: [{ ...CHECK_ASK, main: true }],
    }
  if (as === 'attention')
    return {
      ...base, state: 'attention', status: 'Needs you', say: 'Something needs fixing.',
      asks: [
        { id: 'hook', kind: 'hook', text: 'kelve’s hook is gone from Claude Code’s settings, so I can’t send you debriefs.', action: 'Put it back', to: null, main: true, alt: 'Not now' },
        { id: 'again:5', kind: 'stalled', text: 'I stopped before finishing the debrief for Tidy up the README.', action: 'Ask me again', to: null, main: false, alt: null },
        { ...DEBRIEF_ASK, main: false },
      ],
    }
  return base
}

// ---------------------------------------------------------------- the debrief

const L = (n: number | null, kind: DebriefLineKind, text: string, folded = 0): DebriefLine => ({ n, kind, text, folded })
const fold = (folded: number) => L(null, 'fold', '', folded)

const CODE: Record<string, DebriefLine[]> = {
  d1: [
    L(59, 'added', '/// Whether a failure might pass if the same request is sent again.'),
    L(60, 'added', 'pub fn should_retry(err: &FetchError) -> bool {'),
    L(61, 'added', '    match err {'),
    L(62, 'added', '        FetchError::Timeout | FetchError::Connection(_) => true,'),
    L(63, 'added', '        FetchError::Status(code) => (500..600).contains(code),'),
    L(64, 'added', '        _ => false,'),
    L(65, 'added', '    }'),
    L(66, 'added', '}'),
  ],
  d2: [
    L(26, 'added', 'fn wait_before(attempt: u32) -> Duration {'),
    L(27, 'added', '    let ceiling = BASE * 2u32.pow(attempt);'),
    L(28, 'added', '    // Spread clients out, so ones that failed together don’t retry together.'),
    L(29, 'added', '    let jitter = random_between(0, ceiling.as_millis() as u64 / 2);'),
    L(30, 'added', '    ceiling + Duration::from_millis(jitter)'),
    L(31, 'added', '}'),
  ],
  d3: [
    L(8, 'added', 'const MOST_TRIES: u32 = 5;'),
    fold(4),
    L(13, 'added', '    for attempt in 0..MOST_TRIES {'),
    L(14, 'added', '        match download(url) {'),
    L(15, 'added', '            Ok(body) => return Ok(body),'),
    L(16, 'added', '            Err(e) if should_retry(&e) => {'),
  ],
  d4: [
    L(13, 'context', '    let Some(url) = args.get(1) else {'),
    L(14, 'context', '        eprintln!("Give the address to download: fetch https://example.com/file.zip");'),
    L(15, 'removed', '        std::process::exit(2);'),
    L(15, 'yours', '        std::process::exit(1);'),
    L(16, 'context', '    };'),
  ],
}

const RESEARCH = '#### What your agent found\n\nRetry **429** too. It means the server is busy, not that the request is wrong, and its `Retry-After` header says when to come back. Wait at least that long, up to a minute.\n\nSources: [RFC 6585](https://www.rfc-editor.org/rfc/rfc6585), [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110)'

const RUST = { name: 'rust', title: 'Rust', new: false }
const RETRYING = { name: 'retrying-network-calls', title: 'Retrying network calls', new: true }

type Seed = Partial<DecisionView> & { title: string; why: string }

function decision(n: number, s: Seed): DecisionView {
  return {
    n, from: 'own', group: null, place: '', couldHave: [], restsOn: [], choice: null, status: 'yourCall', note: '', question: QUESTION,
    research: null, remember: false, skill: RETRYING, canRemember: false, code: [], path: null, lang: 'rust', ...s,
  }
}

/** What the person asks their agent first, when they look into a decision. */
export const QUESTION = 'Is this the best way here, or would another be better?'

const D1 = (): DecisionView => decision(1, {
  title: 'Retry only failures that might pass next time', place: 'src/fetch.rs, lines 59 to 66', path: 'src/fetch.rs', code: CODE.d1, restsOn: ['HTTP status codes'],
  why: 'Timeouts, dropped connections and server errors (5xx) often pass by themselves. A 4xx means the request is wrong, so it would only fail again.\n\n*It could have retried every failure, or 429 too.*',
  choice: 'lookInto', status: 'researchReady', question: 'Should fetch retry 429 too?', canRemember: true,
  research: { state: 'ready', markdown: RESEARCH, suggests: 'Retry 429 too, waiting as long as Retry-After says, up to a minute.' },
})
const D2 = (): DecisionView => decision(2, {
  title: 'Wait twice as long each time, plus a little at random', place: 'src/retry.rs, lines 26 to 31', path: 'src/retry.rs', code: CODE.d2, from: 'said', restsOn: ['Exponential backoff', 'Jitter'],
  why: 'Doubling gives a struggling server room to recover. The random part keeps clients that failed together from coming back together.',
  choice: 'agree', status: 'agreed', remember: true, canRemember: true,
})
const D3 = (): DecisionView => decision(3, {
  title: 'Stop after 5 tries, about 30 seconds in all', place: 'src/retry.rs, lines 8 to 16', path: 'src/retry.rs', code: CODE.d3,
  why: 'Long enough to ride out a short outage, short enough that nobody is left waiting.\n\n*It could have added a --retries flag.*',
})
const D4 = (): DecisionView => decision(4, {
  title: 'Bad arguments exit with 1, and only its own failures with 2', place: 'src/args.rs, line 15', path: 'src/args.rs', code: CODE.d4, from: 'edit', skill: RUST,
  why: 'You changed `exit(2)` to `exit(1)`. `1` says fetch was called wrong. `2` says fetch itself failed.',
  choice: 'agree', status: 'agreed', remember: true, canRemember: true,
})
const X = (n: number, title: string, why: string, group: string | null = null): DecisionView => decision(n, { title, why, group, path: null, lang: null })

const IDEAS: IdeaView[] = [
  { name: 'Exponential backoff', what: 'Waiting longer after each failure, usually twice as long: 1, 2, 4, 8 seconds.', newToYou: true, explanation: null },
  {
    name: 'Jitter', what: 'A little randomness added to a wait, so many clients don’t all act at once.', newToYou: true,
    explanation: { state: 'ready', markdown: 'In this change, `wait_before()` adds up to half of each wait at random:\n\n```rust\nlet jitter = random_between(0, ceiling.as_millis() as u64 / 2);\n```\n\nWithout it, a thousand copies of fetch that failed together would retry together, and hit the server in waves. With it, they spread out.' },
  },
  { name: 'Thundering herd', what: 'Many clients retrying at once, knocking a server that’s just recovered straight back over.', newToYou: true, explanation: null },
  { name: 'HTTP status codes', what: 'The number a server answers with: 5xx is the server’s problem, 4xx the request’s.', newToYou: false, explanation: null },
]

const file = (path: string, added: number, removed: number, decisions: number[] = [], yours = false) => ({ path, added, removed, decisions, yours, shown: true })
const folder = (name: string, count: number, open: boolean, files: ReturnType<typeof file>[]): DebriefFolder => ({ name, count, open, files })

const MEDIUM_FOLDERS: DebriefFolder[] = [
  folder('src/', 6, true, [file('src/retry.rs', 69, 0, [2, 3]), file('src/fetch.rs', 9, 0, [1]), file('src/args.rs', 1, 1, [4], true), file('src/clock.rs', 18, 0), file('src/main.rs', 4, 2), file('src/lib.rs', 2, 0)]),
  folder('tests/', 3, false, [file('tests/retry_waits.rs', 44, 0), file('tests/retry_gives_up.rs', 38, 0), file('tests/status_codes.rs', 52, 0)]),
  folder('docs/', 1, false, [file('docs/retries.md', 36, 0)]),
  folder('the top folder', 4, false, [file('README.md', 12, 3), file('CHANGELOG.md', 4, 0), file('Cargo.toml', 1, 1), file('.github/workflows/test.yml', 3, 1)]),
]

function base(id: string): DebriefView {
  return {
    id, title: 'Retry failed downloads with a growing wait', state: 'ready', example: false, steps: [], files: 14, added: 293, removed: 8,
    did: 'When a download fails because of the network or the server, fetch now tries again, up to 5 times, waiting longer each time. A missing page still stops it at once.',
    how: 'Every download now goes through `with_retries()`, in `src/retry.rs`. It asks `should_retry()` whether a failure might pass:\n\n- timeouts and dropped connections: **yes**\n- server errors, 5xx: **yes**\n- a wrong request, 4xx: **no**\n\nBefore each try it waits about 0.5, 1, 2, 4, then 8 seconds, plus up to half again at random. After 5 tries it gives up and reports the last error.',
    decisions: [D1(), D2(), D3(), D4()], left: 1, forSkills: [], suggested: null, used: [], ideas: IDEAS.map((i) => ({ ...i })), folders: MEDIUM_FOLDERS,
    onSend: [], problems: [], sentAt: null,
  }
}

/** The example's debriefs: `1` medium, `2` small, `3` large; `example` the one to try in kelve. */
export function exampleDebrief(id: string, as: string | null): DebriefView {
  let v = base(id)
  if (id === '2') {
    v = {
      ...v, title: 'Bad arguments exit with 1', files: 1, added: 1, removed: 1,
      did: 'You changed one line after your agent handed over.', how: 'Where the address to download is missing, fetch now exits with `1`, not `2`.',
      decisions: [{ ...D4(), n: 1, choice: null, status: 'yourCall', remember: false, canRemember: false }], left: 1,
      ideas: [{ name: 'Exit status', what: 'The number a program ends with: 0 means it worked, anything else says how it failed.', newToYou: false, explanation: null }],
      folders: [folder('src/', 1, true, [file('src/args.rs', 1, 1, [1], true)])],
    }
  } else if (id === '3') {
    v = {
      ...v, title: 'Resume interrupted downloads, and store them by contents', files: 86, added: 2914, removed: 611,
      did: 'Downloads now pick up where they stopped, and every file is stored once, named by its contents.',
      how: 'A download is now a series of pieces. Each request asks for the next part with a `Range` header, so a dropped connection loses one piece, not all of it.\n\nWhen the last piece arrives, the file is checked and stored under the hash of its contents. Past 5 GB, the oldest files go first. The command line gains `--resume`, `--store` and `--no-progress`.',
      decisions: [
        { ...D1(), group: 'Downloads' }, { ...D2(), group: 'Downloads' }, { ...D3(), group: 'Downloads' },
        X(4, 'Resume from where a download stopped', 'It asks for the rest with a `Range` header, so a file that fails at 90% doesn’t start again.', 'Downloads'),
        X(5, 'Name each stored file by the hash of its contents', 'The same file is stored once, and a corrupted one is caught when its hash doesn’t match.', 'Storing downloads'),
        X(6, 'Write to a temporary file, then move it into place', 'Nobody ever sees half a file, even if fetch is stopped.', 'Storing downloads'),
        X(7, 'Keep at most 5 GB, removing the oldest first', 'Enough for a week of typical use.', 'Storing downloads'),
        { ...D4(), n: 8, group: 'The command line' },
        X(9, 'Show progress only when someone is watching', 'In a file or a script, a progress bar is only noise.', 'The command line'),
      ],
      left: 6,
      folders: [
        folder('src/net/', 14, true, [file('src/net/retry.rs', 69, 0, [2, 3]), file('src/net/range.rs', 142, 0, [4]), file('src/net/fetch.rs', 9, 0, [1])]),
        folder('src/store/', 12, false, [file('src/store/hash.rs', 88, 0, [5])]),
        folder('src/cli/', 9, false, [file('src/cli/args.rs', 1, 1, [8], true)]),
        folder('src/ui/', 18, false, []), folder('tests/', 16, false, []), folder('docs/', 7, false, []),
        folder('migrations/', 3, false, []), folder('scripts/', 2, false, []), folder('the top folder', 5, false, []),
      ],
    }
  }
  if (id === 'example') v.example = true
  if (as === 'writing') v = { ...v, state: 'writing', steps: [{ text: 'Read the 14 files', done: true, now: false }, { text: 'Found 4 decisions', done: true, now: false }, { text: 'Writing it up', done: false, now: true }] }
  if (as === 'queued') v = { ...v, state: 'waiting', steps: [{ text: 'Waiting for your agent', done: false, now: true }, { text: 'Reading the 14 files', done: false, now: false }, { text: 'Writing it up', done: false, now: false }] }
  if (as === 'failed') v = { ...v, state: 'failed', problems: ['Decision 2, “Wait twice as long each time”, has no lines. Add `- lines:` with the path and the line ranges in the change.'] }
  if (as === 'sent') v = { ...v, decisions: v.decisions.map((d) => (d.choice ? d : { ...d, choice: 'agree', status: 'agreed' })), left: 0, state: 'sent', sentAt: at('11:05') }
  return withOnSend(v)
}

// ---------------------------------------------------------------- what's done in it

const word = (ns: number[]) => {
  const w = ns.map(String)
  return `${ns.length === 1 ? 'decision' : 'decisions'} ${w.length > 1 ? `${w.slice(0, -1).join(', ')} and ${w[w.length - 1]}` : w[0]}`
}
const cap = (t: string) => t.charAt(0).toUpperCase() + t.slice(1)

/** What happens when it's sent, or, once it's sent, what's happening: as kelve says it (`server::debriefs`). */
export function onSend(v: DebriefView): SendLine[] {
  const sent = v.state === 'sent'
  const out: SendLine[] = []
  const nums = (f: (d: DecisionView) => boolean) => v.decisions.filter(f).map((d) => d.n)
  const line = (kind: SendLine['kind'], text: string, action: string | null = null, to: string | null = null): SendLine => ({ kind, text, action, to })
  const redo = nums((d) => d.choice === 'change')
  if (redo.length) out.push(line('redo', sent ? `Your agent redoes ${word(redo)} when you next message it.` : `Your agent redoes ${word(redo)}.`))
  const looking = nums((d) => d.status === 'researching')
  if (looking.length) out.push(sent ? line('research', `Your agent is looking into ${word(looking)}.`, 'Open the debrief', `debrief/${v.id}`) : line('research', `Your agent keeps looking into ${word(looking)}.`))
  const skills: Array<{ s: NonNullable<DecisionView['skill']>; ns: number[] }> = []
  for (const d of v.decisions.filter((d) => d.canRemember && d.remember && d.skill)) {
    const had = skills.find((x) => x.s.name === d.skill!.name)
    if (had) had.ns.push(d.n)
    else skills.push({ s: d.skill!, ns: [d.n] })
  }
  skills.sort((a, b) => Number(a.s.new) - Number(b.s.new))
  for (const { s, ns } of skills) {
    const one = ns.length === 1
    const text = sent
      ? s.new ? `Your agent drafts a new skill, ${s.title}, from ${word(ns)}.` : `Your agent adds ${word(ns)} to your ${s.title} skill.`
      : s.new ? `${cap(word(ns))} ${one ? 'starts' : 'start'} a new skill, ${s.title}.` : `${cap(word(ns))} ${one ? 'goes' : 'go'} into your ${s.title} skill.`
    out.push(line(s.new ? 'newSkill' : 'skill', text, sent ? (s.new ? 'See your skills' : 'See the skill') : null, sent ? (s.new ? 'skills' : `skills/${s.name}`) : null))
  }
  const open = nums((d) => !d.choice)
  if (open.length && !sent) out.push(line('open', `${cap(word(open))} ${open.length === 1 ? 'stays' : 'stay'} open.`))
  if (!out.length) out.push(line('open', 'Your agent leaves it as it is.'))
  return out
}

function withOnSend(v: DebriefView): DebriefView {
  return { ...v, left: v.decisions.filter((d) => !d.choice).length, onSend: onSend(v) }
}

function statusOf(choice: DecisionChoice | null, research: DecisionView['research']): DecisionStatus {
  if (choice === 'agree') return 'agreed'
  if (choice === 'change') return 'redoing'
  if (choice === 'lookInto') return research?.state === 'ready' ? 'researchReady' : research ? 'researching' : 'yourCall'
  return 'yourCall'
}

/** The person's call on a decision, as kelve would keep it. */
export function decideIn(v: DebriefView, n: number, choice: DecisionChoice | null, note?: string): DebriefView {
  return withOnSend({
    ...v,
    decisions: v.decisions.map((d) => {
      if (d.n !== n) return d
      const status = statusOf(choice, d.research)
      const yours = d.from !== 'own'
      const remember = choice === 'agree' && yours && !d.choice ? true : d.remember
      const canRemember = choice === 'change' || (choice === 'lookInto' && d.research?.state === 'ready') || (choice === 'agree' && yours)
      return { ...d, choice, status, note: choice === 'change' ? (note ?? d.note) : '', remember, canRemember }
    }),
  })
}

export function rememberIn(v: DebriefView, n: number, on: boolean): DebriefView {
  return withOnSend({ ...v, decisions: v.decisions.map((d) => (d.n === n ? { ...d, remember: on } : d)) })
}

export function lookIn(v: DebriefView, n: number, question: string): DebriefView {
  return withOnSend({
    ...v,
    decisions: v.decisions.map((d) => (d.n === n ? { ...d, choice: 'lookInto', status: 'researching', question, research: { state: 'asked', markdown: null, suggests: null }, canRemember: false } : d)),
  })
}

export function agreeRestIn(v: DebriefView): DebriefView {
  return withOnSend({ ...v, decisions: v.decisions.map((d) => (d.choice ? d : { ...d, choice: 'agree', status: 'agreed', canRemember: d.from !== 'own' })) })
}

export function explainIn(v: DebriefView, idea: string): DebriefView {
  return { ...v, ideas: v.ideas.map((i) => (i.name === idea && !i.explanation ? { ...i, explanation: { state: 'asked', markdown: null } } : i)) }
}

export function knowIn(v: DebriefView, idea: string): DebriefView {
  return { ...v, ideas: v.ideas.map((i) => (i.name === idea ? { ...i, newToYou: false } : i)) }
}

export function sendIn(v: DebriefView): DebriefView {
  return withOnSend({ ...v, state: 'sent', sentAt: new Date().toISOString() })
}

// ---------------------------------------------------------------- the change

export function exampleChange(id: string): DebriefChangeView {
  const v = exampleDebrief(id, null)
  return { id, title: v.title, files: v.files, added: v.added, removed: v.removed, folders: v.folders.map((f) => ({ ...f, open: true })) }
}

const A = (n: number, text: string) => L(n, 'added', text)
const C = (n: number, text: string) => L(n, 'context', text)

const FILES: Record<string, Omit<DebriefDiffView, 'path'>> = {
  'src/fetch.rs': {
    lang: 'rust', kind: 'changed', shown: true,
    lines: [
      fold(35), C(36, 'pub enum FetchError {'), C(37, '    Timeout,'), C(38, '    Connection(io::Error),'), C(39, '    Status(u16),'), A(40, '    GaveUp,'), C(41, '}'), fold(17),
      ...CODE.d1, fold(412),
      C(479, 'impl fmt::Display for FetchError {'), C(480, '    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {'), C(481, '        match self {'),
      A(482, '            FetchError::GaveUp => write!(f, "fetch tried 5 times and gave up. Check your connection, then try again."),'),
      C(483, '            FetchError::Timeout => write!(f, "The server took too long to answer."),'), fold(157),
    ],
    notes: [{ at: 9, n: 1, title: 'Retry only failures that might pass next time', status: 'researchReady' }],
  },
  'src/retry.rs': {
    lang: 'rust', kind: 'new', shown: true,
    lines: [
      A(1, 'use std::thread::sleep;'), A(2, 'use std::time::Duration;'), A(3, ''), A(4, 'use crate::clock::random_between;'), A(5, 'use crate::fetch::{download, should_retry, Body, FetchError};'), A(6, ''),
      A(7, 'const BASE: Duration = Duration::from_millis(500);'), A(8, 'const MOST_TRIES: u32 = 5;'), A(9, ''), A(10, '/// Downloads `url`, trying again when a failure might pass.'),
      A(11, 'pub fn with_retries(url: &str) -> Result<Body, FetchError> {'), A(12, '    let mut last = None;'), A(13, '    for attempt in 0..MOST_TRIES {'), A(14, '        match download(url) {'),
      A(15, '            Ok(body) => return Ok(body),'), A(16, '            Err(e) if should_retry(&e) => {'), A(17, '                last = Some(e);'), A(18, '                sleep(wait_before(attempt));'),
      A(19, '            }'), A(20, '            Err(e) => return Err(e),'), A(21, '        }'), A(22, '    }'), A(23, '    Err(last.unwrap_or(FetchError::GaveUp))'), A(24, '}'), A(25, ''),
      ...CODE.d2,
    ],
    notes: [
      { at: 8, n: 3, title: 'Stop after 5 tries, about 30 seconds in all', status: 'yourCall' },
      { at: 26, n: 2, title: 'Wait twice as long each time, plus a little at random', status: 'agreed' },
    ],
  },
  'src/args.rs': {
    lang: 'rust', kind: 'your edit', shown: true,
    lines: [fold(12), ...CODE.d4, fold(2)],
    notes: [{ at: 5, n: 4, title: 'Bad arguments exit with 1, and only its own failures with 2', status: 'agreed' }],
  },
  'README.md': {
    lang: 'markdown', kind: 'changed', shown: true,
    lines: [fold(41), C(42, '## Options'), C(43, ''), L(44, 'removed', 'fetch gives up on the first failure.'), A(44, 'When a download fails because of the network or the server, fetch tries'), A(45, 'again, up to 5 times, waiting longer each time.'), C(46, ''), fold(20)],
    notes: [],
  },
}

/** A file of the example's change: those it has, else its lines as added. */
export function exampleDiff(path: string): DebriefDiffView {
  const f = FILES[path]
  if (f) return { path, ...f }
  return { path, lang: null, kind: 'new', shown: true, lines: [A(1, `// ${path}`), fold(40)], notes: [] }
}
