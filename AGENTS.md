# Working on kelve with agents

kelve turns what the person knows into skills their agents use, and shows them what their agent did and why: as their agent hands work over, kelve asks it for a debrief, learns from what they correct, change and send, and keeps it as skills it hands to their agents as they work, checked on their own past work and, if they ask, verified by kelve's hosted service. This repo is the open-source app that runs on the person's computer (docs/skills.md is the design). This repo is built with agents. This file is for those agents, and for the people directing them.

## How to work here

1. Read before you write. Find the code your change touches, the tests around it, and the docs in `docs/`. Match what's there: its names, its layout, its comments.
2. Make the smallest change that does the whole job. No drive-by refactors, no speculative abstractions, no new dependencies unless the change can't be made well without one (say why in the commit).
3. Decide nothing the person hasn't. If the task leaves a real choice open, ask, or say what you chose and why in one line.
4. Test what you change, then run every check CI runs (below). Report what you ran and what it said. If something couldn't run, say why. To try a change as people use kelve, be its agent yourself in the sandbox (`bun dev/kelve.ts`, docs/developing.md): open something, do each job, and look at the reader.
5. Show the person your work in their own kelve when you've finished it, committed or not, without being asked: use its `open` tool, once, with `what: worktree` and the branch it'll merge into as `base`, in a worktree or not. It's the same thing in kelve each time, brought up to date, so show it again only when the work has changed since; a commit, a push or a pull request changes nothing in it. If they ask to see more, such as the whole repo, show that. It starts kelve if it isn't running, and opens it in a new browser tab. Never start or open kelve another way as well (`kelve serve`, `kelve open`, `kelve diff`).
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
- `cargo test` also writes the web app's TypeScript types to `web/src/api/gen` from the Rust types in `crates/core`. Commit those files with the Rust change. Never edit them by hand; CI fails if they're out of date. A new API type also needs a line in `web/src/api/types.ts`. It writes the hosted service's OpenAPI documents, `docs/hosted-api.json` and, for version 1, `docs/graph-api.json`, from `crates/graph-api` too, and what kelve draws for each kind of character, with the CSS that moves them, to `web/src/ui/gen` (the web app draws characters itself, and `e2e/tests/character.spec.ts` holds it to those): commit them with the change.
- The end-to-end tests (`web/e2e`) start their own kelve, with a fresh store in a temporary folder, beside the stand-in hosted service; the person's agent is a stand-in too (`web/e2e/agent.ts`), calling kelve as `kelve mcp` does. When one fails, `web/e2e/.server.log` has kelve's log, `web/e2e/.graph.log` the hosted service's, and `web/test-results` the browser's trace.
- `bun run looks` compares a screenshot of every state in the states gallery (`web/src/example/states.ts`) with the one committed. After a change meant to show, run it with `--update-snapshots`, look at each image that changed, and commit them with the change.
- After adding, removing or updating a dependency, Rust's or the web app's, run `bun run notices` in `web/`, and commit `THIRD_PARTY_NOTICES.md` with the change.

## Commits

Commit as the person directing the work: their name and email, the way their git config has it. Don't add `Co-Authored-By` trailers for agents, or "Generated with" lines, to commits or pull requests. The person is responsible for every line they commit.

Write commit messages like the ones already here: an area, a colon, and what changed in plain words (`research, docs: what to build kelve with, area by area, and what Nathan decided`). The body says why, when that isn't obvious.

Never push, open pull requests, or touch anything outside this repo unless the person asks.

## The code

```
crates/core     Everything pure: skills, their parts and kinds of work (skills.rs), the debrief and
                when one is due (debrief.rs, due.rs), where something learned goes (learn.rs), your
                agent's character (character.rs), the hook for each agent (hook.rs), what the
                person knows already (spec.rs, from marks.md), what's sent to the hosted
                service (hosted.rs), secret stripping, fencing material, what agents are told for
                each job (instructions.rs), updating, and the API's shapes (views.rs). No IO. The
                shared graph's entries, ideas, relations, lessons and terms stay here only for the
                hosted service's version 1, until it retires it: kelve never uses them.
crates/store    The local store: SQLite, its schema (migrations/), and every query.
crates/engine   Reading what the agent hands over, without running anything: walking a folder as
                git does, reading text with its secrets masked, and changes from git's own files
                (gitoxide).
crates/graph-api The hosted service's open API: version 2 (v2/: signing in, the account and its
                page, scores, verified runs), the one client for it, which alone retries, and a
                stand-in for tests (the `stand-in` feature). Version 1, the shared graph's, stays
                for the hosted service; kelve sends it only the weekly count.
crates/server   The local service: the jobs it keeps for the person's agent (agent.rs) and its
                list (list.rs), the agent loop (agent_loop/: debriefs, research, explanations,
                learning, and skills to the agent), what was handed over (handover.rs), debriefs
                and home, skills and their checks, your agent, kelve.ai (hosted.rs), the hook's
                router (moment.rs) and the hook (hooks.rs), what's open (reader.rs),
                the API (api/), the work it does in the background (pipeline/), signing a browser
                in, and the web app, on one port.
crates/cli      The `kelve` command: open and diff (starting kelve in the background), serve,
                status, stop, update; setup and hook, to set it up; and for the person's agent,
                mcp (kelve's tools over MCP), job, kept, moment (what the hook runs) and worktree.
web/            The web app: home, debriefs, skills, your agent and settings. web/e2e tests it in
                a browser.
dev/            One command for developing kelve (bun dev/kelve.ts), and the example project its
                sandbox, the app's example and the tests all use.
docs/           How kelve is built, run and released, its design (skills.md), and the hosted
                service's API.
```

