// The app's own example: a change an agent made to a small payments service,
// already read and with its entries, answered from what the sandbox recorded,
// with no server. The first run shows the reader on it, beside its steps; the
// states gallery and the screenshot tests draw every screen from it: #/example
// the reader, with its own address after it #/example/first-run        before
// any agent has connected #/example/start            the start page, once one
// has #/example/you/<section>    settings #/example/adding           adding to
// the shared graph `theme=light` or `theme=dark` after the address draws it
// that way; `as=reading` draws the change while your agent is still reading it,
// `as=stopped` once it's stopped part way, `as=paused` with things opened
// before it paused, `as=paused-just` with one paused a moment ago, and
// `as=next` with several things next, your agent not working. `r=plainly` after
// a file's address reads it plainly, as every file, folder and the change are
// written, but `src/bank/mod.rs` while what's open is still being read;
// `as=writing` draws the change's own page while your agent is still writing
// it. What's not written yet is one line (boards Waiting-0 to Waiting-6):
// `as=wait-next` draws it first on your agent's list, `as=wait-writing` being
// written, `as=wait-place` further down, and `as=wait-idle` your agent not
// working, `docs/retries.md` not written either. `as=diagrams` gives
// Exponential backoff and Unique keys diagrams with many nodes and long labels.
// `as=held` draws today's limit on adding to the shared graph reached: in
// settings, and on Jitter's entry. `as=update` draws a newer kelve out, in
// settings. `as=checking` draws Jitter just written by your agent, its claims
// being checked; `as=read-widely`, Exponential backoff read 1,240 times. On the
// start page, anonymous counts are asked about; `as=counts-shared` draws them
// shared, and `as=counts-not` not; `as=returning` draws it for someone who's
// opened something from it before.

import { useEffect, useState } from 'react'
import { answerFromExample, type Fixture } from '../api/client'
import type { AgentList, Block, IdeaPage, ListItem, Me, PlainlyView, Reading, SentToday, Settings, UpdateView, Way } from '../api/types'
import { BACKOFF as BACKOFF_DIAGRAMS, UNIQUE_KEYS } from '../example/diagrams'
import { changeWaiting, folderWaiting } from '../example/plainly'
import { Loading } from '../Loader'
import { Reader } from '../reader/Reader'
import { PageTop } from '../Top'
import { Adding } from './Adding'
import { Start } from './Start'
import { You } from './You'

/** Jitter, which the example's agent wrote: what waits for today's limit, with `as=held`. */
const JITTER = { key: 'kelve:i000000000016' }

/** The change's `src/bank/mod.rs`, not read plainly yet: three files before it on your agent's list. */
const BANK_PLAINLY: PlainlyView = {
  path: 'src/bank/mod.rs',
  code: true,
  what: null,
  ideas: [],
  usedBy: [],
  change: { kind: 'fixes', says: null, movedFrom: null },
  parts: null,
  out: [],
  waiting: { writing: false, reading: false, before: 3, ask: 21 },
}

