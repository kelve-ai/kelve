// Every state of every screen (src/example/states.ts), as it looked when
// its screenshot was taken: light and dark, and at a phone's width those
// that say so.

import { readFileSync } from 'node:fs'
import { expect, test } from '@playwright/test'
import { states } from '../../src/example/states'

const example = JSON.parse(readFileSync(new URL('../../src/example/example.json', import.meta.url), 'utf8')) as {
  answers: Record<string, unknown>
}
/** The moment the example was recorded, a minute on: what "ago" is measured from. */
const now = new Date(new Date((example.answers['/api/me'] as { agent: { lastSeenAt: string } }).agent.lastSeenAt).getTime() + 60_000)

for (const s of states(example.answers)) {
  for (const theme of ['light', 'dark'] as const) {
    test(`${s.name}, ${theme}`, async ({ page }, info) => {
      test.skip(info.project.name === 'phone' && !s.phone, 'drawn at a desktop’s width only')
      if (s.width && info.project.name !== 'phone') await page.setViewportSize({ width: s.width, height: 900 })
      await page.clock.setFixedTime(now)
      await page.emulateMedia({ colorScheme: theme, reducedMotion: 'reduce' })
      await page.goto(`/${s.path}${s.path.includes('?') ? '&' : '?'}theme=${theme}`)
      await expect(page.locator('main, .reader').first()).toBeVisible()
      // Waited for in the page: the font set itself can't come back.
      await page.evaluate(async () => { await document.fonts.ready })
      // The example's iframe, on the first run, drawn too.
      if (s.name === 'first-run') await expect(page.frameLocator('iframe').locator('.reader')).toBeVisible()
      for (const words of s.clicks ?? []) await page.getByText(words, { exact: true }).first().click()
      if (s.points) await page.locator('.lrow').filter({ hasText: s.points }).first().hover()
      if (s.hovers) await page.locator(s.hovers).first().hover()
      await page.waitForTimeout(400)
      // With an entry open, the file scrolls to its idea and settles one of
      // two ways, a fraction of a pixel apart: about 1% of the page. A fold
      // that didn't open, or a pane that didn't show, is far more than 2%.
      const scrolled = /[?&]p=/.test(s.path)
      await expect(page).toHaveScreenshot(`${s.name}-${theme}${info.project.name === 'phone' ? '-phone' : ''}.png`, scrolled ? { maxDiffPixelRatio: 0.02 } : {})
    })
  }
}
