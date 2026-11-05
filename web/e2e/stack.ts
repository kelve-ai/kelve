// Starts what the end-to-end tests run against, and stops it after: the
// stand-in for kelve's hosted service, and kelve itself (built from this checkout,
// serving this checkout's web app), with a fresh store of its own in a
// temporary folder, so the tests never touch anyone's kelve. The person's
// agent is the tests' own stand-in (agent.ts), calling kelve as `kelve mcp`
// does.
//
// Set E2E_PORT for kelve's port, E2E_GRAPH_PORT for the hosted service's, and
// KELVE_BIN for a kelve built somewhere else.

import { type ChildProcess, spawn } from 'node:child_process'
import { createWriteStream, existsSync, mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { WORLD_FILE } from './world'

const web = resolve(import.meta.dirname, '..')

async function up(url: string, server: ChildProcess, what: string, log: string) {
  let stopped: number | null = null
  server.on('exit', (code) => (stopped = code ?? -1))
  const until = Date.now() + 60_000
  while (Date.now() < until) {
    if (stopped !== null) throw new Error(`${what} stopped (${stopped}): see ${log}`)
    try {
      if ((await fetch(url)).status < 500) return
    } catch {
      // Not listening yet.
    }
    await new Promise((done) => setTimeout(done, 250))
  }
  throw new Error(`${what} didn’t start in time: see ${log}`)
}

/** The stand-in for kelve's hosted service, on its own port: where it is, and how to stop it. */
export async function startGraph(port: number, opts: { log?: string } = {}) {
  const bin = resolve(web, process.env.KELVE_GRAPH_BIN ?? '../target/debug/kelve-graph-stand-in')
  if (!existsSync(bin)) throw new Error(`There's no stand-in hosted service at ${bin}. Build it with cargo build -p kelve-graph-api --features stand-in --bin kelve-graph-stand-in.`)
  const log = createWriteStream(join(web, opts.log ?? 'e2e/.graph.log'))
  const server = spawn(bin, ['--port', String(port)], { stdio: ['ignore', 'pipe', 'pipe'] })
  server.stdout?.on('data', (c) => log.write(c))
  server.stderr?.on('data', (c) => log.write(c))
  const url = `http://127.0.0.1:${port}`
  // Any answer at all: it's listening.
  await up(`${url}/`, server, 'The stand-in hosted service', 'e2e/.graph.log')
  return { url, stop: () => server.kill() }
}

/** A kelve of its own, with a fresh store, on `port`, using the hosted service at `graph`. */
export async function startKelve(port: number, graph: string, opts: { log?: string } = {}) {
  const kelve = resolve(web, process.env.KELVE_BIN ?? '../target/debug/kelve')
  if (!existsSync(kelve)) throw new Error(`There's no kelve at ${kelve}. Build it with cargo build -p kelve, or set KELVE_BIN.`)
  const dir = mkdtempSync(join(tmpdir(), 'kelve-e2e-'))
  const secretFile = join(dir, 'secret')
  // A stand-in for the system's picker: it chooses whatever path a test
  // last wrote for it (world.ts, `picks`).
  const pickFile = join(dir, 'next-pick')
  const claudeDir = join(dir, 'claude')
  const picker = join(dir, 'pick.sh')
  writeFileSync(pickFile, '')
  writeFileSync(picker, `#!/bin/sh\ncat '${pickFile}'\n`, { mode: 0o755 })
  const base = `http://localhost:${port}`
  const logFile = opts.log ?? 'e2e/.server.log'
  const log = createWriteStream(join(web, logFile))
  const server = spawn(kelve, ['serve'], {
    env: {
      ...process.env,
      KELVE_URL: base,
      KELVE_BIND: `127.0.0.1:${port}`,
      KELVE_DATA: join(dir, 'data'),
      KELVE_SECRET_FILE: secretFile,
      KELVE_UTC_OFFSET_MINUTES: '0',
      KELVE_GRAPH_URL: graph,
      KELVE_PICKER: picker,
      // Its own Claude Code settings: the notes hook never touches the person's.
      KELVE_CLAUDE_DIR: claudeDir,
      KELVE_AGENTS_HOME: join(claudeDir, '..', 'home'),
      // Its own skills folder: never the person's.
      KELVE_SKILLS: join(dir, 'skills'),
      NO_COLOR: '1',
      RUST_LOG: process.env.RUST_LOG ?? 'info,tower_http=warn,sqlx=warn',
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  server.stdout?.on('data', (c) => log.write(c))
  server.stderr?.on('data', (c) => log.write(c))
  try {
    await up(`${base}/healthz`, server, 'kelve', logFile)
  } catch (e) {
    server.kill()
    throw e
  }
  return { base, secretFile, kelve, pickFile, logFile, claudeDir, skillsDir: join(dir, 'skills'), dataDir: join(dir, 'data'), stop: () => server.kill() }
}

export default async function stack() {
  if (!existsSync(join(web, 'dist/index.html'))) throw new Error('The web app isn’t built. Run bun run build first.')
  const graph = await startGraph(Number(process.env.E2E_GRAPH_PORT ?? 4613))
  let kelve
  try {
    kelve = await startKelve(Number(process.env.E2E_PORT ?? 4603), graph.url)
    const { stop: _, ...world } = kelve
    writeFileSync(WORLD_FILE, JSON.stringify({ ...world, graph: graph.url }, null, 2))
  } catch (e) {
    kelve?.stop()
    graph.stop()
    throw e
  }
  return async () => {
    kelve.stop()
    graph.stop()
  }
}
