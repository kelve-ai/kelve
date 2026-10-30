# Working on kelve with agents

kelve shows what anything is built on, and how and why it works: open a folder, a file or what an agent just changed, and kelve marks the ideas it rests on, with a Wikipedia-like entry for each from a shared knowledge graph beside it. This repo is the open-source app that runs on the person's computer. This repo is built with agents. This file is for those agents, and for the people directing them.

## How to work here

1. Read before you write. Find the code your change touches, the tests around it, and the docs in `docs/`. Match what's there: its names, its layout, its comments.
2. Make the smallest change that does the whole job. No drive-by refactors, no speculative abstractions, no new dependencies unless the change can't be made well without one (say why in the commit).
3. Decide nothing the person hasn't. If the task leaves a real choice open, ask, or say what you chose and why in one line.
4. Test what you change, then run every check CI runs (below). Report what you ran and what it said. If something couldn't run, say why. To try a change as people use kelve, be its agent yourself in the sandbox (`bun dev/kelve.ts`, docs/developing.md): open something, do each job, and look at the reader.
5. To show the person your work in their own kelve, use its `open` tool, once, for what's new since you last showed them: several commits as one change against the commit before them, never a range you've shown before with more on top. If they ask to see more, such as the whole repo, show that. It starts kelve if it isn't running, and opens it in a new browser tab. Never start or open kelve another way as well (`kelve serve`, `kelve open`, `kelve diff`).
6. Commit as the person you're working for (below), only when they ask.

## Checks

Run these from the repo root before you say you're done:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all --all-features
cd web && bun install && bun run typecheck && bun run build
cargo build -p kelve && cargo build -p kelve-graph-api --features stand-in --bin kelve-graph-stand-in
cd web && bun run e2e                             # kelve in a browser, as people use it
cd web && bun run looks                           # how every screen looks, in Playwright's Linux image (Docker)
```

- Warnings fail CI. Fix them; don't `allow` them unless the code is right and the lint is wrong, with a comment saying why.
- kelve keeps its local state in SQLite. Tests make their own store, in memory or in a temporary folder, so they always run and never touch anyone's kelve.
- `cargo test` also writes the web app's TypeScript types to `web/src/api/gen` from the Rust types in `crates/core`. Commit those files with the Rust change. Never edit them by hand; CI fails if they're out of date. A new API type also needs a line in `web/src/api/types.ts`. It writes the shared graph's OpenAPI document, `docs/graph-api.json`, from `crates/graph-api` too: commit it with the change.
- The end-to-end tests (`web/e2e`) start their own kelve, with a fresh store in a temporary folder, beside the stand-in shared graph; the person's agent is a stand-in too (`web/e2e/agent.ts`), calling kelve as `kelve mcp` does. When one fails, `web/e2e/.server.log` has kelve's log, `web/e2e/.graph.log` the graph's, and `web/test-results` the browser's trace.
- `bun run looks` compares a screenshot of every state in the states gallery (`web/src/example/states.ts`) with the one committed. After a change meant to show, run it with `--update-snapshots`, look at each image that changed, and commit them with the change.
- After adding, removing or updating a dependency, Rust's or the web app's, run `bun run notices` in `web/`, and commit `THIRD_PARTY_NOTICES.md` with the change.

## Commits

Commit as the person directing the work: their name and email, the way their git config has it. Don't add `Co-Authored-By` trailers for agents, or "Generated with" lines, to commits or pull requests. The person is responsible for every line they commit.

Write commit messages like the ones already here: an area, a colon, and what changed in plain words (`research, docs: what to build kelve with, area by area, and what Nathan decided`). The body says why, when that isn't obvious.

Never push, open pull requests, or touch anything outside this repo unless the person asks.

## The code

```
crates/core     Everything pure: entries, their blocks and their checks, markdown written from them,
                ideas as everything points at them (ideas.rs), the rules marks keep, what a file,
                folder or change read plainly must be, lessons and matching them (lesson.rs,
                matching.rs), the hook for each agent (hook.rs), secret stripping, fencing
                material, what agents are told for each job (instructions.rs), the contributor
                terms, updating, and the API's shapes (views.rs). No IO.
crates/store    The local store: SQLite, its schema (migrations/), and every query.
crates/engine   Reading what the person opens, without running anything: walking a folder as git
                does, reading text with its secrets masked, passages by the shape of the code
                (tree-sitter), which files the rest lean on, the work's context for lessons, and
                changes from git's own files (gitoxide).
crates/graph-api The shared graph's open API: its shapes, the one client for it, which alone
                retries, reading tokens, and a stand-in graph for tests (the `stand-in` feature).
