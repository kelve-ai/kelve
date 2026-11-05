// kelve, for developing it: one command for all of it. A kelve of its own,
// with a fresh store, beside a stand-in for kelve's hosted service, and the
// web app with hot reload, on ports of their own, so your own kelve is never
// touched. Then be kelve's agent yourself, or let the example's agent be it.
// See docs/developing.md.
//
//   bun dev/kelve.ts up              build what's stale, then start it all; Ctrl C stops it
//        --keep                      keep the store from last time
//        --watch                     rebuild and restart kelve when its Rust changes
//   bun dev/kelve.ts agent           the example's agent hands its change over, and does every job
//        --live                      print each call as it makes it
//   bun dev/kelve.ts moment <json>   one of Claude Code's hook events, as kelve's hook gets it
//   bun dev/kelve.ts link            a link that signs your browser in
//   bun dev/kelve.ts tools           kelve's MCP tools, as an agent sees them
//   bun dev/kelve.ts call <tool> [json | @file]
//                                    one MCP call, as an agent makes it
//   bun dev/kelve.ts mcp             the line that gives your own agent the sandbox's tools
//   bun dev/kelve.ts hosted skills | finish <run> | taken <address> | runs <n>
//                                    kelve.ai standing in: give your agent skills to send (one the
//                                    stand-in's runs come out better with), finish a run at once,
//                                    someone else has an address, or how many runs a day
//   bun dev/kelve.ts example         record the app's own example, for the states gallery, from a fresh sandbox
//   bun dev/kelve.ts newcomer        be someone new: ask a Claude Code of their own to install kelve
//                                    from the installer served here, and see what it set up
//        --agent                     Claude Code installs it, unattended, and what it set up is checked
//
// KELVE_DEV_PORT moves kelve (4698); the hosted service is on the port below it, and
// the web app on the port above.

