// The README's pictures (e2e/pictures): `bun run pictures`, in Playwright's
// Linux image, as the screenshot tests are, beside the same built app.

import { defineConfig, devices } from '@playwright/test'

const port = Number(process.env.LOOKS_PORT ?? 4640)

export default defineConfig({
  testDir: 'e2e/pictures',
  timeout: 60_000,
  reporter: [['list']],
  use: { baseURL: `http://127.0.0.1:${port}` },
  webServer: { command: 'node e2e/looks/serve.mjs', url: `http://127.0.0.1:${port}/`, reuseExistingServer: true },
  projects: [{ name: 'readme', use: { ...devices['Desktop Chrome'], viewport: { width: 1280, height: 800 }, deviceScaleFactor: 2 } }],
})