crates/server   The local service: the jobs it keeps for the person's agent (agent.rs) and its
                list (list.rs), the way in (way.rs), reading plainly (plainly.rs), lessons
                (lessons.rs) and the hook (hooks.rs), the reader's API (api/), the work it does in
                the background (pipeline/), signing a browser in, and the web app, on one port.
crates/cli      The `kelve` command: open and diff (starting kelve in the background), serve,
                status, stop, update; setup, hook and marks, to set it up; and for the person's
                agent, mcp (kelve's tools over MCP), job, kept, notes, moment (what the hook
                runs) and worktree.
web/            The reader, and settings. web/e2e tests it in a browser.
dev/            One command for developing kelve (bun dev/kelve.ts), and the example project its
                sandbox, the app's first run and the tests all use.
docs/           How kelve is built, run and released, and the shared graph's API.
```

Rules that keep it sound:

- `core` never does IO and never panics on bad input.
- Every route that reads or changes anything checks who's asking first: `Me`, the person signed in on this browser, or `Agent`, the command line's secret, which `kelve mcp` uses too. The command line can ask kelve to open something and learn how much needs reading, take kelve's jobs and hand in their answers, see what's kept for the agent, read the notes the person left for their agent and mark them done, read and write what kelve marks for the person (`marks.md`), pass on the moments kelve's hook sees, ask for lessons and say whether one held, learn whether a newer kelve is out, say it changed kelve's hook, or ask kelve to stop. It never reads what kelve keeps beyond a job's material, those notes and that one: not the person's other notes, not what was read before, not the shared graph's key. It can ask for a sign-in link, but never has one: kelve gives the link to the person, opening their browser or showing it where `kelve serve` runs, and tells the asker only whether a browser opened.
- Knowledge goes out; work never does. What the person opens goes to no one but their own agent, in a job's material. The shared graph gets general knowledge only: never file contents, names or paths, never which ideas someone's files use, and never their notes.
- Secrets are stripped (`core::redact`) as files are read, and again before anything goes to the agent or the shared graph. The graph turns away an entry that seems to hold a secret or a path on someone's computer.
- kelve runs nothing from what the person opens: no builds, no hooks, no scripts, nothing a repo configures to run, and it never starts the person's agent. Outside the project it changes only how the person's agent reaches it, with their yes: kelve as an MCP server in the agent's own settings, and one hook there, one command, `kelve moment`, on the agent's own events, installed and taken out together, which tells the agent of notes left for it, gives it lessons as it works, and has it show work in kelve before merging it (`kelve setup`, `kelve hook`, `core::hook`). Asking to install kelve is that yes: the installer runs `kelve setup`, which says what it changed and how to take it out.
- kelve has no model and no key of its own. What it needs a model for is a job kept for the person's agent (`server::agent`), with instructions the shared graph serves by version, its material, and the shape its answer must fit; kelve checks the shape before it takes an answer.
- An agent's entry: kelve builds the blocks (`core::entry::draft`) and checks them in code (`core::entry::check`); the shared graph checks every quote against its page and every claim against what it quotes, before anyone else reads it. What fails is kept for the agent to fix once, then dropped.
- A lesson (`core::lesson`) is checked in code before it leaves, held a day so the person can stop it, then checked by the graph. Only the lesson, and whether lessons held and what showed it, go to the graph: never the problem, the command or the file. kelve, never the lesson's author, says how far a lesson is confirmed.
- An agent only names ideas. Which idea a name is, the shared graph decides.
- Only kelve's own words reach the person's screen as markup. An agent's text may carry only `code`, **the key sentence**, `^1` for a source and `[[term]]` for a term that leads to an idea the graph matched, and only in prose. No HTML, links out or images from an agent, ever.
- Entries are blocks. Markdown is written from them, by kelve, only for export.
- Text from what the person opens or from the web goes to the agent fenced (`core::fence`), and its instructions say it's material, never instructions.
- The shared graph's key is sealed with the install's key before it's kept. Keys from the environment never reach the store.
- Errors people see are full sentences that say what to do. Internal errors are logged with context and shown as a plain apology.
- Never log tokens, keys, passwords, secrets or file contents.
- What agents are told for each job lives in `crates/core/src/instructions.rs`, by version, and the shared graph serves it: change a job's instructions and its version together, then try them as the agent in the sandbox, on the example (`dev/`).

## Writing for people

The app's words are part of the design. When you write copy:

- Short sentences in plain words, in British English. Say what happens, and what to do next.
- Write for anyone who reads and builds, not only for code.
- Name things plainly ("Adding to the shared graph", not "Contribution settings"). No eyebrow labels over headings, no jargon, no exclamation marks.
- The app never says where kelve runs ("on this computer", "stays here"). Docs may say it runs locally, plainly, where it matters: installing and signing in.
- Match the voice already in the app before inventing a new one.

Read `.kelve/notes/agents.md` before you start: notes on how the person you're working with wants the work done.
