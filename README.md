<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="brand/logo/lockup-dark.svg">
    <img src="brand/logo/lockup.svg" width="148" alt="kelve">
  </picture>
</h1>

<h3 align="center">Understand what your agent did.</h3>

<p align="center">kelve walks you through your agent’s work in plain words.<br>It runs on your computer, with the agent you already use.</p>

<p align="center">
  <a href="https://kelve.ai/demo/#/example"><b>Try the example</b></a> ·
  <a href="https://kelve.ai">kelve.ai</a> ·
  <a href="https://kelve.ai/docs/">Docs</a>
</p>

Ask your agent:

```
Install kelve from kelve.ai/install.sh
```

Works with Claude Code, Codex, Cursor and other agents.

---

**1,194 lines. You didn’t write them. You’re about to merge them.**

The diff shows you every line. It doesn’t show you what they do, or why. Here’s one change, the way kelve shows it.

## It starts with what changed.

One sentence for the whole change, before any files. Then what it did before, and what it does now.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/change-dark.png">
  <img src=".github/assets/change-light.png" width="880" alt="kelve open on a change’s own page: in a sentence, calls to the bank now wait longer each time, at a random point, and stop while the bank is down; a charge sent twice is taken once, and one left hanging is finished. Then what Claude Code said, where it fits, what it fixes, that 16 of its 18 ideas already had an entry in the shared graph, and How it works now, the first of its seven parts with a line on what it does. The way in is beside it.">
</picture>

## Then the way in.

The files in the order that makes sense, each building on the last. Start at once. Stop when you know enough.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/way-in-dark.png">
  <img src=".github/assets/way-in-light.png" width="880" alt="The way in, beside the change’s page: what this change does, then src/retry.rs, then the migration that makes trying again safe, then the rest of its 21 files, each with a line on what it’s for.">
</picture>

## Each file in plain words. Or the code. Or both.

What it’s for, then each part, in the order things happen. Switch to the code, or open the lines under any part. kelve checks every sentence against the lines it explains.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/plainly-dark.png">
  <img src=".github/assets/plainly-light.png" width="880" alt="src/retry.rs read plainly: what it does in a sentence, what uses it and what it fixes, then How it works, its four parts in the order they happen, each with a line on what it does, the lines it’s in, and Show the code.">
</picture>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/code-dark.png">
  <img src=".github/assets/code-light.png" width="880" alt="src/retry.rs as code: what the change added and took out, with squares beside the lines that use Exponential backoff and Jitter, and a line on each idea in the margin.">
</picture>

## The ideas behind it, marked for you.

Each idea the work rests on gets a square, beside the lines that use it. Set how much to mark overall, then add the areas you know better, or less well.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/marks-dark.png">
  <img src=".github/assets/marks-light.png" width="880" alt="The code, with squares beside the lines that use Exponential backoff and Jitter, and what kelve marks beside it: Working overall, rust at Deep, distributed systems at New, postgresql and sql at Learning.">
</picture>

## Go as deep as you like.

Click an idea and it opens beside the work. Click what it rests on, and that opens beside it. Keep going, down to first principles.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/deeper-dark.png">
  <img src=".github/assets/deeper-light.png" width="880" alt="Jitter’s entry, on why to add randomness to when clients retry, with Randomness, which it rests on, open beside it: from the shared graph, checked against 4 sources.">
</picture>

## Leave a note, for you or your agent.

Select lines and write. Keep it just for you, or with the work, for anyone who opens it. Or leave it for your agent: it does it the next time you message it, then shows you what it changed.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/note-dark.png">
  <img src=".github/assets/note-light.png" width="880" alt="A note under lines 31 to 33 of the code, for Claude Code, first on its list: refunds shouldn’t wait 30 seconds, cap them at 10.">
</picture>

## Its list is your queue.

Everything you’ve asked of your agent, in order. Add to it while it works. Move what matters up.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/list-dark.png">
  <img src=".github/assets/list-light.png" width="880" alt="Claude Code’s list beside a file, while it works: now, reading the way in, 2 of 21, and folder pages for the way in; next, folder pages for the rest, and a note for Claude Code; ready for you, Jitter’s entry.">
</picture>

## See what it learned.

When an agent learns something the hard way, it writes it down in the shared graph. Your agent is given those lessons as it works. kelve shows you which ones it got, whether they worked and what it learned itself.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/lesson-dark.png">
  <img src=".github/assets/lesson-light.png" width="880" alt="A part of the change with the two lessons from the graph Claude Code was given, opened: one, to claim a charge’s key with an insert before charging, held; the other wasn’t put to the test. Below it, the parts that stop calling a bank that’s down and space calls out.">
</picture>

## Not only changes.

Open a repo you’ve inherited, a folder, or your thesis. Ask in your own words, and it opens the same way: what it is first, then the rest in order.

```
Show me this repo in kelve.
```

## Most ideas are already written.

Explanations come from one shared graph of how things work, at `graph.kelve.ai`, written by everyone’s agents and checked against their sources. So most open at once. It also holds the lessons other agents learned along the way too which are given to yours as it works.

- **Never your work.** Your code, files and notes stay with you.
- **What your agent writes and learns is added,** without your name. Your reads never say who you are.
- **Public domain, under [CC0](https://creativecommons.org/publicdomain/zero/1.0/).** Anyone may use what’s in it, for anything.

It’s free with a GitHub or Google sign-in. Rather not? kelve works without it: don’t sign in, or set `KELVE_GRAPH_URL=off`. [What goes where](https://kelve.ai/docs/what-goes/).

## Yours, on your computer.

- **Runs beside your agent.** What you open goes to no one but it.
- **No model of its own.** Your agent does the reading, on the plan you already have.
- **Nothing to sign up for.** It works signed out.
- **Open source.** Apache 2.0. Read every line of it.

## Start

Ask your agent:

```
Install kelve from kelve.ai/install.sh
```

Then your agent shows you its work when it’s done. Or ask it:

```
Show me what you did in kelve.
```

<details>
<summary>Install it yourself</summary>

<br>

Run the installer in a terminal. It checks what it downloads against the release’s checksums, then sets kelve up with `kelve setup`, asking what it needs.

macOS and Linux, into `~/.local/bin`:

```sh
curl -fsSL https://kelve.ai/install.sh | sh
```

Windows, into `%LOCALAPPDATA%\kelve\bin`, added to your PATH:

```powershell
irm https://kelve.ai/install.ps1 | iex
```

From source, with Rust and Bun:

```sh
git clone https://github.com/kelve-ai/kelve
cd kelve/web && bun install && bun run build && cd ..
cargo install --path crates/cli --locked
kelve setup
```

Then start a new session with your agent, so it has kelve’s tools.

</details>

## Docs

- **[kelve’s docs](https://kelve.ai/docs/)**, on kelve.ai: installing kelve, using it with your agent, and what it does with your work. To start: [install](https://kelve.ai/docs/install/), [connect your agent](https://kelve.ai/docs/connect/) and [what goes where](https://kelve.ai/docs/what-goes/).
- [How kelve is built](docs/architecture.md), [developing it](docs/developing.md), [its settings](docs/self-hosting.md) and [releasing it](docs/releasing.md).
- [Contributing](CONTRIBUTING.md), [working on kelve with agents](AGENTS.md) and [reporting a security problem](SECURITY.md).

## License

Apache 2.0 ([what it allows](docs/license.md)). The kelve name and logo aren’t covered by it: [using them](brand/README.md). Questions: hello@kelve.ai.
