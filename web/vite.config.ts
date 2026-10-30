import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import react from '@vitejs/plugin-react'
import { defineConfig, type Plugin } from 'vite'

// In development the app is served from here and kelve's server runs on its
// own port; everything that isn't the app goes to the server, which is
// started with KELVE_URL pointing here, so links, cookies and sign-in links
// all come here.
// `bun dev/kelve.ts up` starts both, and says where; its kelve is on 4698.
const server = process.env.KELVE_SERVER ?? 'http://127.0.0.1:4698'
const toServer = ['/api', '/mcp', '/signin', '/healthz']

/** kelve's license, and the licenses of the code it includes, go wherever the app goes. */
function licenses(): Plugin {
  return {
    name: 'kelve-licenses',
    generateBundle() {
      for (const [file, from] of [
        ['license.txt', '../LICENSE'],
        ['notices.txt', '../THIRD_PARTY_NOTICES.md'],
      ])
        this.emitFile({ type: 'asset', fileName: file, source: readFileSync(resolve(import.meta.dirname, from), 'utf8') })
    },
  }
}

export default defineConfig({
  plugins: [react(), licenses()],
  server: {
    port: Number(process.env.KELVE_WEB_PORT ?? 4699),
    strictPort: true,
    proxy: Object.fromEntries(toServer.map(path => [path, { target: server }])),
  },
})
