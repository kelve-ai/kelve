// Every state of every screen, drawn from the example with no server: the
// states gallery shows them side by side, and the screenshot tests compare each
// with how it looked.

type Page = { key: string; name: string; entry: unknown; state: string }

export type State = {
  /** A name for its screenshots: `reader-entry`. */
  name: string
  /** What it shows, in a few words. */
  title: string
  /** The example's address for it. */
  path: string
  /** Also drawn at a phone's width. */
  phone?: boolean
  /** Drawn at this width rather than a desktop's: a laptop's, say. */
  width?: number
  /** What's clicked, by the words it shows, once it's drawn: a fold, opened. */
  clicks?: string[]
  /** What's pointed at, by the words it shows: a row of a list, showing what you can do with it. */
  points?: string
  /** What's pointed at, by its selector: kelve's name, showing where it leads. */
  hovers?: string
}

/** The ideas the example has, by name. */
function keysOf(answers: Record<string, unknown>): Map<string, Page> {
  const by = new Map<string, Page>()
  for (const [path, page] of Object.entries(answers)) {
    if (/^\/api\/ideas\/[^/?]+$/.test(path)) by.set((page as Page).name, page as Page)
  }
  return by
}

/** A lesson in the example with another good way beside it, by its ID. */
function withAnother(answers: Record<string, unknown>): string | undefined {
  for (const [path, page] of Object.entries(answers)) {
    if (path.startsWith('/api/lessons/kelve') && ((page as { another?: unknown[] }).another?.length ?? 0) > 0) return (page as { id: string }).id
  }
  return undefined
}

/** A lesson the example's agent was given that held, by its ID. */
function heldLesson(answers: Record<string, unknown>): string | undefined {
  const all = answers['/api/lessons'] as { given?: Array<{ id: string; used: Array<{ outcome: string }> }> } | undefined
  return all?.given?.find((l) => l.used.some((u) => u.outcome === 'held'))?.id
}

