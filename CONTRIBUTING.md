# Contributing to kelve

Thanks for wanting to help. kelve is small on purpose: every change should make it simpler to understand or more useful to the people it's for. Everyone taking part follows the [code of conduct](CODE_OF_CONDUCT.md).

## Before you start

- For anything bigger than a fix, open an issue first and say what you want to change and why, so nobody spends a week on something that won't land.
- Read [AGENTS.md](AGENTS.md). It's written for agents, but its rules are the project's rules: how the code is laid out, what keeps it sound, how to write for people, and how commits look.

## Running it

You need Rust (stable) and [Bun](https://bun.sh); for the browser tests, Playwright's Chromium and WebKit, and for the screenshot tests, Docker. [Developing kelve](docs/developing.md) says how to run it from source beside a kelve you use, with a stand-in shared graph and a stand-in agent that cost nothing.

## Checks

Everything CI runs:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all --all-features
cd web && bun install && bun run typecheck && bun run build
cargo build -p kelve && cargo build -p kelve-graph-api --features stand-in --bin kelve-graph-stand-in
cd web && bun run e2e
cd web && bun run looks
```

`cargo test` writes the web app's types (`web/src/api/gen`) and the shared graph's API document (`docs/graph-api.json`) from the Rust: commit them with the change. After a change meant to show, take the screenshots again with `bun run looks --update-snapshots`, look at each that changed, and commit them. After changing a dependency, run `bun run notices` in `web/` and commit `THIRD_PARTY_NOTICES.md`.

## Pull requests

- One change per pull request, with tests for what it changes.
- Say what changed and why in plain words. Screenshots for anything people see.
- Agents are welcome to help, and you answer for what they wrote: you should be able to explain every line without them. Say in the pull request what an agent decided on its own.
- Sign off your commits (`git commit -s`), certifying the [Developer Certificate of Origin](https://developercertificate.org/): that you wrote the change, or have the right to contribute it.

## Licensing your contribution

kelve is open source, under the [Apache License, Version 2.0](LICENSE).

By contributing, you license your contribution under the same license, as section 5 of the license says. You keep the copyright in what you wrote.
