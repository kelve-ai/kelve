// kelve, for developing it: one command for all of it. A kelve of its own,
// with a fresh store, beside kelve.ai run here (kelve-graph, from the checkout
// beside this one, with Postgres, the benchmark's runner and its gateway in
// Docker), or beside the stand-in for it when there's no checkout, and the web
// app with hot reload, on ports of their own, so your own kelve is never
// touched. Then be kelve's agent yourself, or let the example's agent be it.
// See docs/developing.md.
//
//   bun dev/kelve.ts up              build what's stale, then start it all; Ctrl C or `down` stops it
//        --stand-in                  no keys anywhere: the stand-in model, agent and judge (the default)
//        --real                      rust-1 and Claude Code, for real: kelve.ai's standard runs and judging go on
//                                    ANTHROPIC_API_KEY, or on your Claude plan with a token from claude setup-token
//                                    in ~/.config/kelve-bench-token; you give your own key or token in the app
//        --ci                        4 standard runs and 2 with skills a task, whatever the bank says
//        --keep                      keep the stores from last time
//        --images                    build the benchmark's images again, after changing kelve-graph or kelve's hook
//        --watch                     rebuild and restart kelve when its Rust changes
//   bun dev/kelve.ts status          what's running, and where
//   bun dev/kelve.ts down            stops all of it
//   bun dev/kelve.ts story           the example's agent plays the whole story, through the real MCP
//                                    and hook paths, while you play the person: watch home, the
//                                    debrief, skills and Your agent fill up
//   bun dev/kelve.ts agent           the example's agent hands its change over, and does every job
//        --live                      print each call as it makes it
//   bun dev/kelve.ts moment <json>   one of Claude Code's hook events, as kelve's hook gets it
//   bun dev/kelve.ts link            a link that signs your browser in
//   bun dev/kelve.ts tools           kelve's MCP tools, as an agent sees them
//   bun dev/kelve.ts call <tool> [json | @file]
//                                    one MCP call, as an agent makes it
//   bun dev/kelve.ts mcp             the line that gives your own agent the sandbox's tools
//   bun dev/kelve.ts hosted skills | finish <run> | taken <address> | runs <n>
//                                    give your agent skills to send (one the stand-in's runs come
//                                    out better with); and, with the stand-in for kelve.ai, finish a
//                                    run at once, someone else has an address, or how many runs a day
//   bun dev/kelve.ts example         record the app's own example, for the states gallery, from a fresh sandbox
//   bun dev/kelve.ts trial [task] [--skills <folder>]   with `up --real` going: one real Claude Code trial of a rust-1 task
//   bun dev/kelve.ts newcomer        be someone new: ask a Claude Code of their own to install kelve
//                                    from the installer served here, and see what it set up
//        --agent                     Claude Code installs it, unattended, and what it set up is checked
//
// KELVE_DEV_PORT moves kelve (4698); kelve.ai is on the port below it and the
// web app on the port above, with the rest around them (`ports`).
// KELVE_GRAPH_DIR is kelve-graph's checkout (../kelve-graph), and
// KELVE_SITE_DIR kelve-site's (../kelve-site), run beside them when it's there.

