// The first run (boards O-1 to O-5), against a kelve of its own that no agent
// has said hello to: connect your agent, with the example beside it; then the
// words to say to it, and letting it pick up your notes; then, the first time
// your agent writes an entry the graph didn't have, adding it, and agreeing to
// what that means.

import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test } from '@playwright/test'
import { agent, graph } from '../agent'
import { startKelve } from '../stack'
import { marginNote, newLink, openFile, repo, secret, shown, signIn, world } from '../world'

test('connect your agent, with the example beside it; then what to say to it', async ({ page, browserName }) => {
  const port = Number(process.env.E2E_FRESH_PORT ?? 4605) + (browserName === 'webkit' ? 1 : 0)
  const fresh = await startKelve(port, world().graph, { log: `e2e/.fresh-${browserName}.log` })
  try {
    await signIn(page, fresh, { new: true })
    await page.goto(`${fresh.base}/`)
    await expect(page.getByRole('heading', { name: 'Understand what your agent did.' })).toBeVisible()
    await expect(page.getByText('claude mcp add --scope user kelve -- kelve mcp')).toBeVisible()
    await page.getByRole('radio', { name: 'Codex' }).click()
    await expect(page.getByText('codex mcp add kelve -- kelve mcp')).toBeVisible()
    await expect(page.getByText('Waiting for Codex. It connects the next time you start it.')).toBeVisible()
    await page.getByRole('radio', { name: 'Claude Code' }).click()
    // The example, beside it, to click around in: answered with no server.
    const example = page.frameLocator('iframe[title="An example of kelve"]')
    await expect(example.getByRole('heading', { level: 1, name: 'Retries that back off with jitter' })).toBeVisible()

    // Your agent says hello: the page moves on by itself, to what's set up.
    const me = agent(fresh.base, secret(fresh))
    await me.hello()
    await expect(page.getByRole('heading', { name: 'kelve is set up.' })).toBeVisible()
    await expect(page.getByText('Claude Code is connected')).toBeVisible()
    await expect(page.getByText('kelve marks every idea but the basics')).toBeVisible()
    await expect(page.getByText('Show me what you did in kelve.')).toBeVisible()
    // The hook, when the install didn't add it: one word turns it on.
    await page.getByRole('button', { name: 'Turn it on' }).click()
    await expect(page.getByText('It picks up your notes, and lessons as it works')).toBeVisible()
    expect(readFileSync(join(fresh.claudeDir, 'settings.json'), 'utf8')).toContain('kelve moment')
    // The one choice left: the shared graph, or kelve without it.
    const join_ = page.getByRole('region', { name: 'Join the shared graph' })
    await expect(join_.getByRole('button', { name: /GitHub/ })).toBeVisible()
    await expect(join_.getByRole('button', { name: /Google/ })).toBeVisible()
    await expect(join_).toContainText('public domain (CC0)')
    await join_.getByRole('button', { name: 'Use kelve without it' }).click()
    await expect(join_).toHaveCount(0)
    // Anonymous counts, asked once (Count-1): no thanks, then a word to share.
    const asked = page.locator('.setup li').filter({ hasText: 'Share anonymous counts of how you use kelve' })
    await expect(asked).toContainText('Never your work, your name or your account.')
    await asked.getByRole('button', { name: 'No thanks' }).click()
    const not = page.locator('.setup li').filter({ hasText: 'Doesn’t share counts' })
    await expect(not).toBeVisible()
    await expect(asked).toHaveCount(0)
    // Changed in a word (Count-2): shared, a tick with Stop.
    await not.getByRole('button', { name: 'Share' }).click()
    const shared = page.locator('.setup li.done').filter({ hasText: 'It shares anonymous counts of how you use kelve' })
    await expect(shared.getByRole('button', { name: 'Stop' })).toBeVisible()
    // Never asked again.
    await page.reload()
    await expect(shared).toBeVisible()
    await expect(page.getByText('Share anonymous counts of how you use kelve')).toHaveCount(0)
    // It shows them something, in a new tab: the first time, here, to open
    // when they're ready.
    let before = shown(fresh.logFile).length
    await me.open({ what: 'folder', path: repo(), title: 'A first look' })
    await page.goto(await newLink(fresh.logFile, before))
    await expect(page).toHaveURL(/#\/o\/\d+/)
    await expect(page.getByText(/Claude Code opened .+\./)).toBeVisible()
    await page.getByRole('button', { name: /^Open / }).click()
    await expect(page.getByRole('heading', { name: 'kelve is set up.' })).toHaveCount(0)
    // From then on, what's opened goes straight there.
    before = shown(fresh.logFile).length
    await me.open({ what: 'folder', path: repo(), title: 'Another look' })
    await page.goto(await newLink(fresh.logFile, before))
    await expect(page.getByRole('heading', { level: 1, name: 'Another look' })).toBeVisible()
    await expect(page.getByRole('heading', { name: 'kelve is set up.' })).toHaveCount(0)
    // And the start page leads with what was opened lately: what was set up is done with.
    await page.goto(`${fresh.base}/`)
    await expect(page.getByRole('navigation', { name: 'Opened lately' })).toContainText('Another look')
    await expect(page.getByRole('heading', { name: 'kelve is set up.' })).toHaveCount(0)
    await expect(page.getByText('Claude Code is connected')).toHaveCount(0)
    await expect(page.getByText('To open something else, ask Claude Code:')).toBeVisible()
  } finally {
    fresh.stop()
  }
})

test('the first entry your agent writes asks to be added; signing in agrees, and sends it and what was kept', async ({ page, browserName }) => {
  const port = Number(process.env.E2E_FRESH_PORT ?? 4605) + 2 + (browserName === 'webkit' ? 1 : 0)
  const fresh = await startKelve(port, world().graph, { log: `e2e/.fresh-add-${browserName}.log` })
  try {
    await graph(world().graph, { unagree: true })
    const me = agent(fresh.base, secret(fresh))
    await me.hello()
    await signIn(page, fresh)
    const idea = `Request keys ${Date.now().toString(36).slice(-5)}`
    await me.open({ what: 'change', path: repo(idea), title: 'Retries that back off' })
    await page.goto(`${fresh.base}/#/o/1`)
    await me.work()
    await openFile(page, 'src/keys.rs')
    await marginNote(page, idea).click()
    await me.work()
    const ask = page.getByRole('region', { name: `Add ${idea} to the shared graph` })
    await expect(ask).toContainText(`Claude Code wrote ${idea} just for you.`, { timeout: 15_000 })
    await expect(ask).toContainText('Never your code, file names or notes, or which ideas your work uses.')
    await expect(ask).toContainText('Signing in, you agree to the shared graph’s terms.')

    // Signing in, through GitHub, which the stand-in graph does at once:
    // that's agreeing, so there's nothing more to click.
    const popup = page.waitForEvent('popup')
    await ask.getByRole('button', { name: 'Sign in with GitHub and add' }).click()
    await (await popup).waitForLoadState()

    // Back, and what was kept before agreeing goes to the graph.
    await expect(page.locator('.fv-name')).toContainText('src/keys.rs')
    await expect(ask).toHaveCount(0)
    await expect.poll(async () => (await (await page.request.get(`${fresh.base}/api/ideas/${encodeURIComponent(await keyOf(page, fresh.base, idea))}`)).json()).keptHere, { timeout: 15_000 }).toBe(false)
    // Settings say what you've added, and that signing in agreed.
    await page.goto(`${fresh.base}/#/you/graph`)
    await expect(page.getByLabel('What you’ve added')).toContainText(/entr(y|ies)/)
    await expect(page.getByText('You agreed to them as you signed in.')).toBeVisible()
    // The terms, a click away.
    await page.goto(`${fresh.base}/#/adding`)
    await expect(page.getByRole('listitem').first()).toContainText('lessons your agent learns')
    await expect(page.getByRole('listitem').nth(1)).toContainText('CC0')
    await expect(page.getByText(/You won't be paid/)).toBeVisible()
  } finally {
    fresh.stop()
  }
})

/** An idea's key, by its name, as search finds it. */
async function keyOf(page: import('@playwright/test').Page, base: string, name: string): Promise<string> {
  const found = await (await page.request.get(`${base}/api/search?q=${encodeURIComponent(name)}&opened=1`)).json()
  return found.ideas.find((i: { name: string }) => i.name === name).key
}