import { type ChildProcess, spawn, spawnSync } from 'node:child_process'
import { cpSync, createWriteStream, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { createServer } from 'node:http'
import { join, resolve } from 'node:path'
import { answer, type Job } from './example/agent'
import { exampleChange, git, makeLedger } from './example/make'

const repo = resolve(import.meta.dirname, '..')
const home = join(repo, '.sandbox')
const port = Number(process.env.KELVE_DEV_PORT ?? 4698)
const graphUrl = `http://127.0.0.1:${port - 1}`
const webPort = port + 1
const kelveBin = join(repo, 'target/debug/kelve')
const graphBin = join(repo, 'target/debug/kelve-graph-stand-in')
const ledger = join(home, 'ledger')

/** The sandbox kelve's environment: everything a `kelve` command needs to find it, and only it. */
const env = {
  KELVE_URL: `http://localhost:${webPort}`,
  KELVE_BIND: `127.0.0.1:${port}`,
  KELVE_DATA: join(home, 'data'),
  KELVE_SECRET_FILE: join(home, 'secret'),
  KELVE_GRAPH_URL: graphUrl,
  KELVE_UTC_OFFSET_MINUTES: '0',
  // The sandbox's own Claude Code settings, so the notes hook never touches yours.
  KELVE_CLAUDE_DIR: join(home, 'claude'),
  // And a home of its own for other agents' hooks.
  KELVE_AGENTS_HOME: join(home, 'home'),
  // The sandbox's own skills, never yours.
  KELVE_SKILLS: join(home, 'skills'),
}
/** Where kelve itself answers, behind the web app. */
const kelveUrl = `http://127.0.0.1:${port}`

const say = (...lines: string[]) => console.log(lines.join('\n'))
const secret = () => readFileSync(env.KELVE_SECRET_FILE, 'utf8').trim()
const flag = (name: string) => process.argv.includes(name)

function need() {
  if (!existsSync(env.KELVE_SECRET_FILE)) {
    say('The sandbox isn’t running. Start it with: bun dev/kelve.ts up')
    process.exit(1)
  }
}

/** Every sign-in link the sandbox kelve has shown, newest last. */
function links(): string[] {
  try {
    return [...readFileSync(join(home, 'kelve.log'), 'utf8').matchAll(/https?:\/\/\S+\/signin\?token=[\w-]+(?:&opened=\d+)?/g)].map((m) => m[0])
  } catch {
    return []
  }
}

async function waitFor(url: string, what: string) {
  for (let i = 0; i < 480; i++) {
    try {
      if ((await fetch(url)).status < 500) return
    } catch {
      // Not listening yet.
    }
    await new Promise((done) => setTimeout(done, 250))
  }
  throw new Error(`${what} didn’t start: see .sandbox/`)
}

function cargo(...args: string[]) {
  if (spawnSync('cargo', args, { cwd: repo, stdio: 'inherit' }).status !== 0) process.exit(1)
}

/** Builds kelve and the stand-in hosted service; cargo leaves what hasn't changed. */
function build() {
  say('Building kelve and the stand-in hosted service…')
  cargo('build', '-p', 'kelve')
  cargo('build', '-p', 'kelve-graph-api', '--features', 'stand-in', '--bin', 'kelve-graph-stand-in')
}

/** The example project, as a git repository: the ledger as it was, then its change. */
const makeExample = () => makeLedger(ledger)
const change = exampleChange

/** Starts a process, its output to a log in the sandbox. */
function start(bin: string, args: string[], log: string, extra: Record<string, string> = {}, cwd = repo): ChildProcess {
  const p = spawn(bin, args, { cwd, env: { ...process.env, ...extra }, stdio: ['ignore', 'pipe', 'pipe'] })
  const out = createWriteStream(join(home, log), { flags: 'a' })
  p.stdout?.pipe(out)
  p.stderr?.pipe(out)
  return p
}

/** The stand-in for kelve's hosted service. */
async function startGraph() {
  const graph = start(graphBin, ['--port', String(port - 1)], 'graph.log')
  // Any answer at all: it's listening.
  await waitFor(`${graphUrl}/`, 'The stand-in hosted service')
  return graph
}

/**
 * One of Claude Code's hook events, given to `kelve moment` as kelve's hook
 * gives it, in the example project: what kelve says back to the agent.
 */
function moment(event: Record<string, unknown>, session = 'example-session'): string {
  const input = { ...(event.tool_input as Record<string, unknown> | undefined) }
  if (typeof input.file_path === 'string' && !input.file_path.startsWith('/')) input.file_path = join(ledger, input.file_path)
  const full = { session_id: session, cwd: ledger, ...event, tool_input: input }
  // Straight to kelve: the web app isn't running while the example's recorded.
  const out = spawnSync(kelveBin, ['moment', '--agent', 'claude', '--on', String(event.hook_event_name)], { input: JSON.stringify(full), env: { ...process.env, ...env, KELVE_URL: kelveUrl }, encoding: 'utf8' })
  const text = out.stdout.trim()
  if (!text) return ''
  try {
    const said = JSON.parse(text) as { hookSpecificOutput?: { additionalContext?: string }; reason?: string }
    // As a turn ends, kelve's question is why the stop is held up.
    return said.hookSpecificOutput?.additionalContext ?? said.reason ?? text
  } catch {
    return text
  }
}

async function startKelve() {
  const kelve = start(kelveBin, ['serve'], 'kelve.log', { ...env, NO_COLOR: '1' })
  await waitFor(`${kelveUrl}/healthz`, 'kelve')
  return kelve
}

/** The newest Rust source's time: what `--watch` rebuilds on. */
function rustChanged(): number {
  let newest = 0
  const walk = (dir: string) => {
    for (const f of readdirSync(dir, { withFileTypes: true })) {
      const p = join(dir, f.name)
      if (f.isDirectory()) walk(p)
      else if (f.name.endsWith('.rs') || f.name.endsWith('.sql')) newest = Math.max(newest, statSync(p).mtimeMs)
    }
  }
  walk(join(repo, 'crates'))
  return newest
}

async function up() {
  build()
  if (!flag('--keep')) rmSync(home, { recursive: true, force: true })
  mkdirSync(home, { recursive: true })
  writeFileSync(join(home, 'env'), Object.entries(env).map(([k, v]) => `export ${k}=${v}`).join('\n') + '\n')
  if (!existsSync(ledger)) makeExample()
  const graph = await startGraph()
  let kelve = await startKelve()
  const web = start('bun', ['run', 'dev'], 'web.log', { KELVE_SERVER: kelveUrl, KELVE_WEB_PORT: String(webPort) }, join(repo, 'web'))
  await waitFor(`${env.KELVE_URL}/healthz`, 'The web app')
  await new Promise((done) => setTimeout(done, 300))
  say(
    '',
    `kelve is at ${env.KELVE_URL}, with hot reload, beside the stand-in hosted service at ${graphUrl}.`,
    'Its store is .sandbox/. Logs: .sandbox/kelve.log, .sandbox/graph.log and .sandbox/web.log.',
    `The example project is .sandbox/ledger: a payments service, and the change "${change().commit}".`,
    '',
    `Sign your browser in: ${links().pop() ?? 'bun dev/kelve.ts link'}`,
    '',
    'Then let the example’s agent hand its change over:  bun dev/kelve.ts agent',
    'Or be the agent yourself:                           bun dev/kelve.ts call open \'{"what":"change","path":"' + ledger + '"}\'',
    'Or give your own coding agent the sandbox’s tools:  bun dev/kelve.ts mcp',
    '',
    'Ctrl C stops it all.',
  )
  let stopping = false
  const stop = () => {
    stopping = true
    for (const p of [kelve, graph, web]) p.kill()
    process.exit(0)
  }
  process.on('SIGINT', stop)
  process.on('SIGTERM', stop)
  const died = (what: string) => (code: number | null) => {
    if (stopping) return
    say(`${what} stopped (${code}): see .sandbox/`)
    stop()
  }
  graph.on('exit', died('The stand-in hosted service'))
  web.on('exit', died('The web app'))
  const watchKelve = (p: ChildProcess) => p.on('exit', (code) => (restarting ? undefined : died('kelve')(code)))
  let restarting = false
  watchKelve(kelve)
  if (flag('--watch')) {
    let seen = rustChanged()
    say('Watching the Rust: kelve restarts when it changes.')
    setInterval(async () => {
      const now = rustChanged()
      if (now === seen || restarting) return
      seen = now
      restarting = true
      say('The Rust changed: building kelve…')
      if (spawnSync('cargo', ['build', '-p', 'kelve'], { cwd: repo, stdio: 'inherit' }).status === 0) {
        kelve.kill()
        await new Promise((done) => setTimeout(done, 500))
        kelve = await startKelve()
        watchKelve(kelve)
        say('kelve restarted.')
      }
      restarting = false
    }, 1500)
  }
  await new Promise(() => {})
}

/**
 * Someone new to kelve, from the start, as the site has them begin: they ask
 * their agent, "Install kelve from …/install.sh", and it installs kelve and
 * sets it up (`kelve setup`); then they ask it to show them something, and
 * kelve's first screen says what's set up. This serves the installer and this
 * kelve, packed as a release, beside a stand-in hosted service they've no account on,
 * and gives them a store and a Claude Code of their own, so yours are never
 * touched. That Claude Code stays logged in between runs (.newcomer-claude);
 * kelve's connection and hook in it start again each time. With --agent,
 * Claude Code installs it here, unattended, and what it set up is checked.
 * Ports 4690 to 4692.
 */
async function newcomer() {
  const them = join(home, 'newcomer')
  rmSync(them, { recursive: true, force: true })
  for (const d of ['release', 'bin', 'home']) mkdirSync(join(them, d), { recursive: true })
  // Their Claude Code: logged in once, then new to kelve each time.
  const claude = join(repo, '.newcomer-claude')
  mkdirSync(claude, { recursive: true })
  rmSync(join(claude, 'settings.json'), { force: true })
  const config = join(claude, '.claude.json')
  if (existsSync(config)) {
    const c = JSON.parse(readFileSync(config, 'utf8')) as { mcpServers?: Record<string, unknown> }
    if (c.mcpServers) delete c.mcpServers.kelve
    writeFileSync(config, JSON.stringify(c, null, 2))
  }
  say('Building the web app, kelve and the stand-in hosted service…')
  if (spawnSync('bun', ['run', 'build'], { cwd: join(repo, 'web'), stdio: 'inherit' }).status !== 0) process.exit(1)
  build()
  // A release, as GitHub would serve it: the build for this computer, the
  // checksums, and the installer beside them, as kelve.ai serves it.
  const name = `${process.platform === 'darwin' ? 'macos' : 'linux'}-${process.arch === 'arm64' ? 'arm64' : 'x86_64'}`
  const release = join(them, 'release')
  spawnSync('tar', ['-czf', join(release, `kelve-${name}.tar.gz`), '-C', join(repo, 'target/debug'), 'kelve'])
  const sum = spawnSync('shasum', ['-a', '256', `kelve-${name}.tar.gz`], { cwd: release }).stdout.toString()
  writeFileSync(join(release, 'SHA256SUMS'), sum)
  cpSync(join(repo, 'install.sh'), join(release, 'install.sh'))
  const site = 'http://127.0.0.1:4692'
  const releases = createServer((req, res) => {
    const file = join(release, (req.url ?? '/').split('?')[0].slice(1))
    if (!file.startsWith(release) || !existsSync(file)) return res.writeHead(404).end()
    res.writeHead(200).end(readFileSync(file))
  }).listen(4692, '127.0.0.1')
  const theirGraph = 'http://127.0.0.1:4691'
  const graph = start(graphBin, ['--port', '4691'], 'newcomer-graph.log')
  await waitFor(`${theirGraph}/`, 'The stand-in hosted service')
  const theirs: Record<string, string> = {
    KELVE_BIND: '127.0.0.1:4690',
    KELVE_DATA: join(them, 'data'),
    KELVE_SECRET_FILE: join(them, 'secret'),
    KELVE_GRAPH_URL: theirGraph,
    KELVE_UPDATES: 'off',
    // Where the installer gets kelve from, and puts it: here, not GitHub.
    KELVE_RELEASES: site,
    KELVE_INSTALL_DIR: join(them, 'bin'),
    CLAUDE_CONFIG_DIR: claude,
    KELVE_CLAUDE_DIR: claude,
    KELVE_AGENTS_HOME: join(them, 'home'),
  }
  writeFileSync(join(them, 'env'), [`export PATH="${join(them, 'bin')}:$PATH"`, ...Object.entries(theirs).map(([k, v]) => `export ${k}="${v}"`)].join('\n') + '\n')
  const ask = `Install kelve from ${site}/install.sh`
  const stop = () => {
    graph.kill()
    releases.close()
    process.exit(0)
  }
  process.on('SIGINT', stop)
  process.on('SIGTERM', stop)
  if (flag('--agent')) {
    // Claude Code installs it, unattended, from the example project.
    if (!existsSync(ledger)) makeExample()
    say('', `Asking Claude Code: “${ask}”…`, '')
    const env = { ...process.env, ...theirs, PATH: `${join(them, 'bin')}:${process.env.PATH}` }
    const done = await new Promise<number | null>((ok) =>
      spawn('claude', ['-p', ask, '--allowedTools', 'Bash'], { cwd: ledger, env, stdio: 'inherit' }).on('exit', ok),
    )
    const mcp = existsSync(config) && !!(JSON.parse(readFileSync(config, 'utf8')) as { mcpServers?: Record<string, unknown> }).mcpServers?.kelve
    const hook = existsSync(join(claude, 'settings.json')) && readFileSync(join(claude, 'settings.json'), 'utf8').includes('kelve moment')
    let level = ''
    if (existsSync(theirs.KELVE_SECRET_FILE)) {
      const res = await fetch('http://127.0.0.1:4690/api/agent/marks', { headers: { authorization: `Bearer ${readFileSync(theirs.KELVE_SECRET_FILE, 'utf8').trim()}` } }).catch(() => null)
      const m = res?.ok ? ((await res.json()) as { kept: boolean; spec: { mark?: string } }) : null
      level = m?.kept ? (m.spec.mark ?? '') : ''
    }
    const tick = (ok: boolean, what: string) => `  ${ok ? '✓' : '✗'} ${what}`
    say(
      '',
      done === 0 ? 'Claude Code finished.' : `Claude Code stopped (${done}). If it said it isn't logged in, log it in once: . ${join(them, 'env')}; claude, then /login.`,
      tick(existsSync(join(them, 'bin/kelve')), 'kelve installed'),
      tick(mcp, 'Claude Code connected to kelve'),
      tick(hook, 'kelve’s hook in Claude Code’s settings'),
      tick(!!level, level ? `kelve marks at ${level}, as Claude Code chose` : 'how much kelve marks, chosen'),
      '',
    )
    spawnSync(join(them, 'bin/kelve'), ['stop'], { env, stdio: 'ignore' })
    if (!flag('--keep')) stop()
  }
  say(
    '',
    'Now be them, in another terminal. This gives that shell a kelve and a Claude Code of their own:',
    '',
    `  . ${join(them, 'env')}`,
    '  claude                          (the first time ever: /login, once)',
    '',
    'Then, as the site says, ask it:',
    '',
    `  > ${ask}`,
    '',
    'It installs kelve and sets it up. Start Claude Code again, so it has kelve’s tools, and ask it:',
    '',
    '  > Show me this repo in kelve.',
    '',
    `Their kelve runs on 4690, beside a stand-in hosted service on 4691 where they've no account yet. Logs: .sandbox/newcomer-graph.log, and kelve's in ${join(them, 'data')}.`,
    'Ctrl C stops the installer’s server and the hosted service; stop their kelve with `kelve stop` in that shell.',
  )
  await new Promise(() => {})
}

/** One call to kelve, as `kelve mcp` makes it, with the command line's secret. */
async function agentCall<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${kelveUrl}${path}`, {
    method: 'POST',
    headers: { authorization: `Bearer ${secret()}`, 'content-type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`kelve said no to ${path}: ${await res.text()}`)
  return (await res.json()) as T
}

const caller = { agent: 'claude-code', model: 'example', plan: 'pro' }

/** Does kelve's work until it's been quiet a while. */
async function work(live: boolean) {
  let quiet = 0
  let next: Job | null = null
  let done = 0
  while (quiet < 8) {
    const job: Job | null = next ?? (await agentCall<Job | null>('/api/agent/next', caller))
    next = null
    if (!job) {
      quiet += 1
      await new Promise((d) => setTimeout(d, 400))
      continue
    }
    quiet = 0
    if (live) say(`  next_job → ${job.about}`)
    const handed: { taken: string; why?: string; next: Job | null } = await agentCall('/api/agent/answer', { ...caller, job: job.job, answer: answer(job) })
    if (handed.taken !== 'yes') throw new Error(`kelve didn’t take the answer to ${job.about}: ${handed.why}`)
    if (live) say(`  hand_in → taken`)
    done += 1
    next = handed.next
  }
  return done
}

async function agent() {
  need()
  const live = flag('--live')
  await agentCall('/api/agent/hello', caller)
  const c = change()
  const opened = await agentCall<{ title: string }>('/api/agent/open', { ...caller, what: 'change', path: ledger, commit: git(ledger, 'rev-parse', 'HEAD'), said: c.said, title: c.title, debrief: true })
  say(`Handed over ${opened.title}.`)
  const done = await work(live)
  say(`Did ${done} of kelve’s jobs. Its debrief is on home.`)
}

/** A browser's session, signed in with a link, for asking kelve what a person would. */
async function session(): Promise<string> {
  const before = links().length
  await fetch(`${kelveUrl}/api/signin-link`, { method: 'POST', headers: { authorization: `Bearer ${secret()}` } })
  for (let i = 0; i < 40 && links().length === before; i++) await new Promise((d) => setTimeout(d, 100))
  const link = new URL(links().pop()!)
  const res = await fetch(`${kelveUrl}${link.pathname}${link.search}`, { redirect: 'manual' })
  return (res.headers.get('set-cookie') ?? '').split(';')[0]
}

/**
 * The app's own example: a fresh sandbox, the example's agent hands its
 * change over and does every job, and every answer the reader asks kelve
 * for is recorded, by its address, into the web app, so the states gallery
 * runs with no server. Paths on this computer are written as the person
 * would see them in their own projects.
 */
async function recordExample() {
  build()
  rmSync(home, { recursive: true, force: true })
  mkdirSync(home, { recursive: true })
  makeExample()
  const graph = await startGraph()
  const kelve = await startKelve()
  try {
    const cookie = await session()
    const asMe = async <T>(path: string): Promise<T> => {
      const res = await fetch(`${kelveUrl}${path}`, { headers: { cookie } })
      if (!res.ok) throw new Error(`kelve said no to ${path}: ${await res.text()}`)
      return (await res.json()) as T
    }
    await agentCall('/api/agent/hello', caller)
    const c = change()
    await agentCall('/api/agent/open', { ...caller, what: 'change', path: ledger, commit: git(ledger, 'rev-parse', 'HEAD'), said: c.said, title: c.title })
    await work(false)
    const [open] = await asMe<Array<{ id: number }>>('/api/opened')
    const id = open.id
    const answers: Record<string, unknown> = {}
    const keep = async (path: string) => (answers[path] = await asMe(path))
    await keep('/api/me')
    const settings = (await keep('/api/settings')) as { hints: string[] }
    settings.hints = ['notes', 'met']
    await keep(`/api/opened/${id}`)
    const ch = (await keep(`/api/opened/${id}/change`)) as { files: Array<{ path: string }> }
    await keep(`/api/opened/${id}/folder?list=true`)
    await keep('/api/counts')
    await keep('/api/opened')
    await keep('/api/list')
    answers['/api/problems'] = []
    answers[`/api/opened/${id}/notes`] = []
    for (const f of ch.files) await keep(`/api/opened/${id}/diff?path=${encodeURIComponent(f.path)}`)
    // Every file, so any of them opens: what the change left as it was opens whole.
    const all = (answers[`/api/opened/${id}/folder?list=true`] as { files: Array<{ path: string }> }).files.map((f) => f.path)
    const changed = new Set(ch.files.map((f) => f.path))
    for (const path of all) if (!changed.has(path)) await keep(`/api/opened/${id}/file?path=${encodeURIComponent(path)}`)
    // Which agents this computer has isn't the example's to say.
    ;(answers['/api/me'] as { found: string[] }).found = []
    let text = JSON.stringify({ opened: id, answers }, null, 1)
    // Where things are on this computer, as they'd be in anyone's: as paths,
    // and as kelve shows them, from ~.
    const shown = (p: string) => (process.env.HOME && p.startsWith(process.env.HOME) ? `~${p.slice(process.env.HOME.length)}` : p)
    text = text.split(ledger).join('~/projects/ledger').split(shown(ledger)).join('~/projects/ledger')
    text = text.split(home).join('~/.kelve').split(shown(home)).join('~/.kelve').split(process.env.HOME ?? '\u0000').join('~')
    const out = join(repo, 'web/src/example/example.json')
    mkdirSync(join(repo, 'web/src/example'), { recursive: true })
    writeFileSync(out, `${text}\n`)
    say(`The app’s example is in web/src/example/example.json: ${Object.keys(answers).length} answers.`)
  } finally {
    kelve.kill()
    graph.kill()
  }
}

/** One MCP session with `kelve mcp`, as an agent's: initialize, then one request. */
async function mcp(method: string, params: unknown): Promise<any> {
  need()
  const p = spawn(kelveBin, ['mcp'], { env: { ...process.env, ...env }, stdio: ['pipe', 'pipe', 'inherit'] })
  let buf = ''
  const waiting = new Map<number, (v: any) => void>()
  p.stdout.on('data', (c) => {
    buf += c
    for (let i = buf.indexOf('\n'); i >= 0; i = buf.indexOf('\n')) {
      const line = buf.slice(0, i)
      buf = buf.slice(i + 1)
      if (line.trim()) {
        const m = JSON.parse(line)
        waiting.get(m.id)?.(m)
      }
    }
  })
  let id = 0
  const rpc = (m: string, ps: unknown) =>
    new Promise<any>((done) => {
      const n = ++id
      waiting.set(n, done)
      p.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id: n, method: m, params: ps })}\n`)
    })
  await rpc('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: process.env.SANDBOX_AGENT ?? 'claude-code', version: 'sandbox' } })
  p.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' })}\n`)
  const out = await rpc(method, params)
  p.stdin.end()
  p.kill()
  return out
}