import { type ChildProcess, spawn, spawnSync } from 'node:child_process'
import { randomBytes } from 'node:crypto'
import { cpSync, createWriteStream, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { createServer, request } from 'node:http'
import { connect } from 'node:net'
import { homedir } from 'node:os'
import { join, resolve } from 'node:path'
import { answer, type Job } from './example/agent'
import { changeFiles, exampleChange, git, makeLedger, writeChange } from './example/make'

const repo = resolve(import.meta.dirname, '..')
const home = join(repo, '.sandbox')
const port = Number(process.env.KELVE_DEV_PORT ?? 4698)
/** Every port the sandbox takes, around kelve's. */
const ports = {
  kelve: port,
  web: port + 1,
  graph: port - 1,
  postgres: port - 2,
  gatewayAdmin: port - 3,
  gatewayProxy: port - 4,
  model: port - 5,
  site: port + 2,
  siteDev: port + 3,
}
const graphUrl = `http://127.0.0.1:${ports.graph}`
const webPort = ports.web
const kelveBin = join(repo, 'target/debug/kelve')
const standInBin = join(repo, 'target/debug/kelve-graph-stand-in')
const graphDir = resolve(repo, process.env.KELVE_GRAPH_DIR ?? '../kelve-graph')
const graphBin = join(graphDir, 'target/debug/kelve-graph')
const siteDir = resolve(repo, process.env.KELVE_SITE_DIR ?? '../kelve-site')
const ledger = join(home, 'ledger')
/** What `up` started, for `status` and `down`. */
const upFile = join(home, 'up.json')
const compose = join(repo, 'dev/compose.yaml')
/** The runner's way to the gateway: a container of the sandbox's own. */
const forwarder = 'kelve-dev-forwarder'

/** The sandbox kelve's environment: everything a `kelve` command needs to find it, and only it. */
const env = {
  KELVE_URL: `http://localhost:${webPort}`,
  KELVE_BIND: `127.0.0.1:${port}`,
  KELVE_DATA: join(home, 'data'),
  KELVE_SECRET_FILE: join(home, 'secret'),
  KELVE_GRAPH_URL: graphUrl,
  KELVE_UTC_OFFSET_MINUTES: '0',
  // The sandbox's own Claude Code settings, so kelve's hook never touches yours.
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

async function waitFor(url: string, what: string, log = '.sandbox/') {
  for (let i = 0; i < 480; i++) {
    try {
      if ((await fetch(url)).status < 500) return
    } catch {
      // Not listening yet.
    }
    await new Promise((done) => setTimeout(done, 250))
  }
  throw new Error(`${what} didn’t start: see ${log}`)
}

function cargo(cwd: string, ...args: string[]) {
  if (spawnSync('cargo', args, { cwd, stdio: 'inherit' }).status !== 0) process.exit(1)
}

/** Builds kelve and the stand-in hosted service; cargo leaves what hasn't changed. */
function build() {
  say('Building kelve and the stand-in hosted service…')
  cargo(repo, 'build', '-p', 'kelve')
  cargo(repo, 'build', '-p', 'kelve-graph-api', '--features', 'stand-in', '--bin', 'kelve-graph-stand-in')
}

/** The example project, as a git repository: the ledger as it was, then its change. */
const makeExample = () => makeLedger(ledger)
const change = exampleChange

/**
 * The environment every process the sandbox starts gets: this one's,
 * without any model's key, so none reaches anything unless it's given one
 * on purpose, as `--real` gives the gateway and the runner theirs.
 */
function keyless(): Record<string, string | undefined> {
  const out = { ...process.env }
  for (const k of ['ANTHROPIC_API_KEY', 'CLAUDE_CODE_OAUTH_TOKEN', 'OPENAI_API_KEY', 'KELVE_BENCH_ANTHROPIC_KEY', 'KELVE_BENCH_OPENAI_KEY', 'KELVE_GRAPH_JUDGE_KEY']) delete out[k]
  // A PORT from whatever started the sandbox would win over each process's
  // own address, and kelve-graph would then listen on every interface.
  delete out.PORT
  return out
}

/** Starts a process, its output to a log in the sandbox. */
function start(bin: string, args: string[], log: string, extra: Record<string, string> = {}, cwd = repo): ChildProcess {
  const p = spawn(bin, args, { cwd, env: { ...keyless(), ...extra }, stdio: ['ignore', 'pipe', 'pipe'] })
  const out = createWriteStream(join(home, log), { flags: 'a' })
  p.stdout?.pipe(out)
  p.stderr?.pipe(out)
  return p
}

/** The stand-in for kelve's hosted service. */
async function startStandIn() {
  const graph = start(standInBin, ['--port', String(ports.graph)], 'graph.log')
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

// ---------------------------------------------------------------- kelve.ai, run here

type Mode = 'stand-in' | 'real'

/** What `up` started, as `status` and `down` read it. */
type Up = {
  pid: number
  mode: Mode
  /** kelve-graph, or the stand-in for it. */
  graph: 'kelve-graph' | 'stand-in'
  /** kelve-site's checkout, when its dev server runs. */
  site: string | null
  processes: Array<{ what: string; pid: number; port?: number }>
}

const alive = (pid: number) => {
  try {
    process.kill(pid, 0)
    return true
  } catch {
    return false
  }
}

/** What `up` started, if it's still running. */
function running(): Up | null {
  try {
    const up = JSON.parse(readFileSync(upFile, 'utf8')) as Up
    return alive(up.pid) ? up : null
  } catch {
    return null
  }
}

/** Whether something is listening on a port of this computer's, on either loopback. */
async function listening(p: number): Promise<boolean> {
  const on = (host: string) =>
    new Promise<boolean>((done) => {
      const s = connect({ host, port: p })
      s.once('connect', () => (s.destroy(), done(true)))
      s.once('error', () => done(false))
    })
  return (await on('127.0.0.1')) || (await on('::1'))
}

function docker(...args: string[]) {
  return spawnSync('docker', args, { encoding: 'utf8' })
}

function composed(...args: string[]) {
  return spawnSync('docker', ['compose', '-f', compose, ...args], { encoding: 'utf8', env: { ...process.env, KELVE_DEV_PG_PORT: String(ports.postgres) } })
}

const hasImage = (name: string) => docker('image', 'inspect', '-f', '.', name).status === 0

/** One of kelve-graph's commands, to its end, its output to a log: whether it worked. */
function graphCommand(args: string[], extra: Record<string, string>, log: string): boolean {
  const r = spawnSync(graphBin, args, { cwd: graphDir, env: { ...keyless(), ...extra }, encoding: 'utf8' })
  writeFileSync(join(home, log), `${r.stdout ?? ''}${r.stderr ?? ''}`, { flag: 'a' })
  return r.status === 0
}

/** The bank and the agent's profile each mode runs with, and what they cost. */
function plan(mode: Mode, ci: boolean) {
  const bank = mode === 'real' ? 'rust-1' : 'dev-rust-0'
  const profile = mode === 'real' ? 'claude-code-sonnet-5-5.toml' : 'standin.toml'
  const bankDir = join(graphDir, 'bench/banks', bank)
  const toml = (file: string, key: string) => Number(new RegExp(`^${key}\\s*=\\s*([\\d.]+)`, 'm').exec(readFileSync(file, 'utf8'))?.[1] ?? 0)
  const live = readdirSync(join(bankDir, 'tasks')).filter((t) => /^state\s*=\s*"live"/m.test(readFileSync(join(bankDir, 'tasks', t, 'task.toml'), 'utf8'))).length
  const stdRuns = ci ? 4 : toml(join(bankDir, 'bank.toml'), 'std_runs')
  const skillsMax = ci ? 2 : toml(join(bankDir, 'bank.toml'), 'skills_max')
  const perRun = toml(join(graphDir, 'bench/profiles', profile), 'cost_hint_usd')
  return { bank, bankDir, profile, live, stdRuns, skillsMax, perRun }
}

/** Secrets the sandbox's kelve.ai keeps between its processes: made afresh with each fresh sandbox. */
function hostedSecrets(): { seal: string; gateway: string } {
  const file = join(home, 'hosted-secrets.json')
  if (existsSync(file)) return JSON.parse(readFileSync(file, 'utf8'))
  const made = { seal: randomBytes(32).toString('base64'), gateway: randomBytes(32).toString('hex') }
  writeFileSync(file, JSON.stringify(made), { mode: 0o600 })
  return made
}

/**
 * The site's dev server behind a door of the sandbox's own: kelve.ai's
 * public pages, `/@…` and `/s/…`, go to kelve-graph, as Firebase sends them
 * there, and everything else, its hot reload too, to the site.
 */
function siteDoor() {
  const to = (path: string) => (path.startsWith('/@') || path.startsWith('/s/') ? ports.graph : ports.siteDev)
  const server = createServer((req, res) => {
    const out = request({ host: '127.0.0.1', port: to(req.url ?? '/'), path: req.url, method: req.method, headers: req.headers }, (back) => {
      res.writeHead(back.statusCode ?? 502, back.headers)
      back.pipe(res)
    })
    out.on('error', () => res.headersSent ? res.end() : res.writeHead(502).end('The site isn’t answering yet.'))
    req.pipe(out)
  })
  server.on('upgrade', (req, socket, head) => {
    const out = connect({ host: '127.0.0.1', port: to(req.url ?? '/') }, () => {
      out.write(`${req.method} ${req.url} HTTP/1.1\r\n${Object.entries(req.headers).map(([k, v]) => `${k}: ${v}`).join('\r\n')}\r\n\r\n`)
      out.write(head)
      out.pipe(socket)
      socket.pipe(out)
    })
    out.on('error', () => socket.destroy())
    socket.on('error', () => out.destroy())
  })
  server.listen(ports.site, '127.0.0.1')
  return server
}

async function up() {
  const was = running()
  if (was) {
    say(`The sandbox is up already (${was.mode}). Stop it first: bun dev/kelve.ts down`)
    process.exit(1)
  }
  const mode: Mode = flag('--real') ? 'real' : 'stand-in'
  // An API key if there is one, or else a Claude plan's token, kept in a file so
  // it's never on a command line. Only ever read here: never written or logged.
  const tokenFile = join(homedir(), '.config/kelve-bench-token')
  const token = existsSync(tokenFile) ? readFileSync(tokenFile, 'utf8').replace(/\s/g, '') : ''
  const key = process.env.ANTHROPIC_API_KEY?.trim() || process.env.CLAUDE_CODE_OAUTH_TOKEN?.trim() || token
  const onPlan = !!key?.startsWith('sk-ant-oat')
  if (mode === 'real' && !key) {
    say(`--real needs something for kelve.ai’s standard runs and judging to go on: ANTHROPIC_API_KEY where you run this, or a token from claude setup-token in ${tokenFile}. Or leave --real off for the stand-ins, which need no key.`)
    process.exit(1)
  }
  const withGraph = existsSync(join(graphDir, 'Cargo.toml'))
  const withSite = existsSync(join(siteDir, 'package.json'))
  // What an up that didn't stop left behind is the sandbox's own: stopped, its data kept.
  if (withGraph && composed('ps', '-q').stdout.trim()) composed('stop')
  if (withGraph) docker('rm', '-f', forwarder)
  // Only what this run starts needs its port.
  const wanted: Array<[string, number]> = [['kelve', ports.kelve], ['the web app', ports.web], ['kelve.ai', ports.graph]]
  if (withGraph) wanted.push(['Postgres', ports.postgres], ['the gateway', ports.gatewayAdmin], ['the gateway’s proxy', ports.gatewayProxy], ['the stand-in model', ports.model])
  if (withSite) wanted.push(['the site', ports.site], ['the site’s dev server', ports.siteDev])
  for (const [what, p] of wanted) {
    if (await listening(p)) {
      say(`Port ${p}, for ${what}, is taken by something else. Stop it, or move the sandbox with KELVE_DEV_PORT=…`)
      process.exit(1)
    }
  }
  build()
  if (!flag('--keep')) rmSync(home, { recursive: true, force: true })
  mkdirSync(home, { recursive: true })
  writeFileSync(join(home, 'env'), Object.entries(env).map(([k, v]) => `export ${k}=${v}`).join('\n') + '\n')
  if (!existsSync(ledger)) makeExample()

  const started: Array<{ what: string; p: ChildProcess; port?: number; core: boolean }> = []
  let stopping = false
  const stop = (code = 0) => {
    if (stopping) return
    stopping = true
    for (const s of [...started].reverse()) s.p.kill()
    door?.close()
    if (door) stopSite(siteDir)
    if (withGraph) {
      docker('rm', '-f', forwarder)
      composed('stop')
    }
    rmSync(upFile, { force: true })
    process.exit(code)
  }
  process.on('SIGINT', () => stop())
  process.on('SIGTERM', () => stop())
  // Whatever goes wrong while it starts, nothing it started is left running.
  const failed = (e: unknown) => {
    say(`The sandbox couldn’t start: ${e instanceof Error ? e.message : String(e)}`)
    stop(1)
  }
  process.on('uncaughtException', failed)
  process.on('unhandledRejection', failed)
  const keep = (what: string, p: ChildProcess, port?: number, core = true) => {
    started.push({ what, p, port, core })
    p.on('exit', (code) => {
      if (stopping || restarting) return
      if (core) {
        say(`${what} stopped (${code}): see .sandbox/`)
        stop(1)
      } else say(`${what} stopped (${code}), so it’s gone until the next up: see .sandbox/`)
    })
    return p
  }
  let restarting = false
  let door: ReturnType<typeof siteDoor> | undefined

  let ci = flag('--ci') || mode === 'stand-in'
  let p: ReturnType<typeof plan> | undefined
  if (withGraph) {
    p = plan(mode, ci)
    say('', `kelve.ai: kelve-graph, from ${graphDir}.`)
    say('Building kelve-graph…')
    cargo(graphDir, 'build', '--bin', 'kelve-graph')
    // The benchmark's images, once: each trial runs in them.
    const images = ['kelve-bench-rust:1', 'kelve-bench-kelve:dev', 'kelve-bench-standin:dev', 'kelve-graph:bench']
    const agents = mode === 'real' ? ['kelve-bench-claude-code:2.1.318'] : []
    // kelve-graph's own image runs the forwarder: one from before it had one is stale.
    const stale = hasImage('kelve-graph:bench') && docker('run', '--rm', 'kelve-graph:bench', 'forwarder', '--help').status !== 0
    if (flag('--images') || stale || [...images, ...agents].some((i) => !hasImage(i))) {
      say('Building the benchmark’s images (bench/build.sh): this takes a while the first time…')
      if (spawnSync('sh', [join(graphDir, 'bench/build.sh'), ...(agents.length ? ['--agents'] : [])], { cwd: graphDir, stdio: 'inherit' }).status !== 0) {
        say('The benchmark’s images didn’t build: see above. Is Docker running?')
        process.exit(1)
      }
    }
    say(`Starting Postgres (docker compose project kelve-dev, on ${ports.postgres})…`)
    if (!flag('--keep')) composed('down', '-v')
    const pg = composed('up', '-d', '--wait')
    if (pg.status !== 0) {
      say(`Postgres didn’t start: ${pg.stderr.trim()} Is Docker running?`)
      process.exit(1)
    }
    const { seal, gateway } = hostedSecrets()
    const bench: Record<string, string> = {
      KELVE_GRAPH_DATABASE_URL: `postgres://graph:graph@127.0.0.1:${ports.postgres}/graph`,
      KELVE_GRAPH_RUNS_SEAL: seal,
      KELVE_BENCH_STORE: join(home, 'bench'),
      KELVE_BENCH_GATEWAY_SECRET: gateway,
      KELVE_BENCH_GATEWAY_URL: `http://127.0.0.1:${ports.gatewayAdmin}`,
      // The forwarder, in Docker, reaches the gateway here.
      KELVE_BENCH_GATEWAY_PROXY: `http://host.docker.internal:${ports.gatewayProxy}`,
      KELVE_BENCH_FORWARDER: forwarder,
      ...(ci ? { KELVE_BENCH_PLAN: 'ci' } : {}),
    }
    // The bank, and the agent's profile, as kelve.ai offers them.
    const profiles = join(home, 'profiles')
    mkdirSync(profiles, { recursive: true })
    cpSync(join(graphDir, 'bench/profiles', p.profile), join(profiles, p.profile))
    if (!graphCommand(['profiles', profiles], bench, 'graph-setup.log') || !graphCommand(['bank', 'load', p.bankDir], bench, 'graph-setup.log')) {
      say('kelve-graph couldn’t keep the bank or the profile: see .sandbox/graph-setup.log')
      process.exit(1)
    }
    if (mode === 'real') {
      // rust-1 is still being built: live here, in the sandbox's own database alone.
      composed('exec', '-T', 'postgres', 'psql', '-q', '-U', 'graph', '-c', "update bench_banks set state = 'live', live_from = now() where version = 'rust-1'")
    }
    let upstream = 'https://api.anthropic.com'
    if (mode === 'stand-in') {
      keep('The stand-in model', start(graphBin, ['stand-in-model', '--bind', `127.0.0.1:${ports.model}`], 'model.log', {}, graphDir), ports.model)
      upstream = `http://127.0.0.1:${ports.model}`
    }
    // The gateway holds the model's key, and people's for their runs, in memory alone.
    keep(
      'The gateway',
      start(graphBin, ['gateway', '--proxy', `127.0.0.1:${ports.gatewayProxy}`, '--admin', `127.0.0.1:${ports.gatewayAdmin}`], 'gateway.log', {
        KELVE_BENCH_GATEWAY_SECRET: gateway,
        KELVE_BENCH_ANTHROPIC_URL: upstream,
        KELVE_BENCH_ANTHROPIC_KEY: mode === 'real' ? key! : 'stand-in',
      }, graphDir),
      ports.gatewayAdmin,
    )
    keep(
      'kelve.ai',
      start(graphBin, ['serve'], 'graph.log', {
        ...bench,
        KELVE_GRAPH_BIND: `127.0.0.1:${ports.graph}`,
        KELVE_GRAPH_SITE_URL: graphUrl,
        KELVE_GRAPH_RUNS: 'bench',
        // It's yours alone, here: a Claude subscription may pay for a run.
        KELVE_GRAPH_RUNS_SUBSCRIPTION: 'on',
        KELVE_GRAPH_RUNS_A_DAY: '20',
        // Sign in as a local person: there's no GitHub or Google here.
        KELVE_GRAPH_DEV_SIGNIN: 'on',
      }, graphDir),
      ports.graph,
    )
    await waitFor(`${graphUrl}/healthz`, 'kelve.ai', '.sandbox/graph.log')
    const runner = keep(
      'The runner',
      start(graphBin, ['runner'], 'runner.log', {
        ...bench,
        KELVE_BENCH_WORKERS: '4',
        ...(mode === 'real'
          ? onPlan
            ? // A plan's token works only from Claude Code, so Claude Code judges.
              { KELVE_BENCH_JUDGE: 'claude-code', KELVE_BENCH_JUDGE_TOKEN: key! }
            : { KELVE_GRAPH_JUDGE_PROVIDER: 'anthropic', KELVE_GRAPH_JUDGE_KEY: key!, KELVE_GRAPH_CEILING: '25' }
          : { KELVE_BENCH_JUDGE: 'stand-in' }),
      }, graphDir),
      undefined,
      false,
    )
    // It's ready once it's taking trials, its forwarder up.
    const taking = () => existsSync(join(home, 'runner.log')) && readFileSync(join(home, 'runner.log'), 'utf8').includes('taking trials')
    for (let i = 0; i < 240 && !taking() && runner.exitCode === null; i++) await new Promise((d) => setTimeout(d, 250))
    if (!taking()) say('The runner isn’t taking trials yet: see .sandbox/runner.log. Verified runs wait until it does.')
  } else {
    say('', `There's no kelve-graph at ${graphDir}, so kelve.ai is the stand-in for it: sign-in, runs and pages are faked in memory.`, 'Check kelve-graph out beside kelve, or set KELVE_GRAPH_DIR, to run the real one.')
    ci = false
    keep('The stand-in hosted service', await startStandIn(), ports.graph)
  }

  let kelve = keep('kelve', await startKelve(), ports.kelve)
  keep('The web app', start('bun', ['run', 'dev'], 'web.log', { KELVE_SERVER: kelveUrl, KELVE_WEB_PORT: String(webPort) }, join(repo, 'web')), ports.web)
  await waitFor(`${env.KELVE_URL}/healthz`, 'The web app')
  let site = ''
  if (withSite) {
    if (!existsSync(join(siteDir, 'node_modules/.bin/astro'))) {
      site = `kelve.ai’s site is at ${siteDir}, but its packages aren’t installed: run bun install there, then up again, to run it.`
    } else {
      keep('The site', start(join(siteDir, 'node_modules/.bin/astro'), ['dev', '--port', String(ports.siteDev), '--host', '127.0.0.1'], 'site.log', {}, siteDir), ports.siteDev, false)
      door = siteDoor()
      site = `http://localhost:${ports.site}, from ${siteDir}: its pages /@… and /s/… are kelve.ai’s`
    }
  } else site = `not found at ${siteDir}: set KELVE_SITE_DIR to run it`

  writeFileSync(
    upFile,
    JSON.stringify({
      pid: process.pid,
      mode,
      graph: withGraph ? 'kelve-graph' : 'stand-in',
      site: door ? siteDir : null,
      processes: started.map((s) => ({ what: s.what, pid: s.p.pid ?? 0, port: s.port })),
    } satisfies Up, null, 1),
  )
  await new Promise((done) => setTimeout(done, 300))
  const costs =
    !withGraph
      ? ['Nothing costs anything: kelve.ai is the stand-in, and the agent is the example’s.']
      : mode === 'stand-in'
        ? [
            'Nothing costs anything: no keys anywhere. Verified runs are on dev-rust-0, with the stand-in agent, model and judge,',
            `and take a few minutes: ${p!.live} tasks, ${p!.stdRuns} standard runs and ${p!.skillsMax} with your skills each. Any key does in the app, but sk-refused, which is turned away as a provider would.`,
          ]
        : onPlan
          ? [
              `This uses your Claude plan. kelve.ai’s side goes on the token in ${tokenFile}, which only the gateway and the runner are given, in memory:`,
              `  the first run’s standard runs, once for the bank and profile: ${p!.live} tasks × ${p!.stdRuns} runs${ci ? '' : ' (--ci makes it 4 runs a task)'}, and Claude Code judging them, on ${'claude-sonnet-5-5'}.`,
              'When the plan’s limit is reached, trials wait for it to reset and carry on. The judge is the same family as the agent here: with an API key for another family, it needn’t be.',
              `Your runs’ skills trials go on the key or Claude token you give in the app: up to ${p!.live} × ${p!.skillsMax} runs.`,
            ]
          : [
            `This costs money. kelve.ai’s side pays on ANTHROPIC_API_KEY, which only the gateway and the runner are given, in memory:`,
            `  the first run’s standard runs, once for the bank and profile: ${p!.live} tasks × ${p!.stdRuns} runs × about $${p!.perRun.toFixed(2)} ≈ $${(p!.live * p!.stdRuns * p!.perRun).toFixed(0)}${ci ? '' : ' (--ci makes it 4 runs a task)'},`,
            '  and judging, about $0.02 a judged check, within a ceiling of $25 a day.',
            `Your runs’ skills trials go on the key or Claude token you give in the app: up to ${p!.live} × ${p!.skillsMax} runs, about $${(p!.live * p!.skillsMax * p!.perRun).toFixed(0)} at most on an API key. The app shows the estimate before anything runs.`,
          ]
  say(
    '',
    `kelve is up${withGraph ? `, with kelve.ai run here, in ${mode} mode` : ''}.`,
    '',
    `  kelve           ${env.KELVE_URL}  (the web app, with hot reload; kelve itself on ${ports.kelve})`,
    `  kelve.ai        ${graphUrl}  (${withGraph ? 'kelve-graph: sign-in, pages, scores and verified runs' : 'the stand-in'})`,
    ...(withGraph
      ? [
          `  the runner      verified runs in Docker, on ${p!.bank}, with ${mode === 'real' ? 'Claude Code on Sonnet 5.5' : 'the stand-in agent'}`,
          `  the gateway     127.0.0.1:${ports.gatewayProxy}, its admin on ${ports.gatewayAdmin}${mode === 'stand-in' ? `; the stand-in model on ${ports.model}` : ', to api.anthropic.com'}`,
          `  Postgres        127.0.0.1:${ports.postgres} (docker compose project kelve-dev)`,
        ]
      : []),
    `  the site        ${site}`,
    `  the example     .sandbox/ledger: a payments service, and the change "${change().commit}"`,
    '',
    'Logs are in .sandbox/: kelve.log, graph.log, runner.log, gateway.log, web.log.',
    '',
    ...costs,
    '',
    'Try first:',
    '  bun dev/kelve.ts story     the example’s agent plays the whole story while you watch home, the debrief, skills and Your agent',
    '  bun dev/kelve.ts hosted skills, then Your agent, Get a verified score: sign in as a local person, and run it',
    '  bun dev/kelve.ts status    what’s running;  bun dev/kelve.ts down  stops it all, as Ctrl C does here',
    '',
    `Sign your browser in: ${links().pop() ?? 'bun dev/kelve.ts link'}`,
  )
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
        const i = started.findIndex((s) => s.p === kelve)
        restarting = false
        kelve = await startKelve()
        restarting = true
        started.splice(i, 1)
        keep('kelve', kelve, ports.kelve)
        say('kelve restarted.')
      }
      restarting = false
    }, 1500)
  }
  await new Promise(() => {})
}

