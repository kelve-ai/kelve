# Skills from your work, and proof they make your agent better

Status: proposal, 4 November 2026. Nothing here is built yet. It replaces the earlier proposal for debriefs and briefs.

## The idea, in short

Everyone has the same models. What one person knows that the model doesn't is what makes their agent better than anyone else's. kelve turns that into skills your agents use, and proves they work.

1. **You work with your agent as usual.**
2. **kelve catches what you know that your agent didn't:** what you correct while it works, what you change after it hands over, and the calls it made that nothing told it how to make.
3. **A debrief** shows what it did, how it works and what it decided. You agree, ask for it another way, or have your agent look into it. What you'd want every time goes into your skills.
4. **Your skills are updated.** They're standard skill files, private to you, and every agent you use gets them.
5. **Each part is checked.** A part that changes nothing is dropped, and one your agent still gets wrong is reworded.
6. **kelve proves it.** It runs your agent with your skills against a standard agent, on the same model. You see the gain on your own work, and you can get a verified score to share.

People don't do this to understand their agent's work. They do it because it makes their agent better, and because they can show it. Understanding is still there: to make a skill better, you have to know more. The debrief and the research behind it are how you get there.

## What we know already

**The first experiment** (6 tasks, 30 runs, judged blind; details below) found:

- A personal brief works. Briefed runs beat standard runs in 23 of 24 head-to-heads, with about 44% fewer decisions the person would change.
- A generic skill doesn't. A popular "best practices" skill did no better than nothing.
- Taste nobody wrote down isn't picked up. Only what's caught and written down helps.
- Untested lines are dead weight. 7 of 19 lines in the brief changed nothing, because the agent already did them.

**From research:**

