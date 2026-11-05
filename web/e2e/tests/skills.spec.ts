// Your skills: none yet, then one written by hand; a skill's page, rendered
// and as its SKILL.md; editing it, with what's wrong said before anything's
// saved; taking a part out and undoing it; checking new parts on past work,
// the stand-in agent redoing it with each part and without; and Fits you,
// picked blind. Past work is put in the store as kelve's agent loop keeps it.

import { execFileSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, readdirSync, realpathSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { expect, test, type Page } from '@playwright/test'
import { me, signIn, world } from '../world'

/** A name no other run of these tests has used, so each starts afresh. */
const fresh = (name: string) => `${name}-${Date.now().toString(36)}`

/** Writes a skill of the person's own, as they would in their editor. */
function skill(name: string, title: string, parts: string[]) {
  const dir = join(world().skillsDir, name)
  mkdirSync(dir, { recursive: true })
  writeFileSync(
    join(dir, 'SKILL.md'),
    `---\nname: ${name}\ndescription: How Sam wants ${title} done. Use when doing ${title}.\n---\n\n# ${title}\n\nFor the tests.\n\n## How you like it\n\n${parts.join('\n\n')}\n`,
  )
}

const git = (root: string, ...args: string[]) =>
  execFileSync('git', ['-c', 'user.name=Test', '-c', 'user.email=test@example.com', '-c', 'commit.gpgsign=false', ...args], { cwd: root }).toString().trim()

/** Past work, as kelve keeps it once the person keeps what their agent handed over. */
/** Runs SQL on the test kelve's own store, as kelve's own work would change it. */
function sql(statement: string) {
  const { dataDir } = world()
  const db = readdirSync(dataDir).find((f) => f.endsWith('.db'))
  if (!db) throw new Error('kelve has no store yet')
  execFileSync('sqlite3', ['-cmd', '.timeout 5000', join(dataDir, db), statement])
}

const q = (s: string) => `'${s.replace(/'/g, "''")}'`

function pastTask(request: string) {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'kelve-e2e-past-')))
  mkdirSync(join(root, 'src'))
  writeFileSync(join(root, 'src/lib.rs'), 'fn main() {}\n')
  git(root, 'init', '-q', '-b', 'main')
  git(root, 'add', '.')
  git(root, 'commit', '-q', '-m', 'start')
  const start = git(root, 'rev-parse', 'HEAD')
  writeFileSync(join(root, 'src/lib.rs'), 'fn main() {}\n// with your skills\n')
  git(root, 'commit', '-q', '-am', 'kept')
  const kept = git(root, 'rev-parse', 'HEAD')
  sql(`insert into past_tasks (request, root, start, kept, at) values (${q(request)}, ${q(root)}, ${q(start)}, ${q(kept)}, ${Date.now()})`)
}

/** The note beside a part's heading. */
const note = (page: Page, heading: string) => page.locator('.sk-part').filter({ has: page.getByRole('heading', { name: heading }) }).locator('.sk-note')

test.afterEach(() => {
  // Each test's skills go with it: the next starts with none.
  rmSync(world().skillsDir, { recursive: true, force: true })
})

