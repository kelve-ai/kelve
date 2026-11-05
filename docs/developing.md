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

That's kelve, with an example project, a stand-in for kelve's hosted service and an agent doing its work, all your own and none of them real. Change the web app and the page reloads; start `up` with `--watch` and kelve rebuilds when its Rust changes. Before you say you're done, run the checks in [AGENTS.md](../AGENTS.md).

## Why it's this way

kelve does nothing on its own. It needs work handed over, the person's coding agent to do its jobs, and, for verified scores and a page, kelve's hosted service. The sandbox gives it a stand-in for each: a small payments service whose change is handed over (`dev/example`), a hosted service that speaks the real one's API, and an agent that answers kelve's jobs from what it was given to know. So you never touch the kelve you use, your own skills, kelve.ai, or a model you pay for, and it runs the same on any computer. The same example is every screen in the states gallery and every test, so what you see in the sandbox is what the tests check.

## What you need

- Rust (stable) and [Bun](https://bun.sh).
- For the browser tests, Chromium and WebKit, once: `cd web && bunx playwright install chromium webkit`.
- For the screenshot tests, Docker.

## The sandbox

```bash
bun dev/kelve.ts up
```

It builds what's changed, then starts all of it, on ports of its own, so a kelve you use is never touched:

- **kelve** with a fresh store in `.sandbox/`, on 4698, and skills of its own in `.sandbox/skills` (`KELVE_SKILLS`), never yours;
- **the stand-in hosted service** on 4697 (`kelve-graph-stand-in`);
- **the web app** with hot reload on 4699, handing `/api` to kelve;
- **the example project**, `.sandbox/ledger`: a small payments service, as a git repository, whose last commit is the change "Calls to the bank that are safe to retry".

It prints a link that signs your browser in. Leave it running; Ctrl C stops it all. `--keep` keeps the store from last time; `--watch` rebuilds and restarts kelve when its Rust changes. Logs are in `.sandbox/`. `KELVE_DEV_PORT` moves kelve, with the hosted service on the port below it and the web app on the port above.

Then, in another terminal, have the example's agent hand its work over:

```bash
bun dev/kelve.ts agent
```

It hands the change over in kelve, as a person's agent would, asking for a debrief, and does every job kelve keeps for it: the debrief, then research, explanations and learning as you ask for them from the debrief. Its answers come from `dev/example/loop.ts`. `--live` prints each call. Agree with a decision, ask for one another way or look into it, then run it again, and it does what you asked while you watch.

## Being the agent

The best way to try a change to anything an agent touches is to be the agent yourself: hand work over, take each job, do it, hand it in, and watch the app. One MCP call at a time, exactly as an agent makes them:

```bash
bun dev/kelve.ts call open '{"what":"worktree","base":"main","path":"'$PWD'/.sandbox/ledger","debrief":true}'
```

```bash
bun dev/kelve.ts call next_job
```

Do the job as its instructions say, write the answer in the shape it gives, and hand it in, where `answer.json` is `{"job": 1, "answer": {…}}`:

```bash
bun dev/kelve.ts call hand_in @answer.json
```

Each answer taken comes back with the next job. `open` says the number of what it opened; give it as `for` (`call next_job '{"for":1}'`) to take only that one's jobs, as an agent in one of several tabs does. `bun dev/kelve.ts tools` lists the tools as an agent sees them, and `call skills` reads the sandbox's skills as the agent would. Or give your own coding agent the sandbox's tools: `bun dev/kelve.ts mcp` prints the line that adds them, for Claude Code `claude mcp add kelve-sandbox …`. An agent working on kelve itself can do the same, to try what it changed.

- **The hook.** Be the agent's hook, one of Claude Code's events at a time: a message, a file read, a turn's end.

  ```bash
  bun dev/kelve.ts moment '{"hook_event_name":"UserPromptSubmit","prompt":"No, exit 1 on a bad argument"}'
  bun dev/kelve.ts moment '{"hook_event_name":"PostToolUse","tool_name":"Read","tool_input":{"file_path":"src/retry.rs"}}'
  bun dev/kelve.ts moment '{"hook_event_name":"Stop"}'
  ```

  They're all one session of the agent's; `SANDBOX_SESSION` names another. A message that corrects the agent becomes a `learn` job, and the skill for a kind of work comes the first time a file of that kind is read.
- **A verified run.** Sign in from Your agent or Settings, Account: the stand-in sends the browser straight back. `bun dev/kelve.ts hosted skills` gives the sandbox skills to send, one of them the skill the stand-in's runs come out better with. It takes any key starting `sk-test`, as an API key or a subscription's token. A run moves on a task each time its page asks; `hosted finish <run>` finishes it at once, `hosted taken <address>` gives an address to someone else, and `hosted runs <n>` sets how many runs a day.
- **Notes for the agent.** Leave a note for your agent, then read it as the agent would, from the example project: `bun dev/kelve.ts call notes`, and say it's done with `bun dev/kelve.ts call note_done '{"note":"agent/…","version":"…","did":"…"}'`. The sandbox has a Claude Code folder of its own (`.sandbox/claude`) and a home of its own for other agents' settings (`.sandbox/home`), so turning on kelve's hook never touches yours.
- `bun dev/kelve.ts link` signs a browser in again.

## Someone new

```bash
bun dev/kelve.ts newcomer
```

To try kelve's onboarding as someone new meets it: they ask their agent to install kelve, it does and sets it up, and the first thing they open shows kelve. This serves the installer (`install.sh`) and this kelve, packed as a release, on 4692, beside a stand-in hosted service. Then it prints the line that makes another terminal theirs: a kelve on 4690 with a store of its own, and a Claude Code of their own (`.newcomer-claude`), so yours are never touched. Log that Claude Code in once, the first time; it stays logged in, and kelve's connection and hook in it start again each run. In it, ask: "Install kelve from http://127.0.0.1:4692/install.sh". Then start it again, so it has kelve's tools, and ask it to show you something.

`--agent` has that Claude Code install kelve unattended, then checks what it set up: kelve installed, connected, its hook, and how much kelve marks.

## The example

`dev/example/` is one small project, used everywhere: by the sandbox, the end-to-end tests and the app's own example.

- `base/` and `change/`: the ledger as it was, and the files its change touched. `change.json` says the change's commit message, and the title and line its agent gives.
- `agent.ts` and `loop.ts`: the example's agent, answering kelve's jobs: its debrief of the change, research, explanations, what it learns and what it draws.
- `make.ts`: makes the ledger as a git repository, for the sandbox and the tests.
- `web/src/example/example.json`: the app's own example, every answer the app asks kelve for, recorded by `bun dev/kelve.ts example` from a fresh sandbox. Run it again after changing the example or what the app asks for.

## Every state, and how it looks

With the sandbox up, `#/states` shows every state of every screen, drawn from the example with no server: light and dark side by side, at a phone's width on a toggle, with every string each shows listed under it. It's how a change to how anything looks, or to its words, is reviewed. Every screen also works at `#/example/...` with no server, and `#/example/kit` shows every control.

The same states are screenshot and compared with those committed in `web/e2e/looks/screenshots`. Fonts draw differently on each system, so they're taken in Playwright's own Linux image, here as in CI, which needs Docker:

```bash
cd web && bun run build && bun run looks
```

After a change meant to show, take them again with `bun run looks --update-snapshots`, look at what changed, and commit them.

The README's pictures, in `.github/assets`, are some of those states taken at twice the pixels, so they stay sharp on dense screens and in GitHub's narrow column, and compressed. `web/e2e/pictures` says which. After a change to what they show, take them again, look at each, and commit them:

```bash
cd web && bun run build && bun run pictures
```

## How fast it is

Opening a change should feel instant. To see how long kelve takes over what the app asks for, on a change as large as you like:

```bash
cargo build --release -p kelve && KELVE_BIN=target/release/kelve bun dev/speed.ts --change="against HEAD~25"
```

It starts a kelve of its own, with a fresh store, opens the change, and times each file's diff the first time and again, the change itself, and the side views. `--dir` times another repository, and `--files` how many of its files (30). Nothing is done by an agent: the times are kelve's own. Measure a release build, as people run: a debug build is many times slower.

## Using kelve from source

To use a kelve built from this checkout day to day, install it:

```bash
cd web && bun install && bun run build && cd .. && cargo install --path crates/cli --locked
```

then `kelve stop`, so the next `kelve open` starts the new one. It uses kelve.ai (`https://graph.kelve.ai`) for verified scores and your page, as a release does. To use another hosted service, such as one you run yourself, stop kelve and start it with that service's address:

```bash
kelve stop && KELVE_GRAPH_URL=https://graph.example.com kelve open .
```

Each address has its own store in kelve's data folder, so its own sign-in: sign in once on each, and going back to one finds it as it was.

## Checks

Everything CI runs, from the repo root, is in [AGENTS.md](../AGENTS.md).
