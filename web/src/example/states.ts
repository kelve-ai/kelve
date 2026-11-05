// Every state of every screen, drawn from the example with no server: the
// states gallery shows them side by side, and the screenshot tests compare each
// with how it looked.

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

export function states(): State[] {
  // A file of the example's change, as code.
  const file = (path: string) => `f=${encodeURIComponent(path)}`
  return [
    { name: 'reader-change', title: 'A change, open on what changed in a code file', path: `#/example?${file('src/retry.rs')}`, phone: true },
    { name: 'reader-change-opened', title: 'What changed, with lines as before opened in place', path: `#/example?${file('src/retry.rs')}`, clicks: ['12 lines'] },
    { name: 'reader-text', title: 'Text read as text', path: `#/example?${file('docs/retries.md')}` },
    { name: 'reader-every-file', title: 'Every file, beside the change', path: `#/example?${file('src/retry.rs')}&w=all` },
    { name: 'reader-files', title: 'The files, in folders', path: `#/example?${file('src/retry.rs')}&t=files` },
    { name: 'reader-list', title: 'Your agent’s list, beside the work', path: `#/example?${file('src/retry.rs')}&k=list` },
    { name: 'reader-list-idle', title: 'Your agent’s list, nothing taking it: what to say to it, at the top', path: `#/example?${file('src/retry.rs')}&k=list&as=next`, phone: true },
    { name: 'reader-list-take-off', title: 'Your agent’s list, a row pointed at: Do next and Take off', path: `#/example?${file('src/retry.rs')}&k=list&as=next`, points: 'Name the backoff' },
    { name: 'moved', title: 'A tab whose thing kelve showed again in a newer one', path: '#/example/moved' },
    // kelve's screens under its header (Shell).
    { name: 'home', title: 'Home: your agent, what needs you, and what happened lately', path: '#/example/home', phone: true },
    { name: 'home-working', title: 'Home: your agent at work, with a clock, and what it does next', path: '#/example/home?as=working', phone: true },
    { name: 'home-empty', title: 'Home: someone new, the example to try', path: '#/example/home?as=empty' },
    { name: 'home-attention', title: 'Home: kelve’s hook gone, and a debrief your agent didn’t finish', path: '#/example/home?as=attention' },
    { name: 'debrief', title: 'A debrief: research ready on one decision, agreed on two, one left', path: '#/example/debrief/1' },
    { name: 'debrief-small', title: 'A debrief of one line you changed', path: '#/example/debrief/2', phone: true },
    { name: 'debrief-large', title: 'A debrief of 86 files: decisions by the part they belong to, folders closed', path: '#/example/debrief/3' },
    { name: 'debrief-change-it', title: 'A debrief: changing a decision, saying how, and its code', path: '#/example/debrief/1', clicks: ['Show the code', 'Change it'] },
    { name: 'debrief-look', title: 'A debrief: the question for your agent, before it looks into a decision', path: '#/example/debrief/2', clicks: ['Look into it'] },
    { name: 'debrief-explain', title: 'A debrief: an idea explained', path: '#/example/debrief/1', clicks: ['Explain'] },
    { name: 'debrief-writing', title: 'A debrief, your agent writing it', path: '#/example/debrief/1?as=writing' },
    { name: 'debrief-queued', title: 'A debrief, waiting while your agent is busy', path: '#/example/debrief/1?as=queued' },
    { name: 'debrief-failed', title: 'A debrief your agent couldn’t write', path: '#/example/debrief/1?as=failed' },
    { name: 'debrief-sent', title: 'Sent to your agent: what happens next', path: '#/example/debrief/1', clicks: ['Send to your agent'], phone: true },
    { name: 'debrief-sent-read', title: 'A debrief sent, read again', path: '#/example/debrief/1?as=sent' },
    { name: 'change', title: 'A change, file by file, each decision beside its lines', path: '#/example/change/1', phone: true },
    { name: 'change-yours', title: 'A change: the line you changed after handover', path: '#/example/change/1?file=src%2Fargs.rs' },
    { name: 'skills', title: 'Skills: the list, open on Rust, a part of every state (Skills)', path: '#/example/skills', phone: true },
    { name: 'skills-empty', title: 'Skills: none yet (SkillsEmpty)', path: '#/example/skills?as=empty', phone: true },
    { name: 'skill', title: 'A skill, rendered, with a note beside each part (Skills)', path: '#/example/skills/rust' },
    { name: 'skill-markdown', title: 'A skill as its SKILL.md, a mark beside each ### line (Skills)', path: '#/example/skills/rust', clicks: ['Markdown'], phone: true },
    { name: 'skill-project', title: 'A project’s skill', path: '#/example/skills/12' },
    { name: 'skill-suggested', title: 'A suggested skill, made only if you say so (SkillSuggested)', path: '#/example/skills/suggested/41', phone: true },
    { name: 'skill-check-confirm', title: 'Checking three new parts: what it takes, before it starts (CheckConfirm)', path: '#/example/skills/rust?as=confirm&check', phone: true },
    { name: 'skill-check-running', title: 'Three new parts being checked (CheckRunning)', path: '#/example/skills/rust?as=running' },
    { name: 'skill-edit', title: 'Editing a skill, with its preview (SkillEdit)', path: '#/example/skills/rust/edit', phone: true },
    { name: 'skill-new', title: 'Writing a new skill (SkillEdit)', path: '#/example/skills/new' },
    { name: 'fits', title: 'Fits you: two results of a past task, which is which hidden', path: '#/example/agent/fits', phone: true },
    { name: 'fits-start', title: 'Fits you, before it starts', path: '#/example/agent/fits?as=fits-start' },
    { name: 'fits-waiting', title: 'Fits you, while your agent redoes past work', path: '#/example/agent/fits?as=fits-waiting' },
    { name: 'agent', title: 'Your agent: its character and name, what it knows, Fits you, and a verified score to get', path: '#/example/agent', phone: true },
    { name: 'agent-no-page', title: 'Your agent, signed out of kelve.ai: get a verified score', path: '#/example/agent?as=signed-out' },
    { name: 'agent-sign-in', title: 'Signing in to kelve.ai, over Your agent', path: '#/example/agent?as=sign-in' },
    { name: 'agent-address', title: 'Choosing your page’s address: one you can have', path: '#/example/agent/address' },
    { name: 'agent-address-taken', title: 'Choosing your page’s address: someone has it, others offered', path: '#/example/agent/address?as=taken', phone: true },
    { name: 'agent-look', title: 'Changing how it looks: the characters, a drawing asked for, and a part your agent drew', path: '#/example/agent/look', phone: true },
    { name: 'agent-look-hat', title: 'Changing how it looks: the robot’s hats, with the one your agent drew', path: '#/example/agent/look', clicks: ['Hat'] },
    { name: 'agent-look-colours', title: 'Changing how it looks: its colours, and the background', path: '#/example/agent/look', clicks: ['Colours'] },
    { name: 'score-new', title: 'Your agent: getting a verified score', path: '#/example/agent/score/new' },
    { name: 'score-new-files', title: 'Getting a verified score: exactly what goes', path: '#/example/agent/score/new', clicks: ['See exactly what goes'] },
    { name: 'score-new-no-runs', title: 'Getting a verified score: today’s runs used', path: '#/example/agent/score/new?as=no-runs' },
    { name: 'score-confirm', title: 'Getting a verified score: what it costs on your key, kept', path: '#/example/agent/score/new?as=confirm', phone: true },
    { name: 'score-confirm-subscription', title: 'Getting a verified score from a service run locally: your Claude subscription or an API key', path: '#/example/agent/score/new?as=subscription', clicks: ['Start the run'] },
    { name: 'score-running', title: 'A verified run going, task by task', path: '#/example/agent/score/2', phone: true },
    { name: 'score-paused', title: 'A verified run paused for the Claude plan’s limit', path: '#/example/agent/score/4' },
    { name: 'score-published', title: 'A verified score, published', path: '#/example/agent/score/1', clicks: ['Publish to your page'] },
    { name: 'score-unclear', title: 'A score with no clear difference', path: '#/example/agent/score/3' },
    { name: 'score', title: 'Your agent: a verified score', path: '#/example/agent/score/1' },
    { name: 'settings-agent', title: 'Settings: your agent, the agents connected, debriefs and the skills folder', path: '#/example/settings/agent', phone: true },
    { name: 'settings-account', title: 'Settings: account, signed in, with a page and a key kept', path: '#/example/settings/account' },
    { name: 'settings-account-out', title: 'Settings: account, signed out', path: '#/example/settings/account?as=signed-out', phone: true },
    { name: 'settings-account-waiting', title: 'Settings: account, finishing signing in in the browser', path: '#/example/settings/account?as=signing-in' },
    { name: 'settings-account-delete', title: 'Settings: account, deleting it asked once more', path: '#/example/settings/account', clicks: ['Delete'] },
    { name: 'settings-checks', title: 'Settings: skill checks, when they run and how many a week', path: '#/example/settings/checks' },
    { name: 'settings-reading', title: 'Settings: reading, how code and text look, and the ideas you know', path: '#/example/settings/reading' },
    { name: 'settings-privacy', title: 'Settings: privacy, what leaves, counts and deleting everything', path: '#/example/settings/privacy' },
    { name: 'settings-about', title: 'Settings: about, this kelve and where its docs are', path: '#/example/settings/about' },
    { name: 'welcome', title: 'Meet your agent, the first time: three looks and a name', path: '#/example/welcome', phone: true },
    // Every control the screens are built from.
    { name: 'kit', title: 'Controls: buttons, choices, fields, chips, the character and markdown with notes beside it', path: '#/example/kit', phone: true },
    { name: 'kit-dialog', title: 'Controls: a dialog over the page', path: '#/example/kit?as=dialog' },
    { name: 'kit-toast', title: 'Controls: a toast', path: '#/example/kit?as=toast' },
  ]
}