test('with no skills, you can write one yourself', async ({ page }) => {
  await me().hello()
  await signIn(page)
  await page.goto('/#/skills')
  await expect(page.getByRole('heading', { level: 1, name: 'No skills yet' })).toBeVisible()
  await page.getByRole('link', { name: 'Write one yourself' }).click()
  await expect(page.getByRole('heading', { level: 1, name: 'A new skill' })).toBeVisible()
  await expect(page.getByText('Each ### heading is one part. kelve checks each part.')).toBeVisible()

  const name = fresh('words')
  const box = page.getByRole('textbox', { name: 'Markdown' })
  await box.fill(`---\nname: ${name}\n---\n\n# Words\n`)
  await page.getByRole('button', { name: /^Save/ }).click()
  // Nothing's written till it's right.
  await expect(page.getByRole('alert')).toContainText('The skill has no description.')
  await box.fill(`---\nname: ${name}\ndescription: How Sam writes app copy. Use when writing words people see.\n---\n\n# Words\n\n## How you like it\n\n### Buttons say what they do\nLike Delete note, never OK.\n`)
  // The preview follows as you type.
  await expect(page.getByRole('region', { name: 'Preview' }).getByRole('heading', { name: 'Buttons say what they do' })).toBeVisible()
  await box.press(process.platform === 'darwin' ? 'Meta+s' : 'Control+s')
  await expect(page).toHaveURL(new RegExp(`#/skills/${name}$`))
  await expect(page.getByRole('heading', { level: 1, name: 'Words' })).toBeVisible()
  await expect(note(page, 'Buttons say what they do')).toContainText('Not checked yet')
  await expect(page.getByRole('navigation', { name: 'Your skills' }).getByRole('link', { name: 'Words' })).toHaveAttribute('aria-current', 'page')
})

test('a skill shows rendered and as its SKILL.md, is edited, and a part taken out comes back with Undo', async ({ page }) => {
  await me().hello()
  const name = fresh('style')
  skill(name, 'Style', ['### Errors are one enum\nWith a message for each.', '### Never unwrap outside tests\nReturn the error instead.'])
  await signIn(page)
  await page.goto('/#/skills')
  await page.getByRole('navigation', { name: 'Your skills' }).getByRole('link', { name: 'Style' }).click()
  await expect(page.getByRole('heading', { level: 1, name: 'Style' })).toBeVisible()
  await expect(note(page, 'Errors are one enum')).toContainText('Not checked yet')

  // As its SKILL.md: each part's mark beside its ### line, and only there.
  await page.getByRole('button', { name: 'Markdown' }).click()
  const src = page.getByRole('region', { name: 'SKILL.md' })
  await expect(src).toContainText(`name: ${name}`)
  await expect(src.locator('.sk-ln').filter({ hasText: '### Errors are one enum' }).locator('.sk-mk')).toContainText('Not checked yet')
  await expect(src.locator('.sk-ln').filter({ hasText: 'With a message for each.' }).locator('.sk-mk')).toBeEmpty()
  await page.getByRole('button', { name: 'Rendered' }).click()

  // Checked, and it made no difference: taken out, and back with Undo.
  sql(`update parts set state = 'no_difference', note = 'The same on both past tasks, with or without it' where key = 'never unwrap outside tests' and skill = (select id from skills where name = ${q(name)})`)
  await page.reload()
  await expect(note(page, 'Never unwrap outside tests')).toContainText('Makes no difference')
  await note(page, 'Never unwrap outside tests').getByRole('button', { name: 'Remove' }).click()
  await expect(page.getByRole('status').filter({ hasText: 'Removed from Style.' })).toBeVisible()
  await expect(page.getByRole('heading', { name: 'Never unwrap outside tests' })).toHaveCount(0)
  await page.getByRole('button', { name: 'Undo' }).click()
  await expect(page.getByRole('heading', { name: 'Never unwrap outside tests' })).toBeVisible()

  // Edited: a part added waits to be checked.
  await page.getByRole('link', { name: 'Edit' }).click()
  await expect(page.getByRole('heading', { level: 1, name: 'Editing Style' })).toBeVisible()
  const box = page.getByRole('textbox', { name: 'Markdown' })
  const text = await box.inputValue()
  await box.fill(text.replace(`name: ${name}`, 'name: other'))
  await expect(page.getByRole('status').filter({ hasText: 'Not saved yet' })).toBeVisible()
  await page.getByRole('button', { name: /^Save/ }).click()
  await expect(page.getByRole('alert')).toContainText(`The name has to stay “${name}”`)
  await box.fill(`${text}\n### A duration is a Duration\nA number doesn't say which unit.\n`)
  await page.getByRole('button', { name: /^Save/ }).click()
  await expect(page.getByRole('status').filter({ hasText: 'Saved. 2 parts aren’t checked yet.' })).toBeVisible()
  await page.getByRole('button', { name: 'Check them' }).click()
  await expect(page.getByRole('dialog', { name: 'No past work to check on yet' }).or(page.getByRole('dialog', { name: 'Check 2 new parts of Style?' }))).toBeVisible()
})

