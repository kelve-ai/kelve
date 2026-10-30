// How kelve looks: every state in the states gallery, drawn from the example,
// light and dark, at a desktop's width and a phone's, compared with the
// screenshots committed beside the tests. Screenshots differ from one system's
// fonts to another's, so they're made and compared in Playwright's own Linux
// image, here and in CI: bun run looks                     compare bun run
// looks --update-snapshots  take them again, after a change meant to show

import { defineConfig, devices } from '@playwright/test'

const port = Number(process.env.LOOKS_PORT ?? 4640)

export default defineConfig({
  testDir: 'e2e/looks',
  snapshotPathTemplate: '{testDir}/screenshots/{arg}{ext}',
  fullyParallel: true,
  timeout: 60_000,
  expect: { toHaveScreenshot: { animations: 'disabled', caret: 'hide', maxDiffPixelRatio: 0.002 } },
  reporter: process.env.CI ? [['list'], ['html', { open: 'never', outputFolder: 'looks-report' }]] : [['list']],
  use: { baseURL: `http://127.0.0.1:${port}` },
  webServer: { command: 'node e2e/looks/serve.mjs', url: `http://127.0.0.1:${port}/`, reuseExistingServer: !process.env.CI },
  projects: [
    { name: 'desktop', use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 900 } } },
    { name: 'phone', use: { ...devices['Pixel 7'] } },
  ],
})