/** The screen the example's address names, and its part. */
function screenOf(hash: string): [string, string | undefined] {
  const [, screen = '', part] = hash.split('?')[0].replace(/^#\//, '').split('/')
  return [screen, part]
}

export function Example() {
  const [f, setF] = useState<Fixture | null>(null)
  const [screen, part] = screenOf(location.hash)
  useEffect(() => {
    const theme = new URLSearchParams(location.hash.split('?')[1] ?? '').get('theme')
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme
    void import('../example/example.json').then((m) => {
      const fixture = structuredClone(m.default) as unknown as Fixture
      // Code drawn as the example's settings say, as the app draws it from yours.
      const drawn = fixture.answers['/api/settings'] as Settings | undefined
      if (drawn) Object.assign(document.documentElement.dataset, { code: drawn.codeSize, text: drawn.textSize, highlighting: drawn.highlighting })
      // Before any agent has said hello.
      if (screen === 'first-run') Object.assign(fixture.answers['/api/me'] as Me, { agent: null, agents: [] })
      // The change is what's open: nothing else was asked for, and nothing's paused.
      const list = fixture.answers['/api/list'] as AgentList
      list.open ??= fixture.opened
      list.elsewhere ??= []
      list.paused ??= []
      // What the agent learned leaves tomorrow, whenever the example is shown.
      const tomorrow = new Date(Date.now() + 20 * 3600_000).toISOString()
      const leaving = (v: unknown): void => {
        if (Array.isArray(v)) v.forEach(leaving)
        else if (v && typeof v === 'object') {
          const o = v as Record<string, unknown>
          if (typeof o.leavesAt === 'string' && o.state === 'waiting') o.leavesAt = tomorrow
          Object.values(o).forEach(leaving)
        }
      }
      for (const [path, answer] of Object.entries(fixture.answers)) if (path.includes('lessons')) leaving(answer)
      // Still being read: three of the seven files, a fourth being checked.
      const as = new URLSearchParams(location.hash.split('?')[1] ?? '').get('as')
      // What your agent hasn't read yet has no ideas: In order shows where it's reading.
      const unread = (paths: string[]) => {
        const change = fixture.answers[`/api/opened/${fixture.opened}/change`] as { files: Array<{ path: string; ideas: unknown }> }
        for (const f of change.files) if (paths.includes(f.path)) f.ideas = null
      }
      const opened = fixture.answers[`/api/opened/${fixture.opened}`] as { way: Way }
      if (as === 'reading' || as === 'stopped' || as === 'path') unread(['tests/retry.rs', 'docs/retries.md'].concat(as === 'path' ? [] : ['src/bank/mod.rs']))
      // Your agent finding a way in (Read-1).
      if (as === 'finding') opened.way = { order: [], parts: [], folders: opened.way.folders, finding: true, reading: [], readSteps: [], leftOff: 0 }
      // The rest of the path, in parts, after the way in (Read-4): one you
      // asked for, its first file being read, and one waiting for you.
      if (as === 'path') {
        const [inWay, tests, docs] = [opened.way.order.slice(0, 5), opened.way.order.slice(5, 6), opened.way.order.slice(6)]
        const bare = (steps: typeof tests) => steps.map((s) => ({ ...s, why: '' }))
        opened.way = {
          ...opened.way,
          order: inWay,
          parts: [
            { name: 'What it promises', why: 'The tests, once the code is clear.', steps: bare(tests), queued: true },
            { name: 'For people', why: 'The docs, saying the same in words.', steps: bare(docs), queued: false },
          ],
          reading: tests.map((s) => ({ path: s.path, stage: 'checking' as const })),
        }
        list.working = true
      }
      if (as === 'reading' || as === 'stopped') {
        const change = fixture.answers[`/api/opened/${fixture.opened}/change`] as { reading: Reading }
        change.reading = { read: 3, of: 7, checking: 1, marking: 'marking', pages: 0, pagesLeft: 0 }
        list.working = as === 'reading'
        list.next = [{ key: `read:${fixture.opened}`, kind: 'reading', title: 'Reading its files', sub: null, opened: fixture.opened, idea: null, note: null, left: 4, askedIn: null, asks: [], hand: [] }]
      }
      // Opened after other things (boards J-1 to J-3): the change is read
      // first, a file asked for plainly in the folder still comes, and what was
      // opened before is paused; with `as=paused-just`, the folder was what
      // was open until a moment ago.
      if (as === 'paused' || as === 'paused-just') {
        const change = fixture.answers[`/api/opened/${fixture.opened}/change`] as { reading: Reading }
        change.reading = { read: 3, of: 7, checking: 0, marking: 'marking', pages: 0, pagesLeft: 0 }
        const ago = (ms: number) => new Date(Date.now() - ms).toISOString()
        const hour = 60 * 60 * 1000
        list.working = true
        list.now = [{ key: `read:${fixture.opened}`, kind: 'reading', title: 'Reading its files', sub: null, opened: fixture.opened, idea: null, note: null, left: 4, askedIn: null, asks: [], hand: [] }]
        list.next = []
        list.elsewhere = [
          { key: 'plainly:2:src/ledger.rs', kind: 'plainly', title: 'src/ledger.rs', sub: null, opened: 2, idea: null, note: null, left: null, askedIn: 'ledger', asks: [], hand: [] },
        ]
        list.paused = [
          { opened: 2, kind: 'folder', title: 'ledger', left: 31, openedAt: ago(as === 'paused-just' ? 2 * hour : 25 * hour), pausedAt: ago(as === 'paused-just' ? 5000 : 24 * hour) },
          { opened: 3, kind: 'change', title: 'Every charge carries a key', left: 12, openedAt: ago(26 * hour), pausedAt: ago(25 * hour) },
          { opened: 4, kind: 'file', title: 'retries.md', left: 1, openedAt: '2026-10-03T10:00:00.000Z', pausedAt: ago(26 * hour) },
        ]
      }
      // Several things next, your agent not working: reading what's open,
      // an entry asked for, and a note.
      if (as === 'next') {
        list.working = false
        list.next = [
          { key: `read:${fixture.opened}`, kind: 'reading', title: 'Reading its files', sub: null, opened: fixture.opened, idea: null, note: null, left: 4, askedIn: null, asks: [11, 12, 13, 14], hand: [] },
          { key: 'idea:kelve:unique-keys', kind: 'entry', title: 'Unique keys', sub: null, opened: fixture.opened, idea: 'kelve:unique-keys', note: null, left: null, askedIn: null, asks: [15], hand: [] },
          { key: 'note:agent/say-what-a-retry-costs.md', kind: 'note', title: 'Say what a retry costs the bank', sub: 'On src/retry.rs 12–28', opened: fixture.opened, idea: null, note: 'agent/say-what-a-retry-costs.md', left: null, askedIn: null, asks: [16], hand: [] },
        ]
      }
      // The way in being read, and its folders' pages, each a line with
      // what's in hand; the rest's pages next (List-1). Then the way in
      // read, the rest's pages being written (List-2).
      if (as === 'list-way' || as === 'list-rest') {
        const change = fixture.answers[`/api/opened/${fixture.opened}/change`] as { reading: Reading }
        const line = (key: string, kind: string, title: string, sub: string | null, left: number, hand: ListItem['hand'], of?: number): ListItem => ({ key: `${key}:${fixture.opened}`, kind, title, sub, opened: fixture.opened, idea: null, note: null, left, askedIn: null, asks: [21], of, hand })
        const rest = 'Every folder and the folders in it, the shallowest first, so you can find your own way after the way in.'
        list.working = true
        list.next = []
        if (as === 'list-way') {
          change.reading = { read: 3, of: 10, checking: 2, marking: 'marking', pages: 3, pagesLeft: 2 }
          list.now = [
            line('read', 'reading', 'Reading the way in', null, 4, [
              { path: 'src/api/v1/charges.rs', stage: 'writing' },
              { path: 'src/api/v1/refunds.rs', stage: 'checking' },
              { path: 'src/bank/mod.rs', stage: 'reading' },
            ]),
            line('pages', 'pages', 'Folder pages for the way in', 'Each once its files are read plainly, the deepest first.', 2, [{ path: 'migrations', stage: 'writing' }]),
          ]
          list.next = [line('rest', 'pages', 'Folder pages for the rest', rest, 9, [], 9)]
        } else {
          change.reading = { read: 10, of: 10, checking: 0, marking: 'done', pages: 3, pagesLeft: 0 }
          list.now = [line('rest', 'pages', 'Folder pages for the rest', rest, 6, [
            { path: 'src/api', stage: 'writing' },
            { path: 'docs', stage: 'writing' },
          ], 9)]
        }
      }
      // Signed in to the shared graph, as a person is: agreed by signing in,
      // not yet when adding.
      // What kelve marks for the person, as their agent wrote it.
      fixture.answers['/api/marks'] = {
        spec: {
          mark: 'working',
          read: 'plainly',
          areas: [
            { area: 'rust', level: 'deep' },
            { area: 'distributed systems', level: 'new' },
            { area: 'postgresql', level: 'learning' },
            { area: 'sql', level: 'learning', read: 'plainly' },
          ],
          always: ['Idempotency keys'],
          never: [],
          words: 'I came to backend from frontend work. Mark anything about how money moves.',
          by: 'agent',
          at: '2026-10-22',
        },
        kept: true,
        file: '~/notes/marks.md',
      }
      // Every file, folder and the change itself is read plainly, as the
      // example's agent wrote them (dev/example/agent/plainly.json); while
      // what's open is still being read, src/bank/mod.rs isn't yet.
      const at = `/api/opened/${fixture.opened}/plainly`
      const bank = structuredClone(BANK_PLAINLY)
      if (as && ['reading', 'stopped', 'wait-next', 'wait-writing', 'wait-place', 'wait-idle', 'reading-this'].includes(as)) {
        fixture.answers[`${at}?path=${encodeURIComponent(bank.path)}`] = bank
      }
      const retries = fixture.answers[`${at}?path=${encodeURIComponent('docs/retries.md')}`] as PlainlyView
      // Reading what's open, as a row of the list; an idea with no entry
      // yet, and a note, as theirs.
      const reading = (left: number, title = 'Reading its files'): ListItem => ({ key: `read:${fixture.opened}`, kind: 'reading', title, sub: null, opened: fixture.opened, idea: null, note: null, left, askedIn: null, asks: [11, 12, 13, 14], hand: [] })
      const unwritten = Object.entries(fixture.answers).find(([path, page]) => /^\/api\/ideas\/[^/?]+$/.test(path) && !(page as IdeaPage).entry)?.[1] as IdeaPage | undefined
      const entry: ListItem = { key: `idea:${unwritten?.key}`, kind: 'entry', title: unwritten?.name ?? '', sub: null, opened: fixture.opened, idea: unwritten?.key ?? null, note: null, left: null, askedIn: null, asks: [15], hand: [] }
      const note: ListItem = { key: 'note:agent/say-what-a-retry-costs.md', kind: 'note', title: 'Say what a retry costs the bank', sub: 'On src/retry.rs 12–28', opened: fixture.opened, idea: null, note: 'agent/say-what-a-retry-costs.md', left: null, askedIn: null, asks: [16], hand: [] }
      if (as === 'writing') {
        fixture.answers[`${at}/change`] = changeWaiting({ writing: true, reading: false, before: 0, ask: 31 })
        list.working = true
        list.now = [reading(1)]
        list.next = []
      }
      if (as === 'wait-next') {
        fixture.answers[`${at}/change`] = changeWaiting({ writing: false, reading: false, before: 0, ask: 31 })
        bank.waiting = { writing: false, reading: false, before: 0, ask: 21 }
        list.working = true
        list.next = [reading(4)]
      }
      if (as === 'wait-writing') {
        fixture.answers[`${at}/folder?path=src`] = folderWaiting('src', { writing: true, reading: false, before: 0, ask: 32 })
        bank.waiting = { writing: true, reading: false, before: 0, ask: 21 }
        for (const [path, page] of Object.entries(fixture.answers)) {
          if (path.startsWith(`/api/ideas/${encodeURIComponent(unwritten?.key ?? '')}`) && !path.includes('/outline')) Object.assign(page as IdeaPage, { stage: 'writing', step: 'Being written' })
        }
        list.working = true
        list.now = [reading(4), entry]
        list.next = []
      }
      // The file itself being read now: it went first, being looked at (Read-3).
      if (as === 'reading-this') {
        bank.waiting = { writing: false, reading: true, before: 0, ask: 21 }
        list.working = true
        list.now = [reading(4)]
        list.next = []
      }
      if (as === 'wait-place') {
        list.working = true
        list.now = [reading(4)]
        list.next = [note, entry]
      }
      if (as === 'wait-idle') {
        Object.assign(retries, { what: null, parts: null, out: [], change: { kind: 'adds', says: null, movedFrom: null }, waiting: { writing: false, reading: false, before: 2, ask: 22 } })
        list.working = false
        list.next = [reading(4), entry, note]
      }
      // Diagrams, first in each entry's part on how it works.
      if (as === 'diagrams') {
        for (const [name, blocks] of [['Exponential backoff', BACKOFF_DIAGRAMS], ['Unique keys', UNIQUE_KEYS]] as const) {
          for (const [path, page] of Object.entries(fixture.answers)) {
            const p = page as { name?: string; entry?: { blocks: Block[] } }
            if (!path.startsWith('/api/ideas/') || p.name !== name || !p.entry) continue
            const at = p.entry.blocks.findIndex((b) => b.part === 'how')
            p.entry.blocks.splice(at, 0, ...blocks)
          }
        }
      }
      fixture.answers['/api/graph/account'] = { graph: true, from: 'app', handle: 'quiet-heron', waiting: false, agreed: screen !== 'adding' }
      // What was sent today; with `as=held`, a new account's ideas for
      // the day ran out, so the rest waits until midnight UTC, and Jitter,
      // which your agent wrote, waits with it.
      const held = as === 'held'
      const midnight = new Date()
      midnight.setUTCHours(24, 0, 0, 0)
      const today: SentToday = held
        ? { sent: 3, accepted: 1, checking: 1, turnedAway: 1, held: { until: midnight.toISOString(), what: 'ideas', newAccount: true } }
        : { sent: 2, accepted: 1, checking: 1, turnedAway: 0, held: null }
      fixture.answers['/api/graph/today'] = today
      if (held) {
        const jitter = fixture.answers[`/api/ideas/${encodeURIComponent(JITTER.key)}`] as IdeaPage | undefined
        if (jitter) Object.assign(jitter, { state: 'writing', stage: 'checking', step: 'Waiting to go to the shared graph until today’s limit resets.', checking: null, keptHere: false })
      }
      // Just written by your agent, its claims being checked, two of three done.
      if (as === 'checking') {
        for (const [path, page] of Object.entries(fixture.answers)) {
          if (!path.startsWith(`/api/ideas/${encodeURIComponent(JITTER.key)}`) || path.includes('/outline')) continue
          const p = page as IdeaPage
          if (!p.checking) continue
          p.checking = { status: 'pending', claims: p.checking.claims.map((c, i) => ({ ...c, state: i < 2 ? 'held' : 'waiting' })) }
          Object.assign(p, { state: 'writing', stage: 'checking', canFlag: false })
        }
      }
      // An entry the graph had, read widely.
      if (as === 'read-widely') {
        for (const [path, page] of Object.entries(fixture.answers)) {
          if (path.startsWith('/api/ideas/') && (page as IdeaPage).name === 'Exponential backoff' && (page as IdeaPage).fromGraph) (page as IdeaPage).reads = 1240
        }
      }
      // Asked once about sharing counts; answered, shared or not.
      if (as === 'counts-shared' || as === 'counts-not') {
        Object.assign(fixture.answers['/api/settings'] as Settings, { shareCounts: as === 'counts-shared', hints: [...(fixture.answers['/api/settings'] as Settings).hints, 'counts'] })
      }
      // Someone coming back: the start page no longer says what was set up.
      if (as === 'returning') {
        const s = fixture.answers['/api/settings'] as Settings
        s.hints = [...s.hints, 'started']
      }
      // kelve is up to date; with `as=update`, a newer one is out.
      const update: UpdateView = as === 'update'
        ? { version: '0.2.1', newer: { version: '0.3.0', line: 'Your agent can update kelve for you, once you say yes.' } }
        : { version: '0.2.1', newer: null }
      fixture.answers['/api/update'] = update
      answerFromExample(fixture)
      setF(fixture)
    })
  }, [screen])
  if (!f) return <Loading />
  const me = f.answers['/api/me'] as Me
  if (screen === '') return <Reader id={f.opened} example />
  return (
    <>
      <PageTop here={screen === 'you' ? 'settings' : 'start'} settings="#/example/you/agent" />
      <div className="page">
        {(screen === 'first-run' || screen === 'start') && <Start me={me} />}
        {screen === 'you' && <You focus={part} me={me} />}
        {screen === 'adding' && <Adding idea="Jitter" />}
      </div>
    </>
  )
}
