// Every control kelve's screens are built from (ui/kit.tsx), on one page in
// the example, #/example/kit, so the states gallery and the screenshot tests
// show each, light and dark. `as=dialog` opens the dialog; `as=toast` shows
// the toast.

import { useState } from 'react'
import { Header } from '../Header'
import { Character, type Move } from '../ui/Character'
import { Btn, Chip, CopyLine, Dialog, Kbd, Segmented, Spinner, TextField, TextLink, Toast, Toggle, type Tone } from '../ui/kit'
import { Markdown } from '../ui/markdown'

const PENCIL = (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M18.4 2.6a2 2 0 0 1 2.9 2.9L11 15.8 8 16l.2-3 10.2-10.4Z" />
    <path d="M7 17c-2 0-3.5 1.4-3.5 3.5H7a3 3 0 0 0 0-6" />
  </svg>
)

const SKILL = `# Rust

How Sam likes Rust written, in \`fetch\` and everywhere else.

## How you like it

### Errors are one enum, with a message for each

One \`Error\` enum for the crate, so every failure says what to do next.

### Bad arguments exit 1, the tool’s own failures exit 2

- \`1\` when what was asked can't be done
- \`2\` when something went wrong in \`fetch\` itself

## Watch out for

### Fix clippy warnings, never quiet them

\`\`\`rust
// Not this:
#[allow(clippy::needless_return)]
\`\`\`

| Part | Checked |
|---|---|
| Errors are one enum | Works |
| Exit codes | Not checked |
`

/** A note beside each part, as the skills page puts them. */
const NOTES: Record<string, { tone: Tone; words: string }> = {
  'Errors are one enum, with a message for each': { tone: 'skill', words: 'Works' },
  'Bad arguments exit 1, the tool’s own failures exit 2': { tone: 'done', words: 'Not checked' },
  'Fix clippy warnings, never quiet them': { tone: 'check', words: 'Checking now' },
}

const MOVES: Move[] = ['idle', 'work', 'happy', 'wave', 'sleep']
const TONES: Tone[] = ['debrief', 'check', 'skill', 'score', 'part', 'done']

export function Kit() {
  const as = new URLSearchParams(location.hash.split('?')[1] ?? '').get('as')
  const [dialog, setDialog] = useState(as === 'dialog')
  const [debriefs, setDebriefs] = useState<'all' | 'worth' | 'never'>('worth')
  const [folder, setFolder] = useState(true)
  const [name, setName] = useState('')
  return (
    <div className="app">
      <Header example />
      <main className="app-main kit">
        <h1>Controls</h1>

        <section aria-labelledby="kit-buttons">
          <h2 id="kit-buttons">Buttons</h2>
          <div className="kit-row">
            <Btn kind="primary">Send to your agent</Btn>
            <Btn>Change how it looks</Btn>
            <Btn kind="quiet">Not now</Btn>
            <Btn disabled>Can’t yet</Btn>
          </div>
          <div className="kit-row">
            <Btn kind="primary" size="small">Agree</Btn>
            <Btn size="small">Copy</Btn>
            <Btn kind="quiet" size="small">Skip for now</Btn>
            <Btn size="small" pressed>Rust</Btn>
            <Btn icon label="Change how it looks">{PENCIL}</Btn>
          </div>
          <div className="kit-row">
            <Btn kind="primary" size="large">Start</Btn>
            <Btn size="large">Look into it</Btn>
            <TextLink href="#/example/kit">See every skill</TextLink>
          </div>
        </section>

        <section aria-labelledby="kit-choices">
          <h2 id="kit-choices">Choices</h2>
          <div className="kit-row">
            <Segmented label="Debriefs" value={debriefs} onChange={setDebriefs} options={[{ value: 'all', label: 'Every time' }, { value: 'worth', label: 'When it’s worth it' }, { value: 'never', label: 'Never' }]} />
            <Segmented small label="Show the skill as" value="rendered" onChange={() => {}} options={[{ value: 'rendered', label: 'Rendered' }, { value: 'markdown', label: 'Markdown' }]} />
          </div>
          <div className="kit-row">
            <Toggle on={folder} onChange={setFolder} label="Put your skills in each agent’s skills folder" />
            <span>Put your skills in each agent’s skills folder</span>
          </div>
          <div className="kit-field">
            <TextField label="Give it a name, if you like" value={name} onChange={setName} placeholder="Sam’s agent" maxLength={24} hint="Shown on your page, if you publish it." />
          </div>
          <div className="kit-field">
            <CopyLine text="claude mcp add --scope user kelve -- kelve mcp" />
            <CopyLine words text="Open kelve." />
          </div>
        </section>

        <section aria-labelledby="kit-small">
          <h2 id="kit-small">Small things</h2>
          <div className="kit-row">
            {TONES.map((t) => <Chip key={t} tone={t}>{t[0].toUpperCase() + t.slice(1)}</Chip>)}
            <Chip>Not checked</Chip>
          </div>
          <div className="kit-row">
            <Spinner>Your agent is writing the debrief</Spinner>
            <span>Press <Kbd>Esc</Kbd> to close</span>
          </div>
          <div className="kit-row kit-squares" role="img" aria-label="Better on 2 tasks, the same on 1, worse on 1">
            <i style={{ background: 'var(--better)' }} />
            <i style={{ background: 'var(--better)' }} />
            <i style={{ background: 'var(--same)' }} />
            <i style={{ background: 'var(--worse)' }} />
          </div>
        </section>

        <section aria-labelledby="kit-character">
          <h2 id="kit-character">Your agent</h2>
          <div className="kit-row kit-chars">
            {MOVES.map((m) => (
              <figure key={m}>
                <Character size={96} move={m} label={`Your agent, ${m}`} />
                <figcaption>{m}</figcaption>
              </figure>
            ))}
          </div>
        </section>

        <section aria-labelledby="kit-markdown">
          <h2 id="kit-markdown">Markdown, with notes beside it</h2>
          <Markdown
            text={SKILL}
            label="A skill"
            margin={(b) => {
              const note = b.heading && NOTES[b.heading.text]
              return note ? <Chip tone={note.tone}>{note.words}</Chip> : null
            }}
          />
        </section>

        <div className="kit-row">
          <Btn onClick={() => setDialog(true)}>Open the dialog</Btn>
        </div>
      </main>
      {dialog && (
        <Dialog
          title="Send to your agent?"
          onClose={() => setDialog(false)}
          actions={
            <>
              <Btn kind="quiet" onClick={() => setDialog(false)}>Cancel</Btn>
              <Btn kind="primary" onClick={() => setDialog(false)}>Send to your agent</Btn>
            </>
          }
        >
          <p>It redoes one decision, and two parts go into your Rust skill.</p>
        </Dialog>
      )}
      {as === 'toast' && <Toast action={{ label: 'Undo', run: () => {} }}>Sent to your agent.</Toast>}
    </div>
  )
}
