// Home and the debrief, as people use them (boards Home, Debrief,
// DebriefSent, ChangeView): the agent hands its work over and writes its
// debrief; home asks you to read it; you agree, ask for a decision another
// way, have your agent look into one, ask for an idea to be explained or
// say you know it, and send it, which leaves your agent a note. A small
// change can be debriefed anyway. The change shows each file, each
// decision beside its lines. The stand-in agent writes the debriefs, the
// research and the explanations, as kelve asks (e2e/debriefs.ts).

import { expect, type Page, test } from '@playwright/test'
import { debriefed, explained, handOver, researched } from '../debriefs'
import { me, signIn } from '../world'

/** A title no other test, or run, has. */
const unique = (title: string) => `${title} ${Date.now().toString(36)}`

const decision = (page: Page, title: string) => page.getByRole('article', { name: new RegExp(title) })

test('home asks you to read the debrief, and you answer it and send it', async ({ page }) => {
  const title = unique('Retries that back off')
  const { id, root } = await debriefed(title)
  await signIn(page)
  // Jitter is new to you, as it was before any other run said you know it.
  const before = (await (await page.request.get('/api/marks')).json()) as { spec: { never: string[] } }
  await page.request.post('/api/marks', { data: { ...before.spec, never: before.spec.never.filter((n) => n !== 'Jitter') } })
  await page.goto('/#/')
  const bubble = page.getByRole('region', { name: 'From your agent' })
  await expect(bubble.getByText(`I finished ${title}. I made 2 decisions you might want a say in.`)).toBeVisible()
  await bubble.locator('.hm-ask').filter({ hasText: title }).getByRole('link', { name: 'Read the debrief' }).click()
  await expect(page).toHaveURL(new RegExp(`#/debrief/${id}$`))
  await expect(page.getByRole('heading', { level: 1, name: title })).toBeVisible()
  await expect(page.getByText('2 to answer')).toBeVisible()

  // What you said, agreed: kept for every time, in your Rust skill to be.
  const wait = decision(page, 'Wait twice as long each time')
  await wait.getByRole('button', { name: 'Agree' }).click()
  await expect(wait.getByText('Agreed', { exact: true })).toBeVisible()
  await expect(wait.getByRole('checkbox')).toBeChecked()
  await expect(wait.getByText('in a new skill, Rust')).toBeVisible()
  // Its code, folded away until opened.
  await wait.getByRole('button', { name: 'Show the code' }).click()
  await expect(wait.getByRole('group', { name: 'The code: src/retry.rs, lines 1 to 4' })).toContainText('200 * 2u64.pow(attempt)')

  // Another way: a note for your agent, as you send it.
  const start = decision(page, 'Start from 200 milliseconds')
  await start.getByRole('button', { name: 'Change it' }).click()
  await start.getByRole('textbox', { name: 'How you want it' }).fill('Start from half a second.')
  await page.getByRole('heading', { level: 2, name: 'How it works' }).click()
  await expect(start.getByText('Redoing', { exact: true })).toBeVisible()
  await expect(page.getByText('All answered')).toBeVisible()

  // An idea explained, and one you know.
  const backoff = page.locator('.db-idea').filter({ hasText: 'Exponential backoff.' })
  await backoff.getByRole('button', { name: 'Explain' }).click()
  await expect(backoff.getByText('Your agent is writing this.')).toBeVisible()
  await explained(id, 'Exponential backoff', 'In this change, `wait()` doubles each wait: 200, 400, 800 milliseconds.')
  await page.reload()
  await backoff.getByRole('button', { name: 'Explain' }).click()
  await expect(backoff.getByText('doubles each wait')).toBeVisible()
  const jitter = page.locator('.db-idea').filter({ hasText: 'Jitter.' })
  await expect(jitter.getByText('New to you')).toBeVisible()
  await jitter.getByRole('button', { name: 'I know this' }).click()
  await expect(jitter.getByText('New to you')).toHaveCount(0)
  const marks = (await (await page.request.get('/api/marks')).json()) as { spec: { never: string[] } }
  expect(marks.spec.never).toContain('Jitter')

  // What happens when it's sent, then sent.
  const send = page.locator('.db-send')
  await expect(send.getByText('Your agent redoes decision 2.')).toBeVisible()
  await expect(send.getByText('Decision 1 starts a new skill, Rust.')).toBeVisible()
  await page.getByRole('button', { name: 'Send to your agent' }).click()
  await expect(page.getByRole('heading', { level: 1, name: 'Sent to your agent' })).toBeVisible()
  await expect(page.getByText('Your agent redoes decision 2 when you next message it.')).toBeVisible()
  await expect(page.getByText('Your agent drafts a new skill, Rust, from decision 1.')).toBeVisible()

  // The note waits for your agent, on the decision's lines.
  await expect
    .poll(async () => {
      const notes = await me().notes(root)
      return notes.map((n) => n.note).join('\n')
    }, { timeout: 20_000 })
    .toContain('Start from half a second.')

  // Home: done, a line on the timeline.
  await page.getByRole('link', { name: 'Back to home' }).click()
  const line = page.locator('.hm-row').filter({ hasText: title })
  await expect(line.getByRole('link', { name: 'Open' })).toBeVisible()
  await expect(page.locator('.hm-ask').filter({ hasText: title })).toHaveCount(0)
})