async function status() {
  const up = running()
  if (!up) {
    say('The sandbox isn’t running. Start it with: bun dev/kelve.ts up')
    return
  }
  say(`The sandbox is up, in ${up.mode} mode, with ${up.graph === 'kelve-graph' ? `kelve-graph from ${graphDir}` : 'the stand-in for kelve.ai'} (process ${up.pid}).`, '')
  for (const p of up.processes) {
    const ok = alive(p.pid) && (p.port === undefined || (await listening(p.port)))
    say(`  ${ok ? '✓' : '✗'} ${p.what.padEnd(33)}${p.port ? `127.0.0.1:${p.port}` : ''}${ok ? '' : '  not running: see .sandbox/'}`)
  }
  if (up.site) say(`  ${(await listening(ports.site)) ? '✓' : '✗'} ${'The site, with kelve.ai’s pages'.padEnd(33)}http://localhost:${ports.site}`)
  if (up.graph === 'kelve-graph') {
    const pg = composed('ps', '--status', 'running', '-q').stdout.trim()
    say(`  ${pg ? '✓' : '✗'} ${'Postgres'.padEnd(33)}127.0.0.1:${ports.postgres}`)
    const fwd = docker('ps', '-q', '--filter', `name=^${forwarder}$`).stdout.trim()
    say(`  ${fwd ? '✓' : '✗'} ${'The runner’s forwarder'.padEnd(33)}${forwarder}, in Docker`)
    const trials = docker('ps', '-q', '--filter', 'label=kelve.bench.trial').stdout.trim().split('\n').filter(Boolean).length
    say('', trials ? `${trials} trial${trials === 1 ? '' : 's'} running in Docker.` : 'No trials running.')
  }
  say('', `kelve is at ${env.KELVE_URL}; kelve.ai at ${graphUrl}.`)
}

