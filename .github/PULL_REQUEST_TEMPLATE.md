## What changed, and why

## How you checked it

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test --all --all-features --locked`
- [ ] `cd web && bun run typecheck && bun run build`, if the app changed
- [ ] `cd web && bun run e2e`, and `bun run looks` for anything people see
- [ ] Screenshots, for anything people see

## What an agent decided

If an agent wrote some of this, say what it decided on its own: the choices nobody asked it to make.

Commits are signed off (`git commit -s`), under the [Developer Certificate of Origin](https://developercertificate.org/).
