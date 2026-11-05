// kelve.ai in the example: Sam, signed in, with a page at kelve.ai/@sam and
// two verified scores on it, and a run of each kind to look at. `as` changes
// it: `signed-out`, `sign-in` (the sheet open), `signing-in` (waiting), `taken` (the
// address someone has), `no-runs` (today's used), `confirm` (Start the run
// pressed), `subscription` (a service run locally, which takes a Claude
// subscription too). Runs: 1 finished and verified, 2 going, 3 finished
// with no clear difference, 4 paused for the plan's limit. What's sent is
// answered by the same address, read (`send`).

import type { EstimateView, HandleView, HostedView, OfferView, RunView, ScoreSquare, ScoreView, TaskSquare } from '../api/types'

const AT = '2026-11-03T14:20:00.000Z'
const MORE = '2026-11-06T00:00:00.000Z'

const squares = (pattern: string): ScoreSquare[] => pattern.split('').map((c) => (c === 'b' ? 'better' : c === 'w' ? 'worse' : 'same'))

function score(id: string, run: string, kindName: string, modelName: string, pattern: string, shown: boolean, verified = true): ScoreView {
  const sq = squares(pattern)
  const better = sq.filter((s) => s === 'better').length
  const gain = Math.round((better * 18) / sq.length)
  return { id, run, link: `https://kelve.ai/s/${id}`, shown, kindName, agentName: 'Claude Code', modelName, bank: `${kindName.toLowerCase()}-1`, squares: sq, better, of: sq.length, gain, gainLow: gain - 9, gainHigh: gain + 9, verified, at: AT }
}

function run(id: string, state: RunView['state'], pattern: string, s: ScoreView | null, paused?: boolean): RunView {
  const sq: TaskSquare[] = pattern.split('').map((c) => ({ b: 'better', s: 'same', w: 'worse', r: 'running', q: 'waiting' })[c] as TaskSquare)
  return { run: id, state, kindName: 'Rust', agentName: 'Claude Code', modelName: 'Sonnet 5.6', squares: sq, minutesLeft: state === 'running' ? 15 : null, pausedUntil: paused ? '2026-11-05T16:00:00.000Z' : null, why: null, score: s }
}

export function hostedAnswers(as: string | null): Record<string, unknown> {
  const out = as === 'signed-out' || as === 'signing-in' || as === 'sign-in'
  const account: HostedView = {
    available: true,
    signedIn: !out,
    waiting: as === 'signing-in',
    handle: out ? null : 'sam',
    page: out ? null : 'https://kelve.ai/@sam',
    public: !out,
    runs: out ? null : { left: as === 'no-runs' ? 0 : 1, aDay: 1, more: MORE },
    going: null,
    keys: out ? [] : [{ provider: 'anthropic', name: 'Anthropic', kind: 'apiKey' }],
    problem: null,
  }
  const local = as === 'subscription'
  const offer: OfferView = {
    kinds: [
      { kind: 'rust', name: 'Rust', about: 'Command-line tools, libraries, and fixing bugs in code that already exists.', tasks: 12 },
      { kind: 'words', name: 'Words', about: 'The words in an app: buttons, errors, settings and empty screens.', tasks: 10 },
    ],
    agents: [
      { agent: 'claude-code', name: 'Claude Code', models: [{ model: 'claude-sonnet-5-6', name: 'Sonnet 5.6', provider: 'anthropic', providerName: 'Anthropic' }, { model: 'claude-opus-5-5', name: 'Opus 5.5', provider: 'anthropic', providerName: 'Anthropic' }] },
      { agent: 'codex', name: 'Codex', models: [{ model: 'gpt-5.5', name: 'GPT-5.5', provider: 'openai', providerName: 'OpenAI' }] },
    ],
    skills: [
      { path: 'everywhere/SKILL.md', bytes: 1640 },
      { path: 'rust/SKILL.md', bytes: 2458 },
      { path: 'rust/references/errors.md', bytes: 1126 },
      { path: 'words/SKILL.md', bytes: 1946 },
    ],
    runs: { left: as === 'no-runs' ? 0 : 1, aDay: 1, more: MORE },
    accepts: local ? ['subscription', 'apiKey'] : ['apiKey'],
  }
  const estimate: EstimateView = {
    run: '2',
    kindName: 'Rust',
    agentName: 'Claude Code',
    modelName: 'Sonnet 5.6',
    tasks: 12,
    runsPerTask: 3,
    minutes: 20,
    provider: 'anthropic',
    providerName: 'Anthropic',
    dollars: 9.6,
    agentRuns: 36,
    runsLeft: 0,
    accepts: offer.accepts,
    kept: local ? [] : ['apiKey'],
    paying: offer.accepts[0],
    skills: offer.skills,
  }
  const taken = as === 'taken'
  const check: HandleView = taken
    ? { handle: 'alex', available: false, why: 'Someone has that one.', instead: ['alex-agent', 'alexbuilds'] }
    : { handle: 'sam', available: true, why: 'You can have that one.', instead: [] }
  const verified = score('s9', '1', 'Rust', 'Sonnet 5.6', 'bbbbbbbbbbsw', false)
  return {
    '/api/hosted': account,
    '/api/hosted/scores': [
      score('s1', '1', 'Rust', 'Sonnet 5.6', 'bbbbbbbbbbsw', true),
      score('s2', '5', 'Words', 'Sonnet 5.5', 'bbbbbbbssw', true),
      score('s3', '6', 'Rust', 'Opus 5.5', 'bbbbbbbsssww', false),
    ],
    '/api/hosted/offer': offer,
    '/api/hosted/runs': estimate,
    '/api/hosted/handle/check': check,
    '/api/hosted/runs/1': run('1', 'finished', 'bbbbbbbbbbsw', verified),
    '/api/hosted/runs/2': run('2', 'running', 'bbsbwbrqqqqq', null),
    '/api/hosted/runs/2/confirm': run('2', 'queued', 'qqqqqqqqqqqq', null),
    '/api/hosted/runs/3': run('3', 'finished', 'bssssswsssss', score('s8', '3', 'Rust', 'Sonnet 5.6', 'bssssswsssss', false, false)),
    '/api/hosted/runs/4': run('4', 'running', 'bbsbqqqqqqqq', null, true),
    '/api/hosted/scores/s9': { ...verified, shown: true },
  }
}
