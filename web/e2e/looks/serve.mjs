// Serves the built web app, and nothing else: the example answers from
// what it holds, so the screenshot tests need no kelve.

import { createServer } from 'node:http'
import { readFile } from 'node:fs/promises'
import { extname, join, normalize } from 'node:path'

const dist = join(import.meta.dirname, '../../dist')
const port = Number(process.env.LOOKS_PORT ?? 4640)
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.woff2': 'font/woff2', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png' }

createServer(async (req, res) => {
  const path = normalize(decodeURIComponent(new URL(req.url ?? '/', 'http://x').pathname)).replace(/^(\.\.[/\\])+/, '')
  const file = join(dist, path === '/' ? 'index.html' : path)
  try {
    const body = await readFile(file.startsWith(dist) ? file : join(dist, 'index.html'))
    res.writeHead(200, { 'content-type': types[extname(file)] ?? 'application/octet-stream' })
    res.end(body)
  } catch {
    res.writeHead(200, { 'content-type': 'text/html' })
    res.end(await readFile(join(dist, 'index.html')))
  }
}).listen(port, '127.0.0.1')
