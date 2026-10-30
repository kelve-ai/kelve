// The end-to-end tests: kelve as people use it, in a browser, against a
// kelve started for them with a seeded world (see e2e/stack.ts). They run
// one at a time, since they share that world.

import { defineConfig, devices } from '@playwright/test'

const port = Number(process.env.E2E_PORT ?? 4603)

export default defineConfig({
  testDir: 'e2e/tests',
  globalSetup: './e2e/stack.ts',
  fullyParallel: false,
  workers: 1,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : [['list']],
  use: {
    baseURL: `http://localhost:${port}`,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    { name: 'desktop', use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 900 } }, testIgnore: /phone\.spec\.ts/ },
    // The desktop app draws with the system's web view: WebKit on macOS.
    { name: 'webkit', use: { ...devices['Desktop Safari'], viewport: { width: 1440, height: 900 } }, testIgnore: /phone\.spec\.ts/ },
    { name: 'phone', use: { ...devices['Pixel 7'] }, testMatch: /phone\.spec\.ts/ },
  ],
})
