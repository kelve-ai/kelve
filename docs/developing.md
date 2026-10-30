# Developing kelve

## Start here

In one terminal, from the repo root:

```bash
bun dev/kelve.ts up
```

Open the link it prints. Then, in another:

```bash
bun dev/kelve.ts agent
```

That's kelve, with an example project, a shared graph and an agent doing its work, all your own and none of them real. Change the web app and the page reloads; start `up` with `--watch` and kelve rebuilds when its Rust changes. Before you say you're done, run the checks in [AGENTS.md](../AGENTS.md).

## Why it's this way

kelve does nothing on its own. It needs something opened, a shared graph to read and add to, and the person's coding agent to do its work. The sandbox gives it a stand-in for each: a small payments service to open (`dev/example`), a graph that speaks the real graph's API, and an agent that answers kelve's jobs from what it was given to know. So you never touch the kelve you use, the real graph, or a model you pay for, and it runs the same on any computer. The same example is the app's first run, every screen in the states gallery and every test, so what you see in the sandbox is what the tests check.

## What you need

- Rust (stable) and [Bun](https://bun.sh).
- For the browser tests, Chromium and WebKit, once: `cd web && bunx playwright install chromium webkit`.
- For the screenshot tests, Docker.

## The sandbox

```bash
bun dev/kelve.ts up
```

It builds what's changed, then starts all of it, on ports of its own, so a kelve you use is never touched:

- **kelve** with a fresh store in `.sandbox/`, on 4698;
- **the stand-in shared graph** on 4697, holding the example's ideas and entries (`dev/example/graph`);
- **the web app** with hot reload on 4699, handing `/api` to kelve;
- **the example project**, `.sandbox/ledger`: a small payments service, as a git repository, whose last commit is the change "Retries that back off with jitter".

It prints a link that signs your browser in. Leave it running; Ctrl C stops it all. `--keep` keeps the store from last time; `--watch` rebuilds and restarts kelve when its Rust changes. Logs are in `.sandbox/`. `KELVE_DEV_PORT` moves kelve, with the graph on the port below it and the web app on the port above.

Then, in another terminal, have the example's agent show you its work:

```bash
bun dev/kelve.ts agent
```

It opens the change in kelve, as a person's agent would, and does every job kelve keeps for it: reading the files, tracing ideas down, writing the entries the graph doesn't have. Its marks, relations and entries are real, from `dev/example/agent/`, and every entry quotes its pages exactly. `--live` prints each call. Open an idea the graph has no entry for, then run it again, and it writes one while you watch.

## Being the agent

The best way to try a change to anything an agent touches is to be the agent yourself: open something, take each job, do it, hand it in, and watch the reader. One MCP call at a time, exactly as an agent makes them:

```bash
bun dev/kelve.ts call open '{"what":"change","path":"'$PWD'/.sandbox/ledger"}'
```

```bash
bun dev/kelve.ts call next_job
```

Do the job as its instructions say, write the answer in the shape it gives, and hand it in, where `answer.json` is `{"job": 1, "answer": {…}}`:

```bash
bun dev/kelve.ts call hand_in @answer.json
```

Each answer taken comes back with the next job. `open` says the number of what it opened; give it as `for` (`call next_job '{"for":1}'`) to take only that one's jobs, as an agent in one of several tabs does. `bun dev/kelve.ts tools` lists the tools as an agent sees them. Or give your own coding agent the sandbox's tools: `bun dev/kelve.ts mcp` prints the line that adds them, for Claude Code `claude mcp add kelve-sandbox …`. An agent working on kelve itself can do the same, to try what it changed.

- **Entries are checked against real pages.** The real graph fetches each page an entry quotes; the stand-in is given them. The example's pages are given as it starts. For an entry of your own, give it each page you cite: `bun dev/kelve.ts graph page <url>`. A quote not on its page fails, as it would on the real graph.
- **Watching the check.** `bun dev/kelve.ts graph hold` keeps new entries' checks pending, to see an entry while it's checked; `graph release` finishes them. `graph grade` has it grade entries, as the real graph does, asking for what each lacks, so the agent extends them; `graph ungrade` stops it. `graph unagree` forgets that the sandbox's person agreed to the terms, to see agreeing again.
- **Notes for the agent.** Leave a note for your agent in the reader, then read it as the agent would, from the example project: `bun dev/kelve.ts call notes`, and say it's done with `bun dev/kelve.ts call note_done '{"note":"agent/…","version":"…","did":"…"}'`. The sandbox has a Claude Code folder of its own (`.sandbox/claude`) and a home of its own for other agents' settings (`.sandbox/home`), so turning on kelve's hook never touches yours.
- `bun dev/kelve.ts link` signs a browser in again.

## Lessons, alone

Lessons need other people's agents, so the sandbox brings them: `bun dev/kelve.ts agent` puts what other agents learned using the example's ideas into the stand-in graph, then works on the change as an agent with kelve's hook would. It reads and edits files, runs the tests, fails and passes, and meets lessons as it goes; as its turn ends, kelve asks what it learned. Open the change in the reader to see which it was given, which held, and what it learned, waiting to leave. Or be the agent yourself, one of Claude Code's hook events at a time, a turn's end too:

```bash
bun dev/kelve.ts moment '{"hook_event_name":"PostToolUse","tool_name":"Read","tool_input":{"file_path":"src/retry.rs"}}'
bun dev/kelve.ts moment '{"hook_event_name":"Stop"}'
```

They're all one session of the agent's; `SANDBOX_SESSION` names another.

Against a graph of your own instead (`kelve-graph serve`, with its own database), a lesson goes live only once the graph has checked it, which costs a model call, and confirming it takes two projects besides its author's. Two kelves with stores of their own, signed in as two accounts, on two copies of a project, make the second project.

What the agent learns waits a day before it leaves, so the person can stop it. `KELVE_LESSON_HOLD`, in seconds, makes that shorter for trying lessons out (`KELVE_LESSON_HOLD=30`); what came of lessons given then goes to the graph as often, rather than once an hour.

## Someone new

```bash
bun dev/kelve.ts newcomer
```

To try kelve's onboarding as someone new meets it: they ask their agent to install kelve, it does and sets it up, and the first thing they open shows kelve's first screen. This serves the installer (`install.sh`) and this kelve, packed as a release, on 4692, beside a stand-in graph they have no account on yet. Then it prints the line that makes another terminal theirs: a kelve on 4690 with a store of its own, and a Claude Code of their own (`.newcomer-claude`), so yours are never touched. Log that Claude Code in once, the first time; it stays logged in, and kelve's connection and hook in it start again each run. In it, ask: "Install kelve from http://127.0.0.1:4692/install.sh". Then start it again, so it has kelve's tools, and ask it to show you something.

`--agent` has that Claude Code install kelve unattended, then checks what it set up: kelve installed, connected, its hook, and how much kelve marks.

## The example

`dev/example/` is one small project, used everywhere: by the sandbox, the end-to-end tests and the app's own first run.

- `base/` and `change/`: the ledger as it was, and the files its change touched. `change.json` says the change's commit message, and the title and line its agent gives.
- `agent/`: what the example's agent knows. `marks.json`, what it marks in each file and how the file uses each idea; `relations.json`, how each idea relates, down to first principles; `entries.json`, the entries it writes, from real pages; `way.json`, the way in it gives with its change; `lessons.json`, what other agents learned using its ideas, the story its agent meets as it works and what it learns; `plainly.json`, the change's files, its top folder, `src/` and the change itself read plainly (the rest it reads a part for each run of lines).
- `agent.ts`: the example's agent, answering kelve's jobs from those.
- `graph/`: the example's graph, made by `bun dev/kelve.ts record`, which has the agent read the whole ledger, trace every idea it marks and write every entry, then keeps what the stand-in graph was given. Run it again after changing `agent/`. Entries marked `live` are left out, for the agent to write while you watch.
- `make.ts`: makes the ledger as a git repository, for the sandbox and the tests.
- `web/src/example/example.json`: the app's own example, the first run's, every answer the reader asks kelve for, recorded by `bun dev/kelve.ts example` from a fresh sandbox. Run it again after changing the example or what the reader asks for.

## Every state, and how it looks

With the sandbox up, `#/states` shows every state of every screen, drawn from the example with no server: light and dark side by side, at a phone's width on a toggle, with every string each shows listed under it. It's how a change to how anything looks, or to its words, is reviewed.

The same states are screenshot and compared with those committed in `web/e2e/looks/screenshots`. Fonts draw differently on each system, so they're taken in Playwright's own Linux image, here as in CI, which needs Docker:

```bash
cd web && bun run build && bun run looks
```

After a change meant to show, take them again with `bun run looks --update-snapshots`, look at what changed, and commit them.

## How fast it is

Opening a file should feel instant. To see how long kelve takes over what the reader asks for, on a change as large as you like:

```bash
cargo build --release -p kelve && KELVE_BIN=target/release/kelve bun dev/speed.ts --change="against HEAD~25"
```

It starts a kelve of its own, with a fresh store, opens the change, and times each file's diff and plainly the first time and again, the change itself, and the side views. `--dir` times another repository, and `--files` how many of its files (30). Nothing is read by an agent: the times are kelve's own. Measure a release build, as people run: a debug build is many times slower.

## Using kelve from source

To use a kelve built from this checkout day to day, install it:

```bash
cd web && bun install && bun run build && cd .. && cargo install --path crates/cli --locked
```

then `kelve stop`, so the next `kelve open` starts the new one. Without `KELVE_GRAPH_URL`, a kelve built from a checkout reads the real graph (`https://graph.kelve.ai`) once you sign in to it, but never adds to it; `KELVE_WRITE_REAL_GRAPH=1` lets it add. A release build adds to it as people's kelves do.

To use another graph, such as one you run yourself, stop kelve and start it with that graph's address:

```bash
kelve stop && KELVE_GRAPH_URL=https://graph.example.com kelve open .
```

Each graph has its own store in kelve's data folder, so its own sign-in and what kelve kept from it: sign in once on each, and going back to one finds it as it was.

## Checks

Everything CI runs, from the repo root, is in [AGENTS.md](../AGENTS.md).
