# Changelog

What changes in each release of kelve, for the people who run it. Versions follow [Semantic Versioning](https://semver.org): until 1.0, a new minor version can change anything, and its notes say what.

## 0.1.2, 30 October 2026

- **Your agent shows you its work as it hands it over.** When it commits, pushes or opens a pull request for work you haven't seen, kelve's hook has it open that work in kelve, without being asked, once for each commit. It used to wait until the agent merged, which is after work is reviewed.
- **Setup's choices take the arrow keys,** and a command in backticks is never split across lines.
- **Setup puts a stale connection right.** An agent connected with kelve settings of its own, such as another graph from developing kelve, is connected afresh, and setup says so.
- **A note done stays done.** Marking a note done took its jobs off your agent's list a moment later, which could take off the job asked when the note's words changed back. It's done before kelve says so now.
- **`kelve update` on Windows waits out a busy file.** Windows holds a program just written while Defender reads it, and putting it in place then could fail; kelve tries again for up to two seconds.

## 0.1.1, 30 October 2026

- **The command line keeps its secret safe.** It sends it over https anywhere, and over plain http only to this computer; given a plain http address elsewhere in `KELVE_URL`, it says what to do instead.
- **What an agent learned is asked for, even on a fast machine.** A command that passed in the same millisecond as the failure before it now counts as getting past it, so the turn's end asks what was learned.

## 0.1.0, 30 October 2026

The first release.

- **Read it in order.** Your agent finds the way in to what it opens, a change, a repo or a folder: what it is, then each file in the order that lets the next build on it, and reads along the way.
- **Every part, in plain words.** A file's page says what it's for, then each part by its goal, in the order things happen, with what it did before. kelve checks every part against the lines it explains.
- **Anything your agent opens:** code or words, a repo, your notes, one file, or just what your agent did.
- **Every idea, down to first principles.** Each idea is marked beside the lines that use it, and its entry opens beside the work, from the shared graph, leading to the ideas it rests on.
- **Lessons.** What an agent learns the hard way is checked by the shared graph and given to the next agent as it works; kelve shows which it was given, and whether each held.
- **Your agent does the reading,** on the model and plan you already have: kelve has no model and no key of its own. Claude Code and Codex show you their work before they merge it.
- **Notes,** just for you, kept with the work, or for your agent to pick up the next time you message it.
- **Installing and updating:** `kelve.ai/install.sh` and `kelve.ai/install.ps1`, which check what they download against the release's checksums, and `kelve update`.
