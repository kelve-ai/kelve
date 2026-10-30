// How fast the reader's calls are: a kelve of its own, with a fresh store
// in a temporary folder, beside the stand-in shared graph, opens a change
// and times what the reader asks for as you read it. Each file's diff and
// plainly cold (the first time it's asked for) and warm (again, with nothing
// changed), the change's own calls, and the side views. Nothing is read by
// an agent: the times are kelve's own.
//
//   bun dev/speed.ts                       this repository, against HEAD~25
//        --dir=<folder>                    another repository
//        --change="against <rev>"          against a branch or commit, as `kelve diff` takes it
//        --change="since <rev>"            everything since <rev>, uncommitted work too
//        --change="commit <rev>"           one commit
//        --files=<n>                       how many of its files to time (30)
//
// KELVE_BIN for a kelve built elsewhere (target/release/kelve, say);
// SPEED_PORT moves kelve (4670), with the graph on the port above.

import { type ChildProcess, spawn } from 'node:child_process'
import { createWriteStream, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

const repo = resolve(import.meta.dirname, '..')
const arg = (name: string, or: string) => process.argv.find((a) => a.startsWith(`--${name}=`))?.slice(name.length + 3) ?? or
const dir = resolve(arg('dir', repo))
const words = arg('change', 'against HEAD~25')
const most = Number(arg('files', '30'))
const port = Number(process.env.SPEED_PORT ?? 4670)
const kelveBin = resolve(process.env.KELVE_BIN ?? join(repo, 'target/debug/kelve'))
const graphBin = join(repo, 'target/debug/kelve-graph-stand-in')
const base = `http://127.0.0.1:${port}`
const graph = `http://127.0.0.1:${port + 1}`

for (const bin of [kelveBin, graphBin]) {
  if (!existsSync(bin)) {
    console.log(`There's no ${bin}. Build it first: cargo build -p kelve && cargo build -p kelve-graph-api --features stand-in --bin kelve-graph-stand-in`)
    process.exit(1)
  }
}

const home = mkdtempSync(join(tmpdir(), 'kelve-speed-'))
const log = join(home, 'kelve.log')
const running: ChildProcess[] = []

function start(bin: string, args: string[], logFile: string, env: Record<string, string> = {}) {
  const p = spawn(bin, args, { env: { ...process.env, ...env }, stdio: ['ignore', 'pipe', 'pipe'] })
  const out = createWriteStream(logFile)
  p.stdout?.pipe(out)
  p.stderr?.pipe(out)
  running.push(p)
  return p
}

async function up(url: string, what: string) {
  for (let i = 0; i < 240; i++) {
    try {
      if ((await fetch(url)).status < 500) return
    } catch {
      // Not listening yet.
    }
    await new Promise((done) => setTimeout(done, 250))
  }
  throw new Error(`${what} didn’t start: see ${home}`)
}

function stop() {
  for (const p of running) p.kill("SIGKILL")
  rmSync(home, { recursive: true, force: true })
}
process.on('SIGINT', () => {
  stop()
  process.exit(130)
})

const secret = () => readFileSync(join(home, 'secret'), 'utf8').trim()

async function asAgent<T>(path: string, body: unknown = {}): Promise<T> {
  const res = await fetch(`${base}${path}`, { method: 'POST', headers: { authorization: `Bearer ${secret()}`, 'content-type': 'application/json' }, body: JSON.stringify(body) })
  if (!res.ok) throw new Error(`kelve said no to ${path}: ${await res.text()}`)
  return (await res.json()) as T
}

/** Signs in as the person, as a browser would: the cookie it's given. */
async function signIn(): Promise<string> {
  await asAgent('/api/signin-link')
  for (let i = 0; i < 100; i++) {
    const link = /https?:\/\/\S+\/signin\?token=[\w-]+/.exec(readFileSync(log, 'utf8'))?.[0]
    if (link) {
      const res = await fetch(link.replace(/^https?:\/\/[^/]+/, base), { redirect: 'manual' })
      const cookie = res.headers.get('set-cookie')?.split(';')[0]
      if (cookie) return cookie
    }
    await new Promise((done) => setTimeout(done, 50))
  }
  throw new Error('kelve didn’t show a sign-in link')
}

let cookie = ''

/** One call as the reader makes it: how long it took, in milliseconds. */
async function time(path: string): Promise<number> {
  const from = performance.now()
  const res = await fetch(`${base}${path}`, { headers: { cookie } })
  await res.arrayBuffer()
  const took = performance.now() - from
  if (!res.ok && res.status !== 400) throw new Error(`${path}: ${res.status}`)
  return took
}

const sorted = (xs: number[]) => [...xs].sort((a, b) => a - b)
const at = (xs: number[], q: number) => sorted(xs)[Math.min(xs.length - 1, Math.floor(q * xs.length))] ?? 0
const ms = (n: number) => `${n.toFixed(1).padStart(7)} ms`
const rows: string[] = []
function say(what: string, times: number[]) {
  rows.push(`${what.padEnd(34)} median ${ms(at(times, 0.5))}   p90 ${ms(at(times, 0.9))}   n ${times.length}`)
}

try {
  start(graphBin, ['--port', String(port + 1)], join(home, 'graph.log'))
  await up(`${graph}/v1`, 'The stand-in graph')
  mkdirSync(join(home, 'data'), { recursive: true })
  writeFileSync(join(home, 'pick.sh'), '#!/bin/sh\n', { mode: 0o755 })
  start(kelveBin, ['serve'], log, {
    KELVE_URL: base,
    KELVE_BIND: `127.0.0.1:${port}`,
    KELVE_DATA: join(home, 'data'),
    KELVE_SECRET_FILE: join(home, 'secret'),
    KELVE_GRAPH_URL: graph,
    KELVE_GRAPH_KEY: 'stand-in',
    KELVE_PICKER: join(home, 'pick.sh'),
    KELVE_CLAUDE_DIR: join(home, 'claude'),
    KELVE_AGENTS_HOME: join(home, 'home'),
    KELVE_UPDATES: 'off',
    NO_COLOR: '1',
    RUST_LOG: 'warn',
  })
  await up(`${base}/healthz`, 'kelve')
  const caller = { agent: 'claude-code', model: 'speed', plan: 'pro' }
  await asAgent('/api/agent/hello', caller)
  const [kind, rev] = words.split(/\s+/, 2)
  const what = kind === 'since' ? { what: 'worktree', base: rev } : kind === 'commit' ? { what: 'change', commit: rev } : kind === 'against' ? { what: 'change', base: rev } : { what: 'change' }
  await asAgent('/api/agent/open', { ...what, path: dir, title: 'Speed', said: 'Timing the reader.', ...caller })
  cookie = await signIn()
  // The store is fresh: what was just opened is all there is.
  const [{ id }] = (await (await fetch(`${base}/api/opened`, { headers: { cookie } })).json()) as Array<{ id: number }>

  const changeCold = await time(`/api/opened/${id}/change`)
  const change = (await (await fetch(`${base}/api/opened/${id}/change`, { headers: { cookie } })).json()) as { files: Array<{ path: string; skip?: string | null; status: string }> }
  const files = change.files.filter((f) => !f.skip && f.status !== 'deleted').slice(0, most)
  rows.push(`${dir}, ${words}: ${change.files.length} files, ${files.length} timed`, '')
  say('change, first', [changeCold])
  const again = []
  for (let i = 0; i < 5; i++) again.push(await time(`/api/opened/${id}/change`))
  say('change, again', again)

  const cold = { diff: [] as number[], plainly: [] as number[], both: [] as number[] }
  const warm = { diff: [] as number[], plainly: [] as number[] }
  for (const f of files) {
    const q = `path=${encodeURIComponent(f.path)}`
    // As the reader opens a file now: its diff and its plainly together.
    const from = performance.now()
    const [d, p] = await Promise.all([time(`/api/opened/${id}/diff?${q}`), time(`/api/opened/${id}/plainly?${q}`)])
    cold.both.push(performance.now() - from)
    cold.diff.push(d)
    cold.plainly.push(p)
    warm.diff.push(await time(`/api/opened/${id}/diff?${q}`))
    warm.plainly.push(await time(`/api/opened/${id}/plainly?${q}`))
  }
  rows.push('')
  say('a file: diff, first', cold.diff)
  say('a file: plainly, first', cold.plainly)
  say('a file: both together, first', cold.both)
  say('a file: diff, again', warm.diff)
  say('a file: plainly, again', warm.plainly)

  rows.push('')
  const side: Record<string, string> = {
    lessons: `/api/opened/${id}/lessons`,
    'lessons, a file': `/api/opened/${id}/lessons?path=${encodeURIComponent(files[0]?.path ?? '')}`,
    'the change plainly': `/api/opened/${id}/plainly/change`,
    'every file': `/api/opened/${id}/folder?list=true`,
    "the agent's list": '/api/list',
    'what kelve marks': '/api/marks',
    ready: `/api/opened/${id}/ready`,
    notes: `/api/opened/${id}/notes`,
    settings: '/api/settings',
  }
  for (const [what, path] of Object.entries(side)) {
    const times = []
    for (let i = 0; i < 5; i++) times.push(await time(path))
    say(what, times)
  }
  console.log(rows.join('\n'))
} finally {
  stop()
}