async function call(tool: string, raw?: string) {
  const args = raw ? JSON.parse(raw.startsWith('@') ? readFileSync(raw.slice(1), 'utf8') : raw) : {}
  const out = await mcp('tools/call', { name: tool, arguments: args })
  if (out.error) {
    say(`kelve said no: ${JSON.stringify(out.error)}`)
    process.exit(1)
  }
  for (const c of out.result?.content ?? []) say(c.text ?? JSON.stringify(c))
  if (out.result?.isError) process.exit(1)
}

/** kelve.ai, standing in: what it does next, for trying verified runs. */
async function hostedSays(what?: string, arg?: string) {
  const tell = async (body: Record<string, unknown>) => {
    const res = await fetch(`${graphUrl}/stand-in/v2`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) })
    if (!res.ok) throw new Error(`The stand-in said ${res.status}. Is the sandbox up?`)
  }
  if (what === 'skills') {
    for (const [name, text] of [
      ['rust', '---\nname: rust\ndescription: How you like Rust.\n---\n# Rust\n\n## How you like it\n\n### Errors are one enum\n\nOne error type, with a message for each case.\n'],
      ['standin-expert', '---\nname: standin-expert\ndescription: Makes the stand-in run come out better.\n---\n# Stand-in expert\n'],
    ]) {
      mkdirSync(join(env.KELVE_SKILLS, name), { recursive: true })
      writeFileSync(join(env.KELVE_SKILLS, name, 'SKILL.md'), text)
    }
    say(`Your sandbox agent has skills to send, in ${env.KELVE_SKILLS}.`)
    return
  }
  if (what === 'finish' && arg) await tell({ finish: arg })
  else if (what === 'taken' && arg) await tell({ taken: [arg] })
  else if (what === 'runs' && Number(arg) >= 0) await tell({ runs_a_day: Number(arg) })
  else {
    say('bun dev/kelve.ts hosted skills | finish <run> | taken <address> | runs <n>')
    process.exit(1)
  }
  say('Done.')
}