test('your agent looks into a decision, with your question, and you redo it its way', async ({ page }) => {
  const title = unique('Retries, looked into')
  const { id } = await debriefed(title)
  await signIn(page)
  await page.goto(`/#/debrief/${id}`)
  const start = decision(page, 'Start from 200 milliseconds')
  await start.getByRole('button', { name: 'Look into it' }).click()
  const question = start.getByRole('textbox', { name: 'What should your agent find out?' })
  await expect(question).toHaveValue('Is this the best way here, or would another be better?')
  await question.fill('Should it start from a second?')
  await start.getByRole('button', { name: 'Ask your agent' }).click()
  await expect(start.getByText('Your agent is looking into it.')).toBeVisible()
  await expect(start.getByText('Researching', { exact: true })).toBeVisible()
  // Home says what waits for your agent.
  await page.goto('/#/')
  await expect(page.getByRole('region', { name: 'From your agent' })).toContainText('I’ll look into your')

  await researched(id, 2, '#### What your agent found\n\nStart from **half a second**: a blip passes, and nobody waits long.\n\n| Option | For | Against |\n|---|---|---|\n| 200 milliseconds | Quick | Often too soon |\n| **Half a second** | Rides out a blip | A little slower |\n\n**It suggests** starting from half a second.\n\nSources: [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110)')
  await page.goto(`/#/debrief/${id}`)
  await expect(start.getByText('Research ready', { exact: true })).toBeVisible()
  await expect(start.getByRole('table')).toContainText('Rides out a blip')
  await start.getByRole('button', { name: 'Redo it this way' }).click()
  await expect(start.getByText('Redoing', { exact: true })).toBeVisible()
  await expect(start.getByRole('textbox', { name: 'How you want it' })).toHaveValue(/half a second/)
})

test('a small change can be debriefed anyway, and the change shows each decision beside its lines', async ({ page }) => {
  const small = unique('A typo in the README')
  await handOver(small)
  await signIn(page)
  await page.goto('/#/')
  const row = page.locator('.hm-row').filter({ hasText: small })
  await row.getByRole('button', { name: 'Debrief it' }).click()
  await expect(page.getByRole('heading', { level: 1, name: small })).toBeVisible()
  await expect(page.getByText('Waiting for your agent')).toBeVisible()
  await expect(page.getByText('It’ll be on home when it’s ready.')).toBeVisible()

  const title = unique('Retries, file by file')
  const { id } = await debriefed(title)
  await page.goto(`/#/debrief/${id}`)
  await page.getByRole('link', { name: 'Open the change' }).click()
  await expect(page.getByRole('heading', { level: 1, name: title })).toBeVisible()
  const files = page.getByRole('navigation', { name: 'Files' })
  await expect(files.getByRole('button', { name: /retry\.rs/ })).toHaveAttribute('aria-current', 'true')
  await expect(page.getByRole('textbox', { name: 'src/retry.rs, read only' })).toContainText('200 * 2u64.pow(attempt)')
  const note = page.locator('.cv-note').filter({ hasText: '1. Wait twice as long each time' })
  await expect(note).toBeVisible()
  await expect(note).toContainText('Your call')
  await note.click()
  await expect(page).toHaveURL(new RegExp(`#/debrief/${id}$`))
})

test('the example debrief, to try, changes nothing', async ({ page }) => {
  await signIn(page)
  await page.goto('/?example#/example/home?as=empty')
  await expect(page.getByText('Hello. I’m Sam’s agent.')).toBeVisible()
  await page.getByRole('link', { name: 'Try the example' }).click()
  await expect(page.getByRole('heading', { level: 1, name: 'Retry failed downloads with a growing wait' })).toBeVisible()
  // In kelve, the same, saying it's an example.
  await page.goto('/#/debrief/example')
  await expect(page.getByText('An example to try. Nothing here changes your skills.')).toBeVisible()
  await page.getByRole('button', { name: 'Agree with the rest' }).click()
  await expect(page.getByText('All answered')).toBeVisible()
  await page.getByRole('button', { name: 'Send to your agent' }).click()
  await expect(page.getByRole('heading', { level: 1, name: 'Sent to your agent' })).toBeVisible()
})