export function states(answers: Record<string, unknown>): State[] {
  const ideas = keysOf(answers)
  const key = (name: string) => encodeURIComponent(ideas.get(name)?.key ?? '')
  const written = [...ideas.values()].find((p) => !p.entry)
  // The example opens files plainly, as people's kelve does; these states
  // show a file as code unless they say plainly.
  const file = (path: string) => `f=${encodeURIComponent(path)}&r=code`
  const plain = (path: string) => `f=${encodeURIComponent(path)}&r=plainly`
  const held = heldLesson(answers)
  const another = withAnother(answers)
  return [
    { name: 'reader-change-page', title: 'A change, opened on its own page, plainly: how many of its ideas were already written, then its lessons (Value-1)', path: '#/example', phone: true },
    { name: 'reader-start-page-tip', title: 'kelve’s name pointed at: the way to the start page, and Esc (Home-1)', path: `#/example?${file('src/retry.rs')}`, hovers: '.topbar .logo' },
    { name: 'reader-change-writing', title: 'A change’s own page while your agent writes it, its files meanwhile', path: '#/example?as=writing' },
    { name: 'reader-change-next', title: 'A change’s own page, next for your agent', path: '#/example?as=wait-next' },
    { name: 'reader-change', title: 'A change, open on what changed in a code file', path: `#/example?${file('src/retry.rs')}`, phone: true },
    { name: 'reader-change-opened', title: 'What changed, with lines as before opened in place', path: `#/example?${file('src/retry.rs')}`, clicks: ['12 lines'] },
    { name: 'reader-reading', title: 'Still being read: how far, in your agent’s line', path: `#/example?${file('src/retry.rs')}&as=reading`, phone: true },
    { name: 'reader-stopped', title: 'Read part way, your agent stopped', path: `#/example?${file('src/retry.rs')}&as=stopped` },
    { name: 'reader-laptop', title: 'A laptop’s width: ideas float over the code', path: `#/example?${file('src/retry.rs')}&l=32`, width: 1100 },
    { name: 'reader-text', title: 'Text read as text', path: `#/example?${file('docs/retries.md')}` },
    { name: 'reader-plainly', title: 'A code file in a change read plainly, one part’s code shown', path: `#/example?${plain('src/retry.rs')}`, clicks: ['Show the code'], phone: true },
    { name: 'reader-plainly-text', title: 'A text file read plainly, one section shown', path: `#/example?${plain('docs/retries.md')}`, clicks: ['Show the text'] },
    { name: 'reader-plainly-waiting', title: 'A file not read plainly yet: how many files come before it, with Do next, its code meanwhile', path: `#/example?${plain('src/bank/mod.rs')}&as=reading`, phone: true },
    { name: 'reader-plainly-next', title: 'A file not read plainly yet, next for your agent', path: `#/example?${plain('src/bank/mod.rs')}&as=wait-next` },
    { name: 'reader-plainly-writing', title: 'A file being read plainly: a thin moving line', path: `#/example?${plain('src/bank/mod.rs')}&as=wait-writing` },
    { name: 'reader-plainly-reading-this', title: 'A step not read yet, opened: your agent reading it now (Read-3)', path: `#/example?${plain('src/bank/mod.rs')}&as=reading-this` },
    { name: 'reader-finding', title: 'Your agent finding a way in, in In order (Read-1)', path: '#/example?t=order&as=finding' },
    { name: 'reader-path-part', title: 'A part you asked for, open as you read in it: a file at work, its square a stage full (Way-2, Way-3)', path: `#/example?${file('tests/retry.rs')}&t=order&as=path` },
    { name: 'reader-path', title: 'The rest of the path in parts, after the way in: one you asked for being read, one waiting for you (Read-2, Read-4)', path: `#/example?${file('src/bank/mod.rs')}&t=order&as=path` },
    { name: 'reader-plainly-idle', title: 'A text file not read plainly yet, your agent not working: what to say to it', path: `#/example?${plain('docs/retries.md')}&as=wait-idle`, phone: true },
    { name: 'reader-plainly-copied', title: 'What to say, copied: where to paste it', path: `#/example?${plain('docs/retries.md')}&as=wait-idle`, clicks: ['Copy what to say'] },
    { name: 'reader-entry', title: 'An entry beside the work', path: `#/example?${file('src/retry.rs')}&p=${key('Exponential backoff')}`, phone: true },
    { name: 'reader-diagrams', title: 'An entry’s diagrams: a timeline and a row with long labels, each kept in its own box', path: `#/example?${file('src/retry.rs')}&p=${key('Exponential backoff')}&as=diagrams`, phone: true },
    { name: 'reader-diagram-crowded', title: 'A row with more nodes than its width holds, some one long word: drawn as a stack', path: `#/example?${file('src/retry.rs')}&p=${key('Unique keys')}&as=diagrams` },
    { name: 'reader-fold', title: 'An entry’s worked example, opened in place', path: `#/example?${file('src/retry.rs')}&p=${key('Exponential backoff')}`, clicks: ['A worked example'], phone: true },
    { name: 'reader-entry-from-graph', title: 'An entry from the shared graph: checked against its sources, and read widely (Value-2)', path: `#/example?${file('src/retry.rs')}&p=${key('Exponential backoff')}&as=read-widely` },
    { name: 'reader-entry-checking', title: 'An entry your agent just wrote, being checked: its line under the question, a square a claim', path: `#/example?${file('src/retry.rs')}&p=${key('Jitter')}&as=checking` },
    { name: 'reader-entry-held', title: 'An entry your agent wrote, waiting for today’s limit on adding to reset', path: `#/example?${file('src/retry.rs')}&p=${key('Jitter')}&as=held` },
    { name: 'reader-ask', title: 'Saying an entry didn’t help', path: `#/example?${file('src/retry.rs')}&p=${key('Exponential backoff')}`, clicks: ['Didn’t help me understand', 'Needs an example'] },
    { name: 'reader-down', title: 'Tracing down, two entries open', path: `#/example?${file('src/retry.rs')}&p=${key('Exponential backoff')}&p=~${key('Retries')}` },
    ...(written
      ? [
          { name: 'reader-waiting', title: 'An idea with no entry yet', path: `#/example?${file('src/retry.rs')}&p=${encodeURIComponent(written.key)}` },
          { name: 'reader-waiting-place', title: 'An idea with no entry yet, further down your agent’s list: Do next', path: `#/example?${file('src/retry.rs')}&p=${encodeURIComponent(written.key)}&as=wait-place` },
          { name: 'reader-waiting-writing', title: 'An entry being written: its steps under the line', path: `#/example?${file('src/retry.rs')}&p=${encodeURIComponent(written.key)}&as=wait-writing`, phone: true },
          { name: 'reader-waiting-idle', title: 'An idea with no entry yet, your agent not working', path: `#/example?${file('src/retry.rs')}&p=${encodeURIComponent(written.key)}&as=wait-idle` },
        ]
      : []),
    ...(held ? [{ name: 'reader-lesson', title: 'A lesson your agent used, opened beside the change: what showed it held', path: `#/example?p=${encodeURIComponent(`lesson:${held}`)}`, phone: true }] : []),
    ...(another ? [{ name: 'reader-lesson-another', title: 'A better way, written like an entry: what it replaces, and another good way beside it (Better-3)', path: `#/example?p=${encodeURIComponent(`lesson:${another}`)}` }] : []),
    { name: 'reader-lesson-learned', title: 'What your agent learned as it replaced its own work: the first way and the second, and Stop until it leaves (Better-2)', path: `#/example?p=${encodeURIComponent('lesson:learned:1')}` },
    { name: 'reader-every-file', title: 'Every file, beside the change', path: `#/example?${file('src/retry.rs')}&w=all` },
    { name: 'reader-search', title: 'Search', path: `#/example?${file('src/retry.rs')}&q=ret` },
    { name: 'reader-search-start', title: 'Search before you type: the change’s ideas, in the most files first, and its lessons', path: `#/example?${file('src/retry.rs')}&q=` },
    { name: 'reader-files', title: 'The files, each folder with what it’s for', path: `#/example?${file('src/retry.rs')}&t=files` },
    { name: 'reader-folder', title: 'A folder, plainly: the steps a request takes, each by the part that does it; folders’ lines wrap in the files', path: `#/example?d=src&t=files`, phone: true },
    { name: 'reader-folder-top', title: 'A folder, plainly: its parts by what each does', path: `#/example?d=&t=files` },
    { name: 'reader-folder-writing', title: 'A folder’s page being written, its parts meanwhile', path: `#/example?d=src&t=files&as=wait-writing` },
    { name: 'reader-marked', title: 'What kelve marks, beside the work', path: `#/example?${file('src/retry.rs')}&k=marks` },
    { name: 'reader-list', title: 'Your agent’s list, beside the work', path: `#/example?${file('src/retry.rs')}&k=list` },
    { name: 'reader-list-way', title: 'Your agent’s list: the way in being read, and its folders’ pages, each a line with what’s in hand; the rest’s pages next (List-1)', path: `#/example?${file('src/retry.rs')}&k=list&as=list-way` },
    { name: 'reader-list-rest', title: 'Your agent’s list: the way in read, every other folder’s page being written (List-2)', path: `#/example?${file('src/retry.rs')}&k=list&as=list-rest` },
    { name: 'reader-list-idle', title: 'Your agent’s list, your agent not working: what to say to it, at the top', path: `#/example?${file('src/retry.rs')}&k=list&as=next`, phone: true },
    { name: 'reader-list-take-off', title: 'Your agent’s list, a row pointed at: Do next and Take off', path: `#/example?${file('src/retry.rs')}&k=list&as=next`, points: 'Unique keys' },
    { name: 'reader-list-paused', title: 'Your agent’s list: what’s open first, what you opened before paused', path: `#/example?${file('src/retry.rs')}&k=list&as=paused` },
    { name: 'reader-list-paused-shown', title: 'What’s paused, shown, each with Open', path: `#/example?${file('src/retry.rs')}&k=list&as=paused`, clicks: ['Show'] },
    { name: 'reader-list-paused-now', title: 'Opening something else: what was open, paused just now', path: `#/example?${file('src/retry.rs')}&k=list&as=paused-just` },
    { name: 'first-run', title: 'First run: connect your agent', path: '#/example/first-run', phone: true },
    { name: 'start', title: 'Connected: what to say to your agent, and sharing counts asked once (Count-1)', path: '#/example/start' },
    { name: 'start-counts-shared', title: 'Connected, counts shared: a tick with Stop (Count-2)', path: '#/example/start?as=counts-shared' },
    { name: 'start-counts-not', title: 'Connected, counts not shared: a line with Share (Count-2)', path: '#/example/start?as=counts-not' },
    { name: 'start-returning', title: 'Coming back: what was opened lately first, and no ticks', path: '#/example/start?as=returning' },
    { name: 'you-agent', title: 'Settings: your agent', path: '#/example/you/agent', phone: true },
    { name: 'you-agent-update', title: 'Settings: your agent, a newer kelve out, and how to update', path: '#/example/you/agent?as=update', phone: true },
    { name: 'you-graph', title: 'Settings: the shared graph', path: '#/example/you/graph' },
    { name: 'you-lessons', title: 'Settings: lessons, what your agent learned waiting to leave, and what it was given', path: '#/example/you/lessons', phone: true },
    { name: 'you-graph-held', title: 'Settings: the shared graph, today’s limit on adding reached', path: '#/example/you/graph?as=held', phone: true },
    { name: 'you-reading', title: 'Settings: reading', path: '#/example/you/reading' },
    { name: 'you-marks', title: 'Settings: what kelve marks, and how files read', path: '#/example/you/marks' },
    { name: 'you-notes', title: 'Settings: notes', path: '#/example/you/notes' },
    { name: 'you-shared', title: 'Settings: what’s shared', path: '#/example/you/shared' },
    { name: 'adding', title: 'Adding to the shared graph', path: '#/example/adding' },
  ]
}
