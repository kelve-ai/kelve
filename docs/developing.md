# Developing kelve

## Start here

From the repo root, with [kelve-graph](#what-you-need) checked out beside it and Docker running:

```bash
bun dev/kelve.ts up
```

Open the link it prints at the end. Then, in another terminal:

```bash
bun dev/kelve.ts story
```

That's all of kelve on your computer: the app, with an example project, kelve.ai itself (kelve-graph, with its Postgres, the benchmark's runner and its gateway), and an agent playing the whole story while you watch. None of it is yours or real, and in its default mode none of it needs a key or costs anything. Change the web app and the page reloads; start `up` with `--watch` and kelve rebuilds when its Rust changes. Before you say you're done, run the checks in [AGENTS.md](../AGENTS.md).

## Why it's this way

kelve does nothing on its own. It needs work handed over, the person's coding agent to do its jobs, and, for verified scores and a page, kelve's hosted service. The sandbox gives it each: a small payments service whose change is handed over (`dev/example`), kelve.ai run from its own source, signing you in as a local person since there's no GitHub or Google here, and an agent that answers kelve's jobs from what it was given to know. So you never touch the kelve you use, your own skills, the real kelve.ai, or a model you pay for, unless you ask for one, and it runs the same on any computer. The same example is every screen in the states gallery and every test, so what you see in the sandbox is what the tests check.

## What you need

- Rust (stable) and [Bun](https://bun.sh).
- Docker, for kelve.ai's Postgres and its runs, and for the screenshot tests.
- kelve-graph, the hosted service, checked out beside this repo as `../kelve-graph` (or set `KELVE_GRAPH_DIR`). Without it, `up` says so and runs the stand-in for kelve.ai instead (`kelve-graph-stand-in`), which fakes sign-in, runs and pages in memory.
- kelve-site, if you want kelve.ai's site too, as `../kelve-site` (or `KELVE_SITE_DIR`), with its packages installed (`bun install` there). It's run if it's there.
- For the browser tests, Chromium and WebKit, once: `cd web && bunx playwright install chromium webkit`.

## The sandbox

```bash
bun dev/kelve.ts up              # stand-in mode: no keys anywhere
bun dev/kelve.ts up --real       # real models, on your key
bun dev/kelve.ts status          # what's running, and where
bun dev/kelve.ts down            # stops all of it, as Ctrl C in up's terminal does
```

`up` builds what's changed, then starts each part in order, saying what it started, and prints one link that signs your browser in. It builds the benchmark's images the first time (`bench/build.sh` in kelve-graph; `--images` builds them again after you change kelve-graph or kelve's hook). `--keep` keeps the stores from last time; `--watch` rebuilds and restarts kelve when its Rust changes. Logs are in `.sandbox/`: `kelve.log`, `graph.log`, `runner.log`, `gateway.log`, `web.log`, `site.log`.

| Port | What |
|---|---|
| 4699 | **the web app**, with hot reload: open this one |
| 4698 | kelve itself, with a fresh store in `.sandbox/` and skills of its own in `.sandbox/skills` (`KELVE_SKILLS`), never yours |
| 4697 | **kelve.ai**: `kelve-graph serve`, with sign-in, pages (`/@you`, `/s/…`), scores and verified runs |
| 4696 | its Postgres, in Docker (compose project `kelve-dev`, from `dev/compose.yaml`) |
| 4695, 4694 | the runner's gateway, its admin and its proxy, which holds the model's key; the runner reaches it from Docker through its forwarder, `kelve-dev-forwarder` |
| 4693 | the stand-in model, in stand-in mode |
| 4700 | kelve.ai's site, when kelve-site is there: its dev server (on 4701) behind a door that sends `/@…` and `/s/…` to kelve.ai, as Firebase does |

`KELVE_DEV_PORT` moves them all: kelve on it, kelve.ai on the port below, the web app on the port above. The example project is `.sandbox/ledger`: a small payments service, as a git repository, whose last commit is the change "Calls to the bank that are safe to retry".

### Two modes

**Stand-in, the default.** No keys anywhere: `up` takes every model's key out of what it starts. Verified runs use the example bank, `dev-rust-0` (two small Rust tasks), the stand-in agent, which makes a partial fix, or the whole one when the `standin-expert` skill reaches it through the real delivery path, the stand-in model behind the gateway, and the stand-in judge, with 4 standard runs and 2 with your skills a task. A run takes a few minutes and ends with a real result: better on both tasks with `standin-expert`, no better without it. Two tasks are too few to be sure, so it always says no clear difference rather than verified; that takes 5 tasks or more. It costs nothing.

**`--real`.** kelve.ai's side runs on `ANTHROPIC_API_KEY` from your environment: the standard runs and the judging. It's given only to the gateway and the runner, as they start, and never written anywhere. Runs use `rust-1` (made live in the sandbox's database alone, while it's still being built) and Claude Code on Sonnet 5.5. You give your own API key, or a Claude subscription's token from `claude setup-token`, in the app, as anyone would: this kelve.ai takes a subscription, since it's yours alone. It costs money, and `up` says how much: the standard runs once for the bank and profile, about 10 tasks × 20 runs × $0.40, so around $80 (`--ci` makes it 4 runs a task, around $16), judging within $25 a day, and your own runs, which the app estimates before anything starts.

### What to try first

1. **The story.** `bun dev/kelve.ts story`, with the app open on home. The example's agent, through `kelve mcp` and kelve's hook as Claude Code runs them, makes the ledger's change while you talk to it, corrects course when you correct it, and hands it over; kelve asks it for the debrief, and it writes it. Then the story plays you: it agrees with a decision and wants it every time, has one looked into and an idea explained, changes one, and sends the debrief. The agent does each and turns what you sent into parts of your skills. You keep the work, committing it; kelve doesn't yet keep past work to check skills on by itself, so the story keeps it, as the end-to-end tests do. Then a part is checked, the agent redoing that work with and without it, and it draws a party hat. Watch home, the debrief, Skills and Your agent fill up. `--live` prints each job.
2. **A verified score.** `bun dev/kelve.ts hosted skills` gives your sandbox agent skills to send, one of them `standin-expert`. In Your agent, choose Get a verified score, and sign in: kelve.ai's page "Sign in as a local person" stands in for GitHub and Google, so type a name (the same name is the same account). Give a key (in stand-in mode any will do, but `sk-refused`, which is turned away as a provider would), confirm, and watch the run go task by task; `runner.log` and `bun dev/kelve.ts status` show its trials in Docker. Publish the score, choosing your page's address as you do, and open your page, at `http://127.0.0.1:4697/@you`.
3. **Be the agent yourself**, below.

Signing in as a local person is kelve-graph's `KELVE_GRAPH_DEV_SIGNIN=on`, which it refuses unless it listens on loopback alone. The older way, the stand-in for kelve.ai, is still there when kelve-graph isn't, and the end-to-end tests use it.

`bun dev/kelve.ts agent` is the shorter story: the example's agent hands its change over, as it was committed, and does every job kelve keeps for it, as you ask for them from the debrief.

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
- **A verified run, with the stand-in for kelve.ai** (no kelve-graph beside this repo). Sign in from Your agent or Settings, Account: the stand-in sends the browser straight back. It takes any key starting `sk-test`, as an API key or a subscription's token. A run moves on a task each time its page asks; `hosted finish <run>` finishes it at once, `hosted taken <address>` gives an address to someone else, and `hosted runs <n>` sets how many runs a day. With kelve-graph, runs move by themselves, in Docker, and it allows 20 a day.
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
