# How kelve is built

kelve turns what the person knows into skills their agents use, and shows them what their agent did and why. When their own coding agent hands work over, kelve asks it for a debrief: what it did, how it works, and every decision that matters. What the person corrects, changes and sends from a debrief becomes parts of their skills, which kelve hands to their agents as they work. Each part can be checked on their own past work, and kelve's hosted service can verify the gain on tasks nobody sees in advance. The design is in [skills.md](skills.md); this page says how the open-source app is put together.

## The app and the hosted service

This repo is everything that runs on the person's computer: the `kelve` command, the local service, and the web app. It needs nothing else. kelve's hosted service, kelve.ai, is used for one thing only: the person's account there, for verified runs and their page. It's a private service; this repo has its API's shapes, the one client for it and a stand-in for tests (`crates/graph-api`, `docs/hosted-api.json`).

Nothing of the person's leaves their computer, except their skills, to the hosted service, for a verified run they asked for, with secrets and paths stripped first and shown to them before anything goes. Their work, their messages to their agent, their edits and their tasks never leave. Their page shows only what they publish.

## The crates

| Crate | What it holds |
|---|---|
| `crates/core` | Everything that's pure: skills and their parts, sections and kinds of work (`skills.rs`, `skills/`), the debrief's format and its checks, research and explanations (`debrief.rs`), when a debrief is due (`due.rs`), where something learned goes (`learn.rs`), your agent's character, drawn from parts (`character.rs`), the hook for each agent (`hook.rs`), notes as files (`notes.rs`), what kelve marks for the person (`spec.rs`, `lens.rs`, from `marks.md`), what's sent to the hosted service (`hosted.rs`), secret stripping (`redact.rs`), fencing material (`fence.rs`), what agents are told for each job (`instructions.rs`), reports of problems (`problems.rs`), updating (`update.rs`), and the API's shapes (`views.rs`). No IO, and nothing panics on bad input. The shared graph's entries, ideas, relations, lessons and terms are kept here only for the hosted service's version 1, until it retires it; kelve never uses them. |
| `crates/store` | The local store: one SQLite file, its schema (`migrations/`), and every query, grouped by what it's about. |
| `crates/engine` | Reading what the agent hands over, without running anything from it. A folder is walked as git sees it (`.gitignore`, `.kelveignore`, no links followed); files that may hold secrets, lockfiles, vendored, minified, generated and binary files, and licences, are listed but never read. Text is read with every secret masked in place. Changes (uncommitted work, a branch against its base, a commit) come from git's own files through gitoxide; git itself never runs. A branch is read from where it left its base, or, when the base is a local branch behind its remote's, from where it left that. |
| `crates/graph-api` | kelve's hosted service's open API. Version 2 (`v2/`): signing in, the account, its handle and page, the character on it, scores, and verified runs, from the offer to the result, with the one client for it and, with the `stand-in` feature, a stand-in held in memory for tests (`kelve-graph-stand-in` runs it alone); with `openapi`, it's described in `docs/hosted-api.json`. Version 1 is the shared graph's: kelve sends it only the weekly count. Its shapes, client and reading tokens stay for the hosted service, which still serves it (`docs/graph-api.json`). |
| `crates/server` | The local service: the jobs it keeps for the person's agent and what it does with each answer (`agent.rs`), and its list (`list.rs`); the agent loop (`agent_loop/`: debriefs, research, explanations, learning, skills handed over through the hook and linked into each agent's skills folder); what was handed over (`handover.rs`); the debrief's page (`debriefs.rs`) and home (`home.rs`); skills on disk and in the store (`skills_home.rs`, `skills_sync.rs`), and checking them, and Fits you (`skill_checks.rs`); your agent (`your_agent.rs`); kelve.ai (`hosted.rs`); the hook's router (`moment.rs`), kelve's hook in the agent's settings (`hooks.rs`) and work shown as it's done (`handoff.rs`); what's open and the notes beside it (`reader.rs`); what kelve marks for the person (`spec.rs`); the weekly count (`counts.rs`); reports of what went wrong (`problems.rs`); newer releases (`updates.rs`); the system's file picker (`pick.rs`); signing a browser in (`auth.rs`); the API (`api/`); and the web app (`web.rs`), on one port. |
| `crates/cli` | The `kelve` command: `open`, `diff`, `serve`, `status`, `stop` and `update`; `setup`, `hook` and `marks set`, to set kelve up; and for the person's agent, `mcp` (kelve's tools over MCP), `job` and `job done`, `kept`, `notes` and `notes done`, `moment` (what kelve's hook runs, and with `--bench`, the same for a benchmark run, offline) and `worktree`. |
| `web/` | The web app, in React. `web/e2e` tests it in a browser. |

## Running kelve

kelve is the `kelve` command, with the web app inside it. Installing it runs `kelve setup`, which connects the person's agent (Claude Code, Codex, Cursor or Gemini) to `kelve mcp`, adds kelve's hook to its settings, offers to link their skills into each agent's own skills folder, and says how much kelve marks. The agent starts `kelve mcp` itself and calls its tools: `open`, to hand its work over or show the person something; `next_job`, `hand_in` and `kept` for kelve's jobs; `skills`, for the person's skills when the hook didn't give them; `notes` and `note_done` for the notes the person leaves it; `marks` and `set_marks` for what kelve marks; and `update`. `open` says the number of what it opened, and `next_job` and `hand_in` take it as `for`, so agents in several tabs each do their own. An agent without MCP uses `kelve job`, `kelve job done`, `kelve kept` and `kelve notes`.

Each asks a running kelve, with the command line's secret; when none answers, `kelve mcp` and `kelve open` start one first, in the background, as `kelve serve --background` in a process group of its own, so closing the terminal leaves it running. Its log goes to `kelve.log` in its data folder. What the agent opens, kelve opens in a new browser tab, signed in with a one-time link, whatever tabs are already open, so it's always in front of the person: a page can't bring itself, or another tab, forward. When a kelve tab already shows the same thing, the two find each other (a `BroadcastChannel`, `web/src/tabs.ts`), the new tab takes the old one's place, and the old one closes. Where it can't open a browser, it shows the link where `kelve serve` runs. `kelve stop` asks it to stop. People open kelve at `KELVE_URL`, else `kelve.localhost` on `KELVE_BIND`'s port. The command line reaches it at `KELVE_URL`, else `localhost` on that port, since outside a browser not every system resolves `kelve.localhost`.

Once a day kelve asks GitHub for its newest release (`server::updates`); settings say when there's one, and so does one tool's answer to the agent, once for each version. `kelve update`, or the `update` tool, puts it in place ([releasing](releasing.md)). A newer kelve brings its hook in the agent's settings up to date as it starts.

## The local store

One SQLite file in kelve's data folder for each address of the hosted service (`config::store_name`): `kelve.db` for kelve.ai, `kelve-<host>-<port>.db` for any other, and `kelve-off.db` for none, so changing `KELVE_GRAPH_URL` changes the whole store, sign-in and all. In WAL mode, migrated as kelve starts (`crates/store/migrations`), with a copy put aside first when there are migrations to run. It keeps:

| Tables | What |
|---|---|
| `instance`, `secrets` | Settings, with how your agent looks; and the keys kelve keeps, the kelve.ai account's and the person's own model provider's, sealed with the install's key. |
| `sign_in_links`, `browser_sessions` | Signing a browser in, as hashes. |
| `jobs` | Background work, debounced by key, retried with a growing wait. |
| `asks`, `agents` | The jobs kept for the person's agent, each with its material, the shape of its answer and the work to run again once it's answered; and the agents that have said hello. |
| `opened` | What was opened, by whom, with the agent's title and line about it. |
| `handovers`, `handover_files`, `messages` | What the agent handed over, as kelve read it then, each file with its hunks and its text, secrets masked, to compare with what's committed later; and what the person said to their agent, short and with secrets stripped, kept a week, to catch what they corrected. |
| `debriefs`, `decisions`, `research`, `explanations` | Each debrief as the agent wrote it, the person's call on each decision, and the research and explanations they asked for. |
| `skills`, `parts`, `skill_edits`, `skills_given` | What kelve keeps beside the skill files, which are the source of truth: each skill's kind and kind of work, each part and what its checks found, each edit kelve made, to undo, and which skills each session was given. |
| `candidates`, `evidence`, `past_tasks` | What kelve learned and where it would go, with what showed it; and the person's past work, for checks. |
| `checks`, `fits_picks` | Skill checks and what they found; and the person's picks in Fits you. |
| `hosted_runs` | Verified runs asked for, and how each stands, as kelve.ai last said: never the skills that went, or the key that paid. |
| `touched`, `moments`, `held_for_agent` | The agent's work as the hook saw it, kept a day: the files it read and edited, when its last moment was, and what an agent that can't be given text at the moment gets at its next call. |
| `notes_told` | The notes left for the agent that its hook has told it of, at which version. |
| `counts`, `counted`, `counts_sent` | How kelve was used, in numbers a day, and each week's count as it was sent, if the person shares it. |
| `updates` | The newest release kelve heard of, and the newest the agent was told of. |

Times are milliseconds since 1970, UTC, so they compare as numbers.

## The person's agent does kelve's work

kelve has no model and no key of its own. Everything it needs a model for is a job kept for the person's agent (`server::agent`), each under instructions this kelve was built with (`core::instructions`), by version: writing the debrief of work it handed over (`debrief`), looking into a decision in it (`research`), explaining an idea it rests on (`explain`), turning what the person corrected, changed or sent into parts of their skills (`learn`), redoing a past task twice, with a part and without, to check it (`compare`), drawing a part or a character for your agent (`draw`), and doing what a note asks (`note`). Each job carries its instructions, sent whole once and then by name; its material, with everything from the work fenced as material and never instructions (`core::fence`); and the JSON Schema its answer must fit. kelve checks an answer's shape before it takes it, and says what's wrong when it doesn't; one turned away twice is put aside.

A job an agent took and didn't answer within ten minutes waits again. An agent taking one thing's jobs by its number is given its next part when kelve's own work for it asks for it, waiting up to 45 seconds while that work runs; if none comes in time, it's told kelve is still preparing the next part, and asks again. kelve can't start the agent and doesn't try: what's kept waits for the next time the person asks it for kelve's work, or its hook says, as its turn ends, that jobs wait. The agent's list (`server::list`) is the person's view of the notes waiting for it.

## Handing work over, and the debrief

The agent hands its work over as its branch since its base, committed or not (`what: worktree`), so handing it over again brings the same thing up to date, never another. In a folder that isn't in a git repository, it shows every file it changed (`what: files`). kelve keeps the change as it read it then (`server::handover`): where, which commit, whether some wasn't committed, and each file with its hunks and its text, secrets masked.

Whether a debrief is due is decided from the change itself (`core::due`), as the person chose in settings, or at once when they asked for one ("debrief this"). When it's due, `open`'s reply tells the agent to write it now, while it remembers why: it takes the `debrief` job and hands in one markdown file, which kelve checks against the change as it was handed over (`core::debrief::read`). One piece of work has one debrief until it's sent; more on the same work brings it up to date.

`open` lands the person on what was handed over (`#/o/<id>` in the web app): its debrief, once there is one, else the change view, file by file, from what was kept. Something that isn't a change, a folder or some files, opens as plain files, with the notes beside them.

From the debrief the person agrees with each decision, or asks for it another way (a note for their agent), asks their agent to look into it (`research`), to explain an idea (`explain`), or says they know it (`marks.md`). Sending it leaves the notes and asks the agent to learn from it.

## Learning, and skills

What the person corrects while the agent works (the hook keeps what they send, `server::moment`), what they change in work after it was handed over (the agent's own commits as its hook sees them, others as kelve next hears from the agent or the person, `agent_loop::after`), and what they send from a debrief each become a `learn` job (`agent_loop::learn`). Its answer becomes candidates, each placed in a skill by kelve's three questions (`core::learn::target`): a project's skill, a task's, or the skill for the kind of work it was caught in. What was sent from a debrief is written into the skill at once; the rest waits in the next debrief. A new skill is only ever suggested.

Skills are files: the person's own in `~/.kelve/skills/<name>/SKILL.md` (`KELVE_SKILLS`), a project's in its `.kelve/skills/`, with only standard front matter. kelve rereads them (`server::skills_sync`) and never changes back what the person wrote by hand; what kelve knows beside them is in the store. They reach the agent through the hook, each once a session (`agent_loop::deliver`): everywhere and the project's as the session starts, a kind of work's the first time the agent reads or edits that kind of file; through the `skills` tool; and linked into each agent's own skills folder (`agent_loop::links`), which is on for a new install until the person says no, in `kelve setup` or in settings.

Each part can be checked on the person's own past work (`server::skill_checks`): the agent redoes a past task twice, as two subagents in fresh worktrees, with the part and without, and kelve says how close each came to what the person kept (`core::skills::compare`). Fits you does the same with all their skills against none, and the person picks blind.

## kelve.ai

Signing in is for one thing only: the person's account on kelve's hosted service, for verified runs and their page (`server::hosted`, `api/hosted.rs`). kelve listens on a port of its own for one sign-in, and opens the person's browser at kelve.ai with a PKCE challenge (RFC 8252). kelve.ai sends them to GitHub or Google and back, then to kelve with a one-time code, which kelve exchanges, with the verifier only it holds, for a key of its own, sealed like every key it keeps.

A verified run sends the person's skills, with secrets and paths stripped and every file shown to them first, with the kind of work, agent and model they chose, after an estimate in their money they confirm. It's paid with their own provider's key, sealed in the store and sent with that run alone. The page shows their agent's character and name, and the scores they choose to publish; taking it down is one click.

## What's open, and notes

`web/src/reader` shows something opened that isn't a change with a debrief or a handover: a folder, a file or some files, as plain files, read through CodeMirror (`FileView.tsx`, `code.ts`), markdown and plain text as text (`Prose.tsx`), with the notes beside them and the agent's list.

Notes (`NoteParts.tsx`, `NotePage.tsx`, `server::reader`) are markdown files with front matter (`core::notes`): with the work in the project's `.kelve/notes/`; just the person's, and those for their agent, in their own notes folder, a folder per project. A note for the agent says `for: agent` and whether it's done, and is a job on its list; the agent reads those waiting, fenced, with `notes`, and says what it did with `note_done`. kelve's hook, added only with the person's yes (`core::hook`), runs `kelve moment` as they message their agent, which says when notes wait.

## The hook

kelve's hook, `kelve moment --agent <agent> --on <event>` (`cli::moment`), runs on each agent's own events, for eleven agents (`core::hook` reads each one's event as kelve's moment and answers it its own way; `server::hooks` puts it in and takes it out, together). As the person sends a message: notes waiting, a newer kelve, and the skills the session starts with; and what they said, kept to catch corrections. As the agent reads or edits a file: the skill for that kind of work, the first time. As it commits: what the person changed in work it handed over. As its turn ends: to show its work in kelve, when it edited files and nothing has been opened in kelve in that folder since its last edit (`server::handoff::done`, once for each last edit), and the jobs waiting for it. When it commits, pushes or opens a pull request for work kelve hasn't shown, to show it now, once for each commit (`server::handoff::after`, `core::hook::handing_over`).

## When something goes wrong

kelve sends no report of a problem anywhere. When it panics, or its page meets an error, it keeps a short report in `problems/` beside its data (`server::problems`), at most 20, the oldest going first. Each is tidied first (`core::problems`): secrets stripped as everywhere, paths on the person's computer taken out, and a backtrace kept as function names only. Settings shows each report exactly as it's kept, with a link that starts an issue on kelve's repository filled in with it, which the person reads again on GitHub before posting, or not; or they forget it.

## The weekly count

kelve counts a few things a day (`server::counts`): what's opened, by kind; notes written, by where they're kept; jobs the agent did; and the day an agent first said hello. Only numbers are kept. If the person says yes in settings, kelve makes the install a random ID, 128 bits, deleted when they stop, and sends the hosted service last week's count once a week, still to version 1 of its API (`POST /v1/counts`): the week's counts and each day's, kelve's version, the platform and that ID, with no account and no key, so the service can't tie it to anyone. `DO_NOT_TRACK` or `KELVE_TELEMETRY=off` keeps it from going, whatever was chosen. Settings shows this week's exactly as it would go, and every week sent, as sent.

## Signing a browser in

kelve makes a one-time link that signs a browser in, and opens it, or shows it where `kelve serve` runs. `kelve open` asks for a new one. The command line shares a secret with kelve, in a file only its owner can read; with it, the command line can ask for a link, never have one.

## The tests

`cargo test` runs everything, with stores made in memory or in a temporary folder, and a stand-in for the agent and for the hosted service. It also writes the web app's types (`web/src/api/gen`) from `core::views`, the characters kelve draws and the CSS that moves them (`web/src/ui/gen`) from `core::character`, and the hosted service's OpenAPI documents. `web/e2e` starts kelve beside the stand-in hosted service, with a stand-in agent that calls kelve as `kelve mcp` does (`web/e2e/agent.ts`), and tests it in a browser, as people use it (`web/e2e/tests`). `web/e2e/looks` takes a screenshot of every state of every screen (`src/example/states.ts`), drawn from the example, and compares each with the one committed, in Playwright's Linux image.
