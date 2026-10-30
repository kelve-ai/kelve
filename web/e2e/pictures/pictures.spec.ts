// The README's pictures (../.github/assets), from the example, as the
// states gallery draws them, light and dark: at twice the pixels, so they
// stay sharp on dense screens and when GitHub shrinks them to its column.
// `bun run pictures` takes them again; review each before committing.

import { readFileSync } from 'node:fs'
import { expect, test } from '@playwright/test'
import { states } from '../../src/example/states'

const example = JSON.parse(readFileSync(new URL('../../src/example/example.json', import.meta.url), 'utf8')) as {
  answers: Record<string, unknown>
}
/** The moment the example was recorded, a minute on, as the gallery has it. */
const now = new Date(new Date((example.answers['/api/me'] as { agent: { lastSeenAt: string } }).agent.lastSeenAt).getTime() + 60_000)

/**
 * Each picture, by the gallery's state it shows; `folded` leaves out the
 * state's clicks, so no code shows; `width` gives a wider window, for a
 * picture with panes beside the work.
 */
const pictures: { file: string; state: string; folded?: boolean; width?: number }[] = [
  { file: 'change', state: 'reader-change-page' },
  { file: 'in-order', state: 'reader-path' },
  { file: 'plainly', state: 'reader-plainly', folded: true },
  { file: 'down', state: 'reader-down', width: 1440 },
  { file: 'lesson', state: 'reader-lesson' },
]

const all = states(example.answers)
for (const p of pictures) {
  const s = all.find((x) => x.name === p.state)
  if (!s) throw new Error(`There's no state ${p.state} in the gallery.`)
  for (const theme of ['light', 'dark'] as const) {
    test(`${p.file}, ${theme}`, async ({ page }) => {
      if (p.width) await page.setViewportSize({ width: p.width, height: Math.round(p.width / 1.6) })
      await page.clock.setFixedTime(now)
      await page.emulateMedia({ colorScheme: theme, reducedMotion: 'reduce' })
      await page.goto(`/${s.path}${s.path.includes('?') ? '&' : '?'}theme=${theme}`)
      await expect(page.locator('main, .reader').first()).toBeVisible()
      await page.evaluate(async () => { await document.fonts.ready })
      if (!p.folded) for (const words of s.clicks ?? []) await page.getByText(words, { exact: true }).first().click()
      await page.waitForTimeout(400)
      await page.screenshot({ path: `../.github/assets/${p.file}-${theme}.png`, animations: 'disabled', caret: 'hide' })
    })
  }
}