- Skills (`SKILL.md`, agentskills.io) are read by most agents. They're the format to use.
- Skills often don't trigger: agents fail to load them when they'd help. kelve's hook and MCP server get the right skill to the agent at the right moment.
- The best skills come from real expertise: corrections, review, what changed and why. Generic ones come out vague.
- Testing a skill, with and without it, is common. Making skills from a person's own work, for every agent they use, with their say on each line, isn't. Nobody gives a person a verified score of their agent against a standard one.
- **Most skills change nothing.** In [SWE-Skills-Bench](https://arxiv.org/abs/2603.15401), most public skills made no difference to whether the work passed: 39 of 49 in its first version. The few that helped supplied a step the agent otherwise skipped. Those that hurt carried fixed paths that overrode the task, or a whole reference where the task needed one corner of it.
- **Short beats complete, and the agent's own skills hurt.** In [SkillsBench](https://www.skillsbench.ai/blogs/skillsbench-1-1), compact and standard-length skills added 19 to 22 points; comprehensive ones, under 1. Two or three skills in play helped most, four or more about half as much. Skills the agent wrote itself before working made it worse, by 8 to 12 points.
- **Agents pick worse from many similar skills.** [Choosing among skills](https://arxiv.org/abs/2601.04748) holds up to a point, then falls sharply, and similar skills are the main cause, more than how many there are.
- **An open-source tool that makes skills from sessions** shows what to avoid: its agent judges its own work, every piece of work must land in some skill, a skill can't be split, and nothing checks for near-duplicates before making a new one.

## How it works

### 1. kelve catches what you know

Three sources, all already within kelve's reach:

- **What you tell your agent while it works.** kelve's hook already runs as you send each message, in most agents: not yet in Cursor, Copilot, Augment or Antigravity, where your edits still count. When a message corrects the agent ("use a loop here", "that should exit 1"), kelve keeps it, with what the agent had just done.
- **What you change after it hands over.** kelve already sees your agent hand over its work, when it shows it in kelve. Now kelve also records what it handed over. When you commit, kelve compares: what you changed in the agent's work is the clearest sign of what you know.
- **Calls it made that nothing told it how to make.** The `debrief` job lists the decisions in the work that no skill or project instruction covered, where you may have a view.

Each becomes a **candidate**: one proposed line for a skill, with its evidence (the message, the edit, or the decision) and the skill it belongs in. A new job, `learn`, turns corrections and edits into candidates. Your agent takes it like any other job.

### 2. The debrief

It opens when your agent hands over its work. It's one page, built to read the same way for a one-line fix and for a change to 86 files.

1. **What it did,** in a few sentences, and how many files changed.
2. **How it works.** For a small change, one sentence. For a larger one, numbered steps, one for each part of the change, each naming the code it lives in.
3. **What it decided.** Every decision that matters, each with:
   - where it came from: its own call, something you said, or your edit after it handed over;
   - what it chose, as a sentence, and why, with what else it could have done;
   - your call: **Agree**, **Do it differently** (a note to your agent), or **Look into it** (research, below);
   - its own lines of code, folded away until you open them.

   On a large change, decisions are grouped by the part of the change they belong to.
4. **Every file,** by folder, with how many lines each changed and which decisions it holds. Each opens in kelve's change view, which already handles files of any size: a list of files on the left, the diff on the right with unchanged runs folded, and each decision noted in the margin beside its lines. Past about 20 files, folders start closed.
5. **For your skills.** What this debrief adds to which skill, and a new skill when one is worth making (see "When kelve suggests a new skill", below).
6. **What it used from your skills,** and **what the work rests on,** with the ideas new to you marked, and **Explain** and **I know this** beside each (see Explain, below).
7. **Send to your agent:** what it will redo, and what goes into your skills.

It works for any work kelve reads as text: code, docs, config, copy. Images, spreadsheets and other files kelve can't read as text are listed, but not shown, for now.

#### When there's a debrief

**A debrief is for a piece of work, not for every change.** A piece of work is what you asked for, from your request until your agent hands it back as done.

- **Your agent hands over once, when it has finished what you asked.** It shows the work in kelve, as it does today, at the end of its turn: not after each edit, and not when it stops to ask you something.
- **kelve decides whether it's worth a debrief, not the agent.** It decides from the change itself. A debrief is due when the work:
  - changes more than about 20 lines of code;
  - adds a file, a dependency or a command;
  - or names a decision in its handover.

  None is due when the work is small and mechanical (a typo, a rename, formatting, a version bump, files a tool wrote), when your messages already said exactly what to write, or when it changed nothing. Those show on home as one quiet line, with **Debrief it** if you want one anyway.
- **Your agent writes it straight away, while it remembers why.** When kelve answers its handover with a debrief job, the agent writes it before it hands back to you. It has every decision and its reasons in mind now; later, it would have to work them out again from the code.
- **One piece of work, one debrief.** If you ask for more on the same work before you've sent the debrief, your agent brings the same debrief up to date. Once you send it, the next piece of work gets its own.
- **You choose how often,** in settings: **For work worth one** (the default), **For every handover**, or **Only when I ask**. To ask, use **Debrief it** on home, or tell your agent "debrief this".

#### How your agent works with kelve

kelve never starts your agent. It works through what your agent already has: kelve's hook, its MCP tools, and your skills.

**On its own, your agent:**
1. gets your skills as it works, through the hook;
2. reads the notes you left before it starts anything you ask;
3. hands over finished work, once;
4. writes the debrief, when one is due.

**From kelve, you can ask it for more.** Each becomes a job or a note that waits for your agent:

| In kelve | What your agent does |
|---|---|
| **Change it**, on a decision | Redoes that part your way, as a note, before your next request |
| **Look into it** | Researches the question, and answers under the decision |
| **Explain** | Writes a short explanation of an idea |
| **Send**, with what you'd want every time | Adds parts to your skills |
| **Check it**, on a skill | Redoes past tasks with and without the part, only once you've started it, because it costs runs |
| **Make me a part**, on your agent | Draws a part or a character |

**When it does them:** notes go first, as part of your next request, because they change the work. Jobs wait until your agent finishes what it's doing: as it hands back, kelve's hook gives it what's waiting, and it does them before going quiet. You can also tell it "do what's waiting in kelve". Home shows what's waiting, and says so in your agent's words: "I'll look into it when I finish this." If your agent isn't running, nothing is lost: it all waits.

#### The format: plain markdown

Your agent writes the debrief as one markdown file, because markdown is what agents write best. kelve renders it, and adds the actions, statuses and notes on top. Nothing in the file is markup that runs: no HTML, no images, and no links except to sources.

````markdown
---
kelve: debrief/1
title: Retry failed downloads with a growing wait
---

## What it did

When a download fails because of the network or the server, fetch now
tries again, up to 5 times, waiting longer each time.

## How it works

Every download now goes through `with_retries()`, in `src/retry.rs`.
When a request fails, it asks `should_retry()` whether the failure might pass:

- timeouts and dropped connections: **yes**
- anything that says the request is wrong, 4xx: **no**

## Decisions

### Retry only failures that might pass next time
- from: its own call
- lines: src/fetch.rs 59-66
- could have: retried every failure; retried 429 too
- rests on: HTTP status codes, Idempotence

Timeouts, dropped connections and server errors often clear up by
themselves. A 4xx means the request itself is wrong.

## What it rests on

- **Exponential backoff:** waiting longer after each failure: 1, 2, 4, 8 seconds.
````

- **Each decision is a `###` heading under `## Decisions`,** followed by a short list of fields, then why, in prose:
  - `from`: its own call, what you said, or your edit;
  - `lines`: where it is in the change, as a path and line ranges;
  - `could have`: the other ways;
  - `rests on`: the ideas it uses;
  - `part`, on a large change: which part of the change it belongs to, so kelve can group decisions.
- **kelve turns the fields into the page's margin notes and actions,** and never shows them as a list. Every decision gets Agree, Do it differently and Look into it, and Show the code with its own lines.
- **kelve checks the file before it shows it:**
  - every decision's lines exist in the change;
  - `from` is one of the three;
  - the markdown uses only headings, paragraphs, lists, tables, code, bold, italics, code spans and links to sources.

  A debrief that fails a check goes back to your agent once, with what to fix. If it fails again, home says so, with **Ask it again**.
- **Agree with the rest** answers every decision you haven't answered yet, so a long debrief never becomes a list of clicks.

**While it's written**, the debrief shows what your agent is doing (reading the files, finding the decisions, writing how it works) and the robot working. If your agent is busy with something else, it says the debrief waits until your agent is free, and that it'll be on home when it's ready. **Once you send it**, you see exactly what happens next: what goes into which skill, any new skill drafted, any research still running, and any part you can check.

#### Research

**Look into it** turns a decision into a question for your agent, which you can edit before you send it.

1. kelve queues a `research` job, holding the decision, its lines of code and your question.
2. Your agent takes it the next time it's free, or straight away if you tell it to do kelve's research.
3. It answers in a fixed shape: a short answer, the ways it compared (each with one thing for it and one against), the one it suggests, and where it looked.
4. The answer appears under the decision it's about. While it's running, the decision says so. On home, that debrief shows **Research ready**. Research never gets a line of its own on home.
5. From the answer, you can have your agent redo the work that way, add it to a skill, or leave it.

The answer is kept with the debrief, in kelve's store.

It answers in markdown too, in a fixed shape:

````markdown
#### What your agent found

Retry **429** too. It means the server is busy, not that the request
is wrong, and it usually says when to come back.

| Option | For | Against |
|---|---|---|
| Leave 429 alone | Nothing to change | A busy server stops the download |
| **Retry 429, as Retry-After says** | Does what the server asked | A few more lines |

**It suggests** retrying 429, waiting at least as long as `Retry-After`.

Sources: [RFC 6585](https://www.rfc-editor.org/rfc/rfc6585), [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110)
````

kelve renders it under the decision, with the actions underneath.

#### Explain

Every idea under **What it rests on** has **Explain**. It queues an `explain` job, holding:
- the idea's name;
- the lines in this change that use it;
- what `marks.md` says you already know.

Your agent writes a short explanation in markdown, at most about 200 words, using this change as the example. The explanation appears under the idea, and kelve keeps it, so the same idea isn't explained twice for the same code.

**I know this** marks the idea as known in `marks.md`, so it's no longer new to you anywhere: in debriefs, in research, or in explanations.


### 3. Your skills

Skills are standard skill folders, following the [Agent Skills format](https://agentskills.io/specification), so every agent that reads skills can use them. There are three kinds, because they reach your agent in different ways:

| Kind | What it holds | How your agent gets it |
|---|---|---|
| **How you like it** | Your defaults for a kind of work: Rust, the words in an app, docs, or everywhere | kelve's hook hands it over whenever your agent touches that kind of work, so it never depends on the agent remembering to load it |
| **How you do something** | A procedure for a task: adding a command-line flag, retrying network calls, releasing a version | Its description says when it applies, and the agent loads it when the task matches, in any language and any project |
| **One project** | What only matters in one repository: its ports, its release script, its odd corners | Kept in the project, in `.kelve/skills/`, for anyone who works on it, if you choose to share it |

**One skill is one folder.**

```
rust/
  SKILL.md              what the agent reads every time: short, under 150 lines
  references/
    errors.md           detail it reads only when it needs it
    tests.md
```

`SKILL.md` has the format's front matter (a `name`, and a `description` that says when to use it), then its parts.

**A part is a `###` heading and everything under it, up to the next heading.** The heading is the rule itself, in a few words. Under it go a sentence or two on why, and an example or steps only where they're needed. That's what kelve checks, and what each note in the margin belongs to. To add a part by hand, you write a `###` heading; kelve sees a new part, not checked yet. Anything that isn't under a `###` heading isn't a part: it's shown, but never checked.

Parts sit under at most three sections:

- **`## How you like it`:** your defaults.
- **`## Steps`:** a procedure, in order.
- **`## Watch out for`:** mistakes your agent made before, so it doesn't make them again.

An example goes inside the part it shows. Longer detail goes in `references/`, linked from `SKILL.md`, so the agent loads it only when it needs it.

**The files stay clean.** Where each part came from, and what its checks found, are kept in kelve's store, not in the file. The file is what any agent reads, in any tool.

```markdown
---
name: rust
description: How Sam wants Rust written. Use when reading, writing or reviewing Rust.
---

# Rust

## How you like it

### Errors are one enum, with a message for each
Every way a function can fail is a variant, and Display turns it into a
sentence a person can act on.

### Bad arguments exit 1, the tool's own failures exit 2
So scripts can tell "you called it wrong" from "it broke".

## Watch out for

### Fix clippy warnings, never quiet them
Change the code itself. Never add #[allow] to make a warning go away.

## More

- [Error messages in full](references/errors.md)
```

- **They're yours.** The files are the source of truth. You can edit them, and kelve rereads them.
- **They're private.** They go nowhere but your agents, and, only when you ask for a verified score, kelve's hosted service for that run.

**The skills page** lists skills in four groups: suggested, how you like it, how you do something, and one project. Each skill has two views:
- **Rendered:** the skill as a document, in the same style as the debrief: the markdown rendered, with where each part came from and what its check found in the margin, and actions only under a part that needs one.
- **Markdown:** the `SKILL.md` itself, drawn as kelve draws code, with each part's check result in the margin.

#### What makes a part worth keeping

A part earns its place only if it tells your agent something it wouldn't do on its own:
- a default that isn't the model's;
- a step it skips;
- a mistake it makes.

Everything else is dead weight. It costs reading, and it crowds out the parts that matter.

- **It comes from you.** Skills an agent writes for itself, from what it already believes, make it worse. Every part is what you said, kept or changed. The `learn` job phrases it; it never adds what it thinks is good practice.
- **It's general, not a copy of one piece of work.** No paths, names or values from the work it came from. Templates with fixed paths override the task in front of the agent and make it worse. An example from your code is fine, shown as an example.
- **It has three pieces:**
  - a `###` heading that is the rule itself;
  - a sentence or two on why, because an agent that knows why applies a rule in cases the rule didn't name;
  - an example, only where the words aren't enough.

  A procedure is steps in order, ending with how to check it's done.
- **It's checked** (below). A part that makes no difference twice is offered for removal.

#### How big a skill gets

- **Short beats complete.** `SKILL.md` stays under 150 lines, and kelve checks that in code, not by asking the agent. Detail the agent needs only sometimes goes in `references/`.
- **When a skill reaches its limit,** kelve proposes, in this order:
  1. removing parts that make no difference;
  2. moving long examples into `references/`;
  3. splitting it by when it applies. For example, Rust becomes Rust, plus Rust tests, which the agent gets only when it works on tests.

  A skill is never split by topic alone: two skills the agent always gets at the same moment are one skill.
- **Few skills in play at once.** A handful help most. Many, especially similar ones, and the agent picks the wrong one or none. So defaults never depend on the agent choosing: kelve's hook hands over the one skill for each kind of work. Only procedures are chosen by their description, and kelve keeps their number small.

#### Where something new goes

kelve decides, and the debrief shows its choice: "Adds to Rust", with a way to put it somewhere else. You never have to choose between making a skill and adding to one. kelve asks three questions, in order:

1. **Is it true only in this repository?** It goes in the project's skill.
2. **Is it steps for one task, and has that task come up in at least two separate pieces of work?** It goes in the "how you do something" skill for that task, or kelve suggests a new one. Until a task has come up twice, its lines go where a default would.
3. **Otherwise, it's a default.** It goes in the skill for the kind of work it was caught in: Rust, TypeScript, the words in an app, docs. If it was caught in more than one kind, it goes in Everywhere.

**Kinds of work are kelve's, not made up each time.** kelve has a fixed list, matched by the files a change touches, so a person has one Rust skill, not "rust", "Rust CLIs" and "rust-style".

**Before suggesting a new skill,** kelve compares its description with every skill you have. If one is close, it suggests adding to that one instead. That's how duplicates are stopped before they start.

#### How a skill grows

- **Adding is an edit, not a line on the end.** The `learn` job gets the whole skill and puts the new part where it belongs, next to the parts it's about.
- **If it says what a part already says,** the file doesn't change. The part gets one more piece of evidence in kelve's store.
- **If it disagrees with a part,** the debrief shows both, and you choose. Two parts that disagree are never both kept.
- **Your own edits to the file win.** kelve rereads it, and never changes them back.
- **A changed part is checked again.** It replaces the old wording only if it does clearly better. A tie keeps what was there.

#### When kelve suggests a new skill

The `learn` job proposes, and kelve never makes a skill without you. It suggests one when:

- **a task has come up in at least two separate pieces of work,** with steps you approved or corrected, and no skill you have is close. For example, retrying network calls: a new "how you do something" skill;
- **a line only matters in one repository:** it moves to that project's skill;
- **a skill has reached its limit:** it's split by when it applies, as above.

A suggestion shows up in two places:
- **In the debrief,** under For your skills. You see why it's suggested, a preview of its parts, and three choices: **Create the skill**, **Add to** an existing skill **instead**, or **Not now**.
- **On the skills page,** at the top, under Suggested. The draft is shown in full, with **Create the skill**, **Change it first** or **Not now**.

### 4. Each part is checked

A part of a skill earns its place by changing what your agent does. A **skill check** proves it on your own past work: the request, where the work started, and what you finally kept. Your agent redoes a past task twice, as two subagents (kelve never starts your agent), once with the part and once without.

| What the check found | What you see | What you can do |
|---|---|---|
| Not checked yet | Not checked yet | Check it |
| Running | Checking now, with how many past tasks are redone | Wait, or stop it |
| It changes what your agent does | Works | Nothing: it stays |
| Your agent does it anyway | Makes no difference | Remove it, or keep it anyway |
| Your agent still got it wrong, with the part | Failed, with what happened and a clearer wording from your agent | Use that wording, write your own, or remove it |

Checks cost runs of your agent, so kelve batches them, says roughly what a check will use before it starts, and runs them only when you agree. A check waiting for you shows on home as **Needs you**.

### 5. Your agents get them

- **Through kelve's hook,** at the moment they help: your everywhere skill and the project's at the start and as you send a message; a kind's skill when the agent first reads or edits that kind of file.
- **Through MCP:** a new tool, `skills`, in case the hook didn't run.
- **Into each agent's skills folder,** with your yes, as `kelve setup` already asks for the hook. That way your skills work even where kelve isn't running, and in any agent that reads skills.

### 6. Proof: two scores

**Fits you.** On your own computer, on your past work. Your agent with your skills against a standard agent, same model, same tasks. You pick the one you'd rather keep, blind, and kelve counts how close each came to what you kept. It's yours alone, and it's the number that tells you your skills are working.

**Verified.** On kelve's hosted service, on shared tasks nobody sees in advance. This is the number you can show anyone.

1. You choose a kind of work, an agent and a model.
2. Your skills go to the service, with secrets and paths stripped first.
3. The service runs the standard agent and the agent with your skills, on the same model, on secret tasks of that kind. Hidden tests check each run, and a fixed judge scores the rest.
4. It publishes the gain: "beats standard Sonnet 5.5 on 9 of 12 Rust tasks", dated.

**A score is of your agent, not of a skill.** All your skills go, as your agent has them, and kelve's hook hands them over in the run as it does at home. The kind of work you choose only picks the tasks. So what you call your skills changes nothing: two people with differently named skills run the same tasks, scored the same way. It also tests what matters: whether your skills reach the agent and help, not whether one file reads well.

**What you're told about the tasks:** everything but the tasks themselves. For each kind of work:
- what the tasks are like, for example "Rust: command-line tools, libraries, and fixing bugs in code that already exists";
- how many there are, and how they're scored: the hidden tests, and what the judge looks for where tests can't tell;
- the standard agent's own result;
- two retired tasks in full, as examples.

The live tasks are never shown. They're swapped regularly, and the service turns away skills that name them. Skills raise the score only by making your agent better at that kind of work, so there's nothing to gain by aiming at the tasks.

**The terms your skills go under:** used only for that run, deleted after it, never read by a person, never used to train anything. The model provider's API is used on terms with no training and no keeping.

**Your page** is where it's shown: your character, and your verified scores beneath it. It never shows your skills, your tasks or your work. There's no way to share or install someone else's skills, and there won't be: the point is that they're yours. How the page and the service work is in "The hosted service and pages", below.

### 7. Your agent, as a character

Your agent has a pixel character, shown in kelve and on your page. It's there to make your agent feel like your own, and to give people something to recognise. It's yours to design, and nothing about it is earned or tied to your skills.

**One framework for every character.** Every character is drawn on the same grid, 24 pixels wide and 27 tall, in layers, and moves the same way:

| Layer | Needed | How kelve moves it |
|---|---|---|
| `base` | Yes | Bobs while it waits |
| `eyes` and `shut` | Yes | Blinks |
| `light` | No | Glows, faster while it works |
| Arms, down and up | No | Waves and cheers |
| `legs` | No | Stays still while the rest bobs |

Its moves are waiting, working, pleased, hello and asleep. All of them stop for anyone whose computer asks for less motion.

**A robot to build, or anything else.** kelve ships a robot made from parts: head, antenna, hat, eyes, mouth, ears, body, chest, hands and legs, in four colours you choose. Your agent can also draw:
- **a part** for the robot (`kelve-part/1`): a few rows of letters, one letter for each colour, and where the part goes;
- **a whole character** (`kelve-character/1`): every layer, in the same way.

Colours can follow the ones you chose, so a part always matches. kelve checks each one before it shows: pixels only, within the grid, in a few colours, with no words or pictures from anywhere else. Before anything an agent made shows on your public page, kelve's service checks it again. The format is documented for agents to read, so "make me a cat character" just works.

**Its name** is "Sam's agent" until you give it one, if you do. Nothing else about the agent is shown: not which agent it is, not how long it's been learning.

## Rules that change

kelve's rules (`AGENTS.md`) change in four places. They need deciding and writing before any of this is built.

- **What leaves.** Today: knowledge goes out, work never does. New: nothing of yours leaves, except your skills, to kelve's hosted service, for a verified run you asked for, under its terms. Secrets and paths are stripped first. Your work, messages, edits and tasks never leave.
- **Models and keys.** Today: kelve has no model and no key of its own. New: the app still has none, and every job is your agent's. The hosted service runs agents on its own keys, for benchmarks only.
- **The shared graph.** Goes. Nothing goes to it, and nothing comes from it.
- **Signing in.** Stays, for one thing only: your account on the hosted service, for verified runs and your page.

## What we keep from kelve today, and what we don't

| Part of kelve today | In this version | Why |
|---|---|---|
| The local service, settings, updates, problem reports | **Keep** | The base everything runs on |
| Pairing your browser with kelve | **Keep** | Stops other websites and programs reading through kelve's local address |
| The job queue and the agent's list | **Keep** | Every new job runs on it |
| `crates/graph-api`, the client and stand-in for the hosted service | **Keep, changed** | Gains the benchmark's and the page's API; the graph's goes |
| `kelve mcp`, `kelve setup`, the installer | **Keep, changed** | One new tool, `skills`. Setup also offers to put your skills in each agent's skills folder |
| The hook (`kelve moment`) | **Keep, changed** | Catches corrections as you send messages, delivers skills, and knows which worktree is a test run |
| Showing work as it's finished (handoff) | **Keep, changed** | Opens the debrief, and now records what was handed over, to compare with what you commit |
| Notes for the agent | **Keep** | "Redo it now" sends one |
| Reading changes from git, reading files with secrets masked, fencing | **Keep** | The material for every job |
| `marks.md` | **Keep** | Says which ideas are new to you, when research explains them |
| The store, SQLite | **Keep, changed** | New tables for candidates, lines, tests and scores |
| Reading jobs, the way in, reading plainly | **Drop** | The debrief needs none of it |
| Signing in to the hosted service, with GitHub or Google | **Keep, changed** | Now for your account, verified runs and your page |
| Entries, relations, the shared graph, contributor terms | **Drop** | The shared graph goes |
| Lessons | **Drop** | Skills do this job better, and stay private |
| In the web app: the change page, file view, the agent's list, settings | **Keep, changed** | The change page becomes the debrief |
| In the web app: the folder page, margins, entries, Knowledge, search, Adding | **Drop** | All serve reading files and entries |
| New in the web app | **Add** | Skills, Fits you, your verified scores, and your agent's character |

Build alongside today's kelve, switch the change page over once the debrief works, and remove what's marked **Drop** in one clear-out once the next experiment says build it.

## The jobs

Five, all new, all on kelve's existing queue. Each answers in markdown where a person reads the answer, so kelve can render it the same way everywhere. Your agent takes them, and kelve checks each answer's shape before it takes it.

- **`debrief`**, one per handover. Material: the change, fenced, secrets stripped; your skills that applied; the project's instruction files. Answers with one markdown file in the format above: what it did, how it works (one sentence, or a step for each part), every decision that matters (where it came from, what it chose, why, what else it could have done, the part it belongs to, and its lines), which skills applied and whether each held, and the ideas the work rests on.
- **`learn`**, when there's something to learn from. Material: a correction and what the agent had just done, your edits to what it handed over, or what you kept in debriefs; and the whole skill each belongs in, so an addition is an edit in its place. Answers with additions to existing skills, each a part under its `###` heading with its evidence, phrased as a default, a step or a mistake to watch out for, never a fix to particular lines. And, when it's worth it, a new skill to suggest, drafted in full.
- **`research`**, queued by you from a decision in a debrief. Answers with a short answer, the ways it compared, the one it suggests, and where it looked.
- **`explain`**, queued by you from an idea in a debrief. Material: the idea's name, the change's lines that use it, and what `marks.md` says you know. Answers with a short explanation in markdown, using the change as its example.
- **`compare`**, for skill checks and for Fits you. Your agent runs a past task twice, as two subagents in two fresh worktrees, and hands in both.

## The hosted service and pages

kelve's hosted service already runs the shared graph, at `graph.kelve.ai`, from a private repo. The benchmark and the pages are built into that same service, on the same infrastructure, so most of what they need is already there. Its code, and how it's hosted, stay private; this repo gets its API's shapes, the one client for it, and a stand-in for tests, as it does for the graph today (`crates/graph-api`, `docs/graph-api.json`).

### What it already has, and keeps

- **Accounts, and signing in** with GitHub or Google, from a person's kelve, with one key per computer. Your page belongs to that account.
- **Deleting an account,** which now takes the page, the character and every score with it.
- **Calling models** from any provider, priced call by call, with spending limits. The judge and the runs use it.
- **Measuring its judges** against cases with known answers, mixed in with real work, so a judge that drifts stops counting.
- **Background work** that any number of workers share, retried when it fails.
- **Rate limits, the operator's brake, and anonymous counts**, as the graph has them now.
- **Its environments:** development, staging and production, each with its own database.

### What's new in it

**The task bank.** For each kind of work, realistic, self-contained tasks: a starting folder, the request, hidden tests where the work can be tested, and what the judge looks for where it can't. Never published. Tasks are swapped out regularly, and each score names the version of the bank it ran on.

**Standard runs, kept.** The standard agent is the same for everyone, so its runs are done once for each task, agent and model, several times over, and kept. A person's verified run then only needs the agent with their skills, which halves its cost. When a new model or agent version comes out, the standard runs are done again on it.

**A run, step by step:**

1. kelve sends all your skills, with secrets and paths stripped, with the kind of work, agent and model you chose. The service checks them as it does an entry today, and turns away skills that name tasks from the bank.
2. Each task runs in its own sealed container: the agent, run without anyone at the keyboard, with your skills in its skills folder and kelve's hook handing them over as at home, the task's starting folder, and a limit on time and spend. It can reach the model provider and nothing else.
3. When the agent's done, the hidden tests are put in for the first time, in a fresh container, and run. The judge scores what the tests can't.
4. Each task runs more than once, to smooth out chance. The gain is how many tasks your agent did better than the standard one, and the hidden tests each passed.
5. Your skills are deleted as the run ends. They're never logged, never kept, and never read by a person.

**What it costs, and who pays,** is still open (below). The service says what a run will use before it starts, and an account has a limit a day.

### The pages

**Your page,** at `kelve.ai/@yourname`, shows:

- your character, animated, and its name;
- for each kind of work, one square for each task, coloured for better, the same or worse than a standard agent, and the result as "10 of 12";
- under each, the model it ran on and the month, and nothing else about the agent.

**Each score has its own link,** for sending to someone: what was run (kind, agent, model, task bank version), when, and the result. Anyone can open it and see kelve ran it.

**Links preview well.** A shared link shows a card: your character, its name, and its headline gain, made by the service as an image.

**You decide what's public.** Nothing is public until you publish your page. You choose which scores show, and you can take the page down at any time.

**Your name** on the page is checked for length, anything offensive, and a secret or path pasted by mistake, before it's shown.

### What the service stops doing

Everything that serves the shared graph: ideas, entries, relations, lessons, the reading tokens and the contributor terms. It's retired in its own clear-out, after the app's, once the app no longer asks for any of it.

## In, and out for now

In: any text kelve reads (code, Markdown, config, CSV), and any agent kelve's hook supports.

Out, for now:

- Teams, and companies setting their own tasks.
- Files kelve can't read as text: Word, Excel, design files.
- Sharing, publishing or installing skills between people. Never.

## The experiments

### Done: is a briefed agent better than a standard one?

Six tasks in different kinds of work (a Rust command-line tool, a TypeScript config loader, a README, the words for a "Delete project" screen, a Rust parsing library, a React component), written by an agent that never saw the person's taste. Each was done five times on the same model: twice standard, twice with the person's brief, and once with a popular generic skill. A judge per task, blind to which run was which, debriefed every decision against the person's full taste, scored quality and ranked the runs.

| | Standard | Briefed | Generic skill |
|---|---|---|---|
| Decisions the person would change, per run | 7.7 | 4.3 | 7.5 |
| Brief's preferences followed | 67% | 92% | 65% |
| Unwritten preferences followed | 70% | 77% | 68% |
| Mean rank, of 5 | 3.9 | 1.6 | 4.0 |

Briefed beat standard in 23 of 24 head-to-heads, and the generic skill in 12 of 12.

What to be careful of: every run could see kelve's own `AGENTS.md`, which repeats five of the brief's preferences, so the standard runs were partly briefed and the real gap is likely larger. Six tasks, one judge each, and the judges were models. The React component wasn't compiled.

### Next: does kelve's loop make better skills, and do they raise the verified score?

Two questions, run together, outside kelve's own repo so its `AGENTS.md` can't leak in.

1. **Does catching beat writing a skill in one go?** A simulated person with real expertise a model lacks: a house API with its gotchas, a release process, conventions. They do several tasks with an agent, correcting it along the way and editing what it hands over. Then a skill is made three ways: written in one go from a description; made by kelve's loop, from the corrections and edits, tested and pruned; and none. All three are compared on new tasks.
2. **Do skills raise the score others can check?** The new tasks include shared ones with hidden tests, not only fit to the person.

Decided before running:

- **kelve's loop works** if its skill has at least 30% fewer decisions to change than the one written in one go, and passes at least as many hidden tests.
- **The verified score is worth building** if skills raise hidden-test passes by enough to show on a page: at least 2 more tasks in 12.
- If skills help fit but not hidden tests, Fits you is the product and the verified score needs tasks that reward expertise, not general skill.

## Building it

Small pull requests, each one usable. In this repo:

1. **Skill files:** scopes, reading and writing, and the `skills` MCP tool.
2. **Delivery:** the hook adding skills on the agent's events, and setup offering each agent's skills folder.
3. **Catching:** corrections from the hook, what was handed over against what's committed, and the `learn` job.
4. **The debrief:** the job, and the page in place of the change page.
5. **`research`,** queued from the debrief, answered in place.
6. **Skill checks** with `compare`, on past work, and their five states.
7. **Fits you,** blind, kept over time.
8. **The character in kelve:** the pixel framework, the robot's parts, how it moves, and parts and characters made by your agent.
9. **The hosted service's new API** in `crates/graph-api`, with its stand-in, and sending a verified run from kelve.
10. **Publishing your page** from kelve: what shows, and taking it down.
11. **The clear-out:** everything marked **Drop**.

In the hosted service's own repo, alongside:

1. **The task bank** for one kind of work first, with its hidden tests and the judge's cases with known answers.
2. **The runner:** sealed containers, the agent run unattended, hidden tests after, the judge.
3. **Standard runs,** kept for each task, agent and model.
4. **Verified runs** for an account, with limits and costs.
5. **The pages:** your page, a score's own link, and the link preview with your character.
6. **Retiring the graph's work,** once the app no longer uses it.

## Open questions, not needed to start

- Who pays for verified runs, and how many are free.
- Which agents and models can be scored. Both runs must use the same, or the gain means nothing.
- The exact terms, word for word, before anyone sends a skill.
- Which kinds of work are on kelve's list first, and how a change that touches several kinds is matched.
- How close a draft's description must be to an existing skill's before kelve suggests adding to it instead.
- Where the line for a debrief sits. About 20 lines of code is a first guess, to tune on real work.
- Skills that add capability fade as models improve; skills that hold preference last. How the page shows that fairly.
- A score proves a setup, not a person. How much of the record behind it (debriefs, how long) the page needs to make that clear.
- What happens when a skill disagrees with a project's own `AGENTS.md`.
- Whether people can share a character they or their agent made, when skills themselves are never shared.
- Whether people will want more than one character, such as one for each kind of work, and how scores split between them.
- Which kind of work the task bank starts with. One kind done credibly beats several done thinly.
- Which agents the runner supports first. Each needs to run unattended, in a container, with a skills folder.
- How many times each task runs, for a gain that's steady from one run to the next without costing too much.
- What the hosted service is called, now it's no longer a graph, and whether it moves from `graph.kelve.ai`.