/** Astro's dev server may have gone into the background on its own: stopped there too. */
function stopSite(dir: string) {
  spawnSync(join(dir, 'node_modules/.bin/astro'), ['dev', 'stop'], { cwd: dir, stdio: 'ignore' })
}

async function down() {
  const up = running()
  if (up?.site) stopSite(up.site)
  if (up) {
    process.kill(up.pid, 'SIGTERM')
    for (let i = 0; i < 100 && alive(up.pid); i++) await new Promise((d) => setTimeout(d, 200))
    for (const p of up.processes) if (alive(p.pid)) process.kill(p.pid, 'SIGTERM')
  } else {
    // An up that's gone without stopping what it started.
    try {
      for (const p of (JSON.parse(readFileSync(upFile, 'utf8')) as Up).processes) if (alive(p.pid)) process.kill(p.pid, 'SIGTERM')
    } catch {
      // Nothing was started.
    }
  }
  docker('rm', '-f', forwarder)
  if (composed('ps', '-q').stdout.trim()) composed('down')
  rmSync(upFile, { force: true })
  say(up ? `Stopped the sandbox: kelve, the web app and kelve.ai${up.graph === 'kelve-graph' ? ', with its runner, gateway and Postgres' : ''}.` : 'The sandbox wasn’t running. Anything it left is stopped.')
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
  const graph = start(standInBin, ['--port', '4691'], 'newcomer-graph.log')
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
    const tick = (ok: boolean, what: string) => `  ${ok ? '✓' : '✗'} ${what}`
    say(
      '',
      done === 0 ? 'Claude Code finished.' : `Claude Code stopped (${done}). If it said it isn't logged in, log it in once: . ${join(them, 'env')}; claude, then /login.`,
      tick(existsSync(join(them, 'bin/kelve')), 'kelve installed'),
      tick(mcp, 'Claude Code connected to kelve'),
      tick(hook, 'kelve’s hook in Claude Code’s settings'),
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

/** Does kelve's work until it's been quiet a while: for one thing opened, with `only`. */
async function work(live: boolean, only?: number) {
  let quiet = 0
  let next: Job | null = null
  let done = 0
  while (quiet < 8) {
    const got: Job | { preparing: true } | null = next ?? (await agentCall<Job | { preparing: true } | null>('/api/agent/next', { ...caller, for: only }))
    next = null
    if (!got || 'preparing' in got) {
      quiet += got ? 0.25 : 1
      await new Promise((d) => setTimeout(d, 400))
      continue
    }
    const job = got
    quiet = 0
    if (live) say(`  next_job → ${job.about}`)
    const handed: { taken: string; why?: string; next: Job | null } = await agentCall('/api/agent/answer', { ...caller, for: only, job: job.job, answer: answer(job) })
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

/** The person, in their browser: what kelve's app asks, and does, as them. */
function person(cookie: string) {
  const call = async <T>(path: string, body?: unknown): Promise<T> => {
    const res = await fetch(`${kelveUrl}${path}`, {
      method: body === undefined ? 'GET' : 'POST',
      headers: { cookie, ...(body === undefined ? {} : { 'content-type': 'application/json' }) },
      body: body === undefined ? undefined : JSON.stringify(body),
    })
    if (!res.ok) throw new Error(`kelve said no to ${path}: ${await res.text()}`)
    return (await res.json()) as T
  }
  return { get: <T>(path: string) => call<T>(path), post: <T>(path: string, body: unknown = {}) => call<T>(path, body) }
}

/**
 * The whole story, as it goes for someone using kelve: the example's agent,
 * through `kelve mcp` and kelve's hook as Claude Code runs them, makes the
 * ledger's change while they talk to it, and hands it over; kelve asks it
 * for the debrief, and it writes it. Then the person, through the app's own
 * calls, agrees with one decision and wants it every time, has one looked
 * into, an idea explained, and one changed, and sends it; the agent answers
 * each, redoes the decision, and turns what was sent into parts of their skills.
 * They check a part, which the agent does by redoing past work with and
 * without it, and ask it to draw a hat. Jobs go through the calls `kelve
 * mcp`'s next_job and hand_in make, since the example's agent reads their
 * material as data; everything else is the agent's own tools and hook.
 */
async function story() {
  need()
  const live = flag('--live')
  const session_id = `story-${Date.now()}`
  const hook = (event: Record<string, unknown>) => moment(event, session_id)
  const step = (line: string) => say('', line)
  const c = change()

  // kelve's hook in the sandbox's own Claude Code settings, as setup puts it in yours.
  spawnSync(kelveBin, ['hook', 'install', 'claude'], { env: { ...process.env, ...env, KELVE_URL: kelveUrl }, stdio: 'ignore' })
  step('Your agent starts on the ledger, as it was before the change.')
  makeLedger(ledger, false)
  const mcp = await McpSession.start(ledger)
  const prompt = 'Make calls to the bank safe to retry: back off, stop while the bank is down, and never charge twice.'
  say(`  You: “${prompt}”`)
  const told = hook({ hook_event_name: 'UserPromptSubmit', prompt })
  if (told) say(`  kelve’s hook: ${told.split('\n')[0].slice(0, 160)}`)
  const read = hook({ hook_event_name: 'PostToolUse', tool_name: 'Read', tool_input: { file_path: 'src/bank/mod.rs' } })
  if (read) say(`  kelve’s hook, as it reads Rust: ${read.split('\n')[0].slice(0, 160)}`)
  const files = changeFiles()
  for (const f of files) {
    writeChange(ledger, f)
    hook({ hook_event_name: 'PostToolUse', tool_name: 'Write', tool_input: { file_path: f } })
  }
  say(`  It writes ${files.length} files.`)
  const correction = 'No, open the breaker after five failures in a row, not three, and say so in the error.'
  say(`  You: “${correction}”`)
  hook({ hook_event_name: 'UserPromptSubmit', prompt: correction })
  const held = hook({ hook_event_name: 'Stop' })
  if (held) say(`  kelve’s hook, as the turn ends: ${held.split('\n')[0].slice(0, 160)}`)

  step('It hands the work over in kelve, and writes the debrief kelve asks for.')
  const opened = await mcp.call('open', { what: 'change', path: '.', title: c.title, said: c.said })
  const only = Number(/giving for: (\d+)/.exec(opened)?.[1] ?? 0) || undefined
  say(`  open → ${opened.split('. ')[0]}.`)
  await work(live, only)

  const me = person(await session())
  const homeView = await me.get<unknown>('/api/home')
  const id = Math.max(0, ...[...JSON.stringify(homeView).matchAll(/debrief\/(\d+)/g)].map((m) => Number(m[1])))
  if (!id) throw new Error('No debrief came to home: see .sandbox/kelve.log')
  type Decision = { n: number; title: string; restsOn: string[] }
  let d = await me.get<{ title: string; decisions: Decision[] }>(`/api/debriefs/${id}`)
  say(`  The debrief, “${d.title}”: ${d.decisions.length} decisions.`)

  step('You read it: agree with one and want it every time, look into one, have an idea explained, and change one.')
  const [first, second, third] = d.decisions
  await me.post(`/api/debriefs/${id}/decide`, { n: first.n, choice: 'agree' })
  await me.post(`/api/debriefs/${id}/remember`, { n: first.n, on: true })
  say(`  Agreed, every time: “${first.title}”.`)
  if (second) {
    await me.post(`/api/debriefs/${id}/look`, { n: second.n, question: 'Is it worth it for one bank, or does the retry alone do?' })
    say(`  Look into it: “${second.title}”.`)
  }
  const idea = first.restsOn[0]
  if (idea) {
    await me.post(`/api/debriefs/${id}/explain`, { idea })
    say(`  Explain: ${idea}.`)
  }
  const done = await work(live)
  say(`  Your agent looked into it and explained it: ${jobs(done)}.`)
  if (third) {
    await me.post(`/api/debriefs/${id}/decide`, { n: third.n, choice: 'change', note: 'Claim the key in the same transaction as the charge.' })
    say(`  Change it: “${third.title}”, saying how.`)
  }
  await me.post(`/api/debriefs/${id}/agree-rest`, {})
  d = await me.post(`/api/debriefs/${id}/send`, {})
  say('  Sent to your agent.')

  step('Your agent redoes the decision, and turns what you sent into parts of your skills.')
  // What to redo comes to it as one of kelve's jobs, beside the learning.
  say(`  ${jobs(await work(live))}.`)
  // You keep the work: commit it. kelve doesn't keep past work for checks
  // by itself yet, so this keeps it as one, as the end-to-end tests do.
  const start = git(ledger, 'rev-parse', 'HEAD')
  git(ledger, 'add', '-A')
  git(ledger, 'commit', '-q', '-m', c.commit)
  pastTask(prompt, start, git(ledger, 'rev-parse', 'HEAD'))
  say('  You keep the work: it’s committed, and kept as past work to check skills on.')
  type Skill = { id: string; name: string; sections: Array<{ parts: Array<{ key: string; heading: string; check: string }> }> }
  const skills = await me.get<{ like: Array<{ id: string; name: string }>; doing: Array<{ id: string; name: string }>; project: Array<{ id: string; name: string }> }>('/api/skills')
  const all = [...skills.like, ...skills.doing, ...skills.project]
  say(`  Your skills: ${all.map((s) => s.name).join(', ') || 'none yet'}.`)

  const withParts = await Promise.all(all.map((s) => me.get<Skill>(`/api/skills/${encodeURIComponent(s.id)}`)))
  const part = withParts.flatMap((s) => s.sections.flatMap((x) => x.parts.map((p) => ({ skill: s, p })))).find(() => true)
  if (part) {
    step(`You check a part, “${part.p.heading}”: your agent redoes past work with it and without.`)
    await me.post(`/api/skills/${encodeURIComponent(part.skill.id)}/check`, { keys: [part.p.key] })
    say(`  ${jobs(await work(live))}.`)
  } else say('', 'No part to check yet: see .sandbox/kelve.log.')

  step('You ask your agent to draw a party hat for its look.')
  await me.post('/api/character/draw', { asked: 'a party hat' })
  say(`  ${jobs(await work(live))}.`)
  mcp.stop()

  say(
    '',
    'That’s the story. Look at it in kelve:',
    `  home         ${env.KELVE_URL}/#/`,
    `  the debrief  ${env.KELVE_URL}/#/debrief/${id}`,
    `  skills       ${env.KELVE_URL}/#/skills`,
    `  your agent   ${env.KELVE_URL}/#/agent`,
  )
}

const jobs = (n: number) => `${n} job${n === 1 ? '' : 's'}`

/** Past work kelve can check skills on: a request, and the work as it started and as it was kept, in the ledger. */
function pastTask(request: string, start: string, kept: string) {
  const db = readdirSync(env.KELVE_DATA).find((f) => f.endsWith('.db'))
  if (!db) throw new Error('kelve has no store yet')
  const q = (s: string) => `'${s.replace(/'/g, "''")}'`
  const r = spawnSync('sqlite3', ['-cmd', '.timeout 5000', join(env.KELVE_DATA, db), `insert into past_tasks (request, root, start, kept, at) values (${q(request)}, ${q(ledger)}, ${q(start)}, ${q(kept)}, ${Date.now()})`], { encoding: 'utf8' })
  if (r.status !== 0) throw new Error(`Couldn’t keep the past work: ${r.stderr}`)
}

/**
 * The app's own example: a fresh sandbox, the example's agent hands its
 * change over and does every job, and every answer the reader asks kelve
 * for is recorded, by its address, into the web app, so the states gallery
 * runs with no server. Paths on this computer are written as the person
 * would see them in their own projects. kelve's other screens draw their
 * states from the example's own (web/src/example).
 */
async function recordExample() {
  if (running()) {
    say('The example is recorded from a fresh sandbox of its own. Stop this one first: bun dev/kelve.ts down')
    process.exit(1)
  }
  build()
  rmSync(home, { recursive: true, force: true })
  mkdirSync(home, { recursive: true })
  makeExample()
  const graph = await startStandIn()
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
    settings.hints = ['met']
    await keep(`/api/opened/${id}`)
    const ch = (await keep(`/api/opened/${id}/change`)) as { files: Array<{ path: string }> }
    await keep(`/api/opened/${id}/folder?list=true`)
    await keep('/api/counts')
    await keep('/api/opened')
    await keep('/api/list')
    answers['/api/problems'] = []
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

/** One MCP session with `kelve mcp`, as an agent's, working in `cwd`: kept open for many calls. */
class McpSession {
  private buf = ''
  private id = 0
  private waiting = new Map<number, (v: any) => void>()
  private constructor(private p: ReturnType<typeof spawn>) {
    p.stdout!.on('data', (c) => {
      this.buf += c
      for (let i = this.buf.indexOf('\n'); i >= 0; i = this.buf.indexOf('\n')) {
        const line = this.buf.slice(0, i)
        this.buf = this.buf.slice(i + 1)
        if (line.trim()) {
          const m = JSON.parse(line)
          this.waiting.get(m.id)?.(m)
        }
      }
    })
  }

  static async start(cwd = repo): Promise<McpSession> {
    need()
    const s = new McpSession(spawn(kelveBin, ['mcp'], { cwd, env: { ...process.env, ...env }, stdio: ['pipe', 'pipe', 'inherit'] }))
    await s.rpc('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: process.env.SANDBOX_AGENT ?? 'claude-code', version: 'sandbox' } })
    s.p.stdin!.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' })}\n`)
    return s
  }

  rpc(method: string, params: unknown): Promise<any> {
    return new Promise((done) => {
      const n = ++this.id
      this.waiting.set(n, done)
      this.p.stdin!.write(`${JSON.stringify({ jsonrpc: '2.0', id: n, method, params })}\n`)
    })
  }

  /** One of kelve's tools: what it says, as text. */
  async call(tool: string, args: unknown): Promise<string> {
    const out = await this.rpc('tools/call', { name: tool, arguments: args })
    if (out.error) throw new Error(`kelve said no to ${tool}: ${JSON.stringify(out.error)}`)
    return (out.result?.content ?? []).map((c: { text?: string }) => c.text ?? JSON.stringify(c)).join('\n')
  }

  stop() {
    this.p.stdin!.end()
    this.p.kill()
  }
}

/** One MCP session with `kelve mcp`, as an agent's: initialize, then one request. */
async function mcp(method: string, params: unknown): Promise<any> {
  const s = await McpSession.start()
  const out = await s.rpc(method, params)
  s.stop()
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
      ['standin-expert', '---\nname: standin-expert\ndescription: Makes the stand-in run come out better.\n---\n# Stand-in expert\n\n## Steps\n\n### Do the whole job\n\nHandle every input the change touches, not only the one asked about.\n'],
    ]) {
      mkdirSync(join(env.KELVE_SKILLS, name), { recursive: true })
      writeFileSync(join(env.KELVE_SKILLS, name, 'SKILL.md'), text)
    }
    say(`Your sandbox agent has skills to send, in ${env.KELVE_SKILLS}.`)
    return
  }
  if (running()?.graph === 'kelve-graph' && ['finish', 'taken', 'runs'].includes(what ?? '')) {
    say('That’s for the stand-in. kelve.ai runs here as kelve-graph: its runs move by themselves, in Docker, an address is taken by signing in as someone else and choosing it, and it allows 20 runs a day.')
    process.exit(1)
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
  case 'status':
    await status()
    break
  case 'down':
    await down()
    break
  case 'story':
    await story()
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
  case 'trial': {
    // One real trial, through the running sandbox's gateway and forwarder.
    const { seal, gateway } = hostedSecrets()
    const given = process.argv.slice(3)
    const at = given.indexOf('--skills')
    const skills = at >= 0 ? ['--skills', given[at + 1]] : []
    const task = given[0] && !given[0].startsWith('--') ? given[0] : 'wordfreq-top'
    const done = spawnSync(
      graphBin,
      ['bench', 'trial', join(graphDir, 'bench/banks/rust-1'), task, join(graphDir, 'bench/profiles/claude-code-sonnet-5-5.toml'), ...skills],
      {
        cwd: graphDir,
        stdio: 'inherit',
        env: {
          ...keyless(),
          KELVE_GRAPH_RUNS_SEAL: seal,
          KELVE_BENCH_STORE: join(home, 'bench'),
          KELVE_BENCH_GATEWAY_SECRET: gateway,
          KELVE_BENCH_GATEWAY_URL: `http://127.0.0.1:${ports.gatewayAdmin}`,
          KELVE_BENCH_GATEWAY_PROXY: `http://host.docker.internal:${ports.gatewayProxy}`,
          // A forwarder of its own: the runner clears any trial of its own it isn't holding.
          KELVE_BENCH_FORWARDER: 'kelve-dev-one-forwarder',
        },
      },
    )
    docker('rm', '-f', 'kelve-dev-one-forwarder')
    process.exit(done.status ?? 1)
  }
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