test('new parts are checked on past work, as your agent redoes it with each and without', async ({ page }) => {
  const agent = me()
  await agent.hello()
  await agent.work()
  const name = fresh('checked')
  skill(name, 'Checked', ['### Comment what the code is for\nSo the next person knows.', '### Keep functions short\nSo each does one thing.'])
  await signIn(page)
  pastTask('Add a main that does nothing yet.')
  await page.goto(`/#/skills/${name}`)
  await note(page, 'Keep functions short').getByRole('button', { name: 'Check it' }).click()
  const dialog = page.getByRole('dialog', { name: 'Check 2 new parts of Checked?' })
  await expect(dialog).toContainText(/About \d+ runs of your agent\./)
  await expect(dialog).toContainText('Comment what the code is for')
  await dialog.getByRole('button', { name: 'Check them' }).click()
  await expect(page.getByText('Checking 2 new parts')).toBeVisible()
  await expect(note(page, 'Keep functions short')).toContainText('Checking now')

  const jobs = await agent.work()
  expect([...new Set(jobs.filter((j) => j.kind === 'compare').map((j) => j.about))].sort()).toEqual([
    'Checking “Comment what the code is for” on past work',
    'Checking “Keep functions short” on past work',
  ])
  await expect(note(page, 'Keep functions short')).toContainText('Works')
  await expect(note(page, 'Keep functions short')).toContainText(/Better on (\d+) of \1 past tasks?/)
  await expect(page.getByText('Checking 2 new parts')).toHaveCount(0)
})

test('a check can be stopped, and its parts wait to be checked again', async ({ page }) => {
  await me().hello()
  const name = fresh('stopped')
  skill(name, 'Stopped', ['### Name tests as sentences\nSo a failing test says what broke.'])
  await signIn(page)
  pastTask('Add a test.')
  await page.goto(`/#/skills/${name}?check`)
  await page.getByRole('dialog', { name: 'Check a new part of Stopped?' }).getByRole('button', { name: 'Check it' }).click()
  await page.getByRole('button', { name: 'Stop the check' }).click()
  await expect(page.getByRole('status').filter({ hasText: 'Stopped. Those parts aren’t checked yet.' })).toBeVisible()
  await expect(note(page, 'Name tests as sentences')).toContainText('Not checked yet')
})

test('Fits you shows two results of past work blind, then which was yours', async ({ page }) => {
  const agent = me()
  await agent.hello()
  await agent.work()
  skill(fresh('fits'), 'Fits', ['### Comment what the code is for\nSo the next person knows.'])
  await signIn(page)
  pastTask('Say what main is for.')
  await page.goto('/#/agent/fits')
  // Pairs left from before are picked first.
  const ask = page.getByRole('heading', { level: 1, name: 'Which would you rather keep?' })
  const start = page.getByRole('heading', { level: 1, name: 'Fits you' })
  await expect(ask.or(start)).toBeVisible()
  while (await ask.isVisible()) {
    await page.getByRole('button', { name: 'About the same' }).click()
    await page.getByRole('button', { name: 'Next' }).click()
    await expect(ask.or(start)).toBeVisible()
  }
  await page.getByRole('button', { name: 'Start' }).click()
  await expect(page.getByText(/Your agent is redoing/)).toBeVisible()
  await agent.work()
  await expect(page.getByRole('heading', { level: 1, name: 'Which would you rather keep?' })).toBeVisible({ timeout: 10_000 })
  const a = await page.getByLabel('A’s change').innerText()
  await page.getByRole('button', { name: a.includes('with your skills') ? 'Keep A' : 'Keep B' }).click()
  await expect(page.getByRole('heading', { level: 1, name: 'You picked your agent' })).toBeVisible()
  await expect(page.getByText('Yours came 100% close to what you kept; the standard one, 0%.')).toBeVisible()
})