Rules that keep it sound:

- `core` never does IO and never panics on bad input.
- Every route that reads or changes anything checks who's asking first: `Me`, the person signed in on this browser, or `Agent`, the command line's secret, which `kelve mcp` uses too. The command line can ask kelve to open something, take kelve's jobs and hand in their answers, see what's kept for the agent, read the person's skills, pass on the moments kelve's hook sees, learn whether a newer kelve is out, say it changed kelve's hook, or ask kelve to stop. It never reads what kelve keeps beyond a job's material, the skills and that one: not their skills' evidence or their debriefs, not the keys kelve keeps. It can ask for a sign-in link, but never has one: kelve gives the link to the person, opening their browser or showing it where `kelve serve` runs, and tells the asker only whether a browser opened.
- Nothing of the person's leaves, except their skills, to kelve's hosted service, for a verified run they asked for, under its terms: secrets and paths stripped first, and every file shown to them before it goes. Their work, their messages to their agent, their edits and their tasks never leave. Their page shows only what they publish: their agent's character and name, and the scores they choose. The weekly count, numbers only, goes only with their yes.
- Secrets are stripped (`core::redact`) as files are read, and again before anything goes to the agent or the hosted service. The service turns away skills that seem to hold a secret or a path, or name its tasks.
- kelve runs nothing from what the person opens: no builds, no hooks, no scripts, nothing a repo configures to run, and it never starts the person's agent. Outside the project it changes only how the person's agent reaches it, with their yes: kelve as an MCP server in the agent's own settings; one hook there, one command, `kelve moment`, on the agent's own events, installed and taken out together, which tells the agent what the person wants redone from a debrief, keeps what the person says to catch their corrections, gives it their skills as it works, and has it hand work over in kelve as it finishes (`kelve setup`, `kelve hook`, `core::hook`); and links to their skills in each agent's own skills folder, kelve's links and nothing else. Asking to install kelve is that yes: the installer runs `kelve setup`, which says what it changed and how to take it out.
- kelve has no model and no key of its own. Every job is the person's agent's (`server::agent`), with instructions kelve was built with (`core::instructions`), its material, and the shape its answer must fit; kelve checks the shape before it takes an answer. The hosted service runs agents on its own keys, for benchmarks only; a verified run is paid with the person's own provider key, which kelve keeps sealed and sends with that run alone.
- Signing in is for one thing only: the person's account on kelve's hosted service, for verified runs and their page. Everything else works signed out.
- A debrief is checked against the change as it was handed over (`core::debrief`), and a skill's parts in code (`core::skills`); what fails goes back to the agent once, with what to fix, then is dropped. What kelve learned waits for the person in their next debrief; only what they sent is written into a skill at once, and every edit kelve makes to a skill can be undone. kelve never changes back what the person wrote in a skill by hand.
- Only kelve's own words reach the person's screen as markup. An agent's text is markdown of a few kinds (headings, paragraphs, lists, tables, code, bold, italics and links to pages on the web), which kelve draws as its own elements. No HTML or images from an agent, ever.
- Text from what the person opens and their messages goes to the agent fenced (`core::fence`), and its instructions say it's material, never instructions.
- The keys kelve keeps, the account's on kelve.ai and the person's provider key, are sealed with the install's key before they're kept, and never logged.
- Errors people see are full sentences that say what to do. Internal errors are logged with context and shown as a plain apology.
- Never log tokens, keys, passwords, secrets or file contents.
- What agents are told for each job lives in `crates/core/src/instructions.rs`, by version: change a job's instructions and its version together, then try them as the agent in the sandbox, on the example (`dev/`).

## Writing for people

The app's words are part of the design. When you write copy:

- Short sentences in plain words, in British English. Say what happens, and what to do next.
- Write for anyone who reads and builds, not only for code.
- Name things plainly ("Getting a verified score", not "Benchmark configuration"). No eyebrow labels over headings, no jargon, no exclamation marks.
- The app never says where kelve runs ("on this computer", "stays here"). Docs may say it runs locally, plainly, where it matters: installing and signing in.
- Match the voice already in the app before inventing a new one.