const [command, ...rest] = process.argv.slice(2).filter((a) => !a.startsWith('--'))
switch (command) {
  case 'up':
    await up()
    break
  case 'agent':
    await agent()
    break
  case 'example':
    await recordExample()
    break
  case 'newcomer':
    await newcomer()
    break
  case 'link': {
    need()
    const before = links().length
    await fetch(`${kelveUrl}/api/signin-link`, { method: 'POST', headers: { authorization: `Bearer ${secret()}` } })
    await new Promise((done) => setTimeout(done, 300))
    say(links().length > before ? links().pop()! : 'kelve opened your browser, signed in.')
    break
  }
  case 'tools': {
    const out = await mcp('tools/list', {})
    for (const t of out.result.tools) say(`${t.name}: ${t.description}`, `  takes ${JSON.stringify(t.inputSchema.properties)}`, '')
    break
  }
  case 'call':
    if (!rest[0]) {
      say('bun dev/kelve.ts call <tool> [json | @file]')
      process.exit(1)
    }
    await call(rest[0], rest[1])
    break
  case 'mcp':
    need()
    say(
      'Give your own agent the sandbox’s tools, for Claude Code:',
      '',
      `  claude mcp add kelve-sandbox ${Object.entries(env).map(([k, v]) => `-e ${k}=${v}`).join(' ')} -- ${kelveBin} mcp`,
      '',
      'Any other agent: an MCP server over stdio that runs',
      `  ${kelveBin} mcp`,
      'with this environment:',
      ...Object.entries(env).map(([k, v]) => `  ${k}=${v}`),
      '',
      'Then ask it to show you something in kelve, and to do kelve’s work.',
    )
    break
  case 'hosted':
    await hostedSays(rest[0], rest[1])
    break
  case 'moment': {
    need()
    if (!rest[0]) {
      say('bun dev/kelve.ts moment \'{"hook_event_name":"PostToolUse","tool_name":"Read","tool_input":{"file_path":"src/retry.rs"}}\'')
      process.exit(1)
    }
    say(moment(JSON.parse(rest[0]), process.env.SANDBOX_SESSION ?? 'sandbox-session') || 'kelve had nothing for that.')
    break
  }
  default:
    say(readFileSync(import.meta.filename, 'utf8').split('\n').filter((l) => l.startsWith('//')).map((l) => l.slice(3)).join('\n'))
    process.exit(command ? 1 : 0)
}
