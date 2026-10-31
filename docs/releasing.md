# Releasing kelve

kelve's first version is the `kelve` command, with the web app inside: it runs kelve in the background and opens it in the browser.

A release is a version tag. Pushing one runs `.github/workflows/release.yml`, which runs CI on the tagged commit first, and only once it passes builds the command for each platform, with the web app inside, and attaches it to a GitHub Release:

- `kelve-macos-arm64.tar.gz` and `kelve-macos-x86_64.tar.gz`;
- `kelve-linux-x86_64.tar.gz` and `kelve-linux-arm64.tar.gz`;
- `kelve-windows-x86_64.zip`;
- `SHA256SUMS`.

## Cutting one

1. Set the version in `Cargo.toml` (`[workspace.package]`). The release refuses a tag that says otherwise.
2. Commit, then tag and push:

   ```bash
   git tag v0.2.0 && git push origin v0.2.0
   ```

The release stays a draft until every platform's build is attached, then it's published.

## Installing it

On macOS and Linux, `install.sh` installs the newest release's build for the computer it runs on, checked against `SHA256SUMS`, into `~/.local/bin`:

```bash
curl -fsSL https://kelve.ai/install.sh | sh
```

On Windows, `install.ps1` does the same in PowerShell, into `%LOCALAPPDATA%\kelve\bin`, and adds that folder to the person's `PATH`:

```powershell
irm https://kelve.ai/install.ps1 | iex
```

Then each runs `kelve setup`, which connects the person's agent, adds kelve's hook to its settings, and sets how much kelve marks: in a terminal it asks, and run by an agent it leaves the agent one word to choose.

Both need the repository and its releases to be public. `KELVE_INSTALL_DIR` puts `kelve` somewhere else, and, for `install.sh`, `KELVE_RELEASES` installs from another address, such as the release `bun dev/kelve.ts newcomer` serves. Once a release is out, the release workflow installs it with each script, on macOS, Linux and Windows, and checks the version it says.

A command installed this way isn't marked as downloaded from the internet, so it needs no signing: macOS's Gatekeeper and Windows' SmartScreen check what a browser downloads. A `kelve.exe` downloaded in a browser, from the release page, may get a SmartScreen warning until it's signed.

kelve.ai serves both scripts: each deploy of the site copies them from kelve at the commit it builds against, so a change to either reaches people once the site deploys again. Run on Windows in Git Bash, MSYS2 or Cygwin, as an agent told to install from `kelve.ai/install.sh` may, `install.sh` hands over to `install.ps1` in PowerShell; `KELVE_INSTALLER` gives it another `install.ps1` to run.

## Updating

`kelve update` updates kelve in place: it finds the newest release where the installer does, downloads this computer's build, checks it against `SHA256SUMS`, and puts it in place of itself. A kelve that's running on an older version stops once the work it's doing is done, and starts again on the new one. Running the installer again still works, but leaves a running kelve on the old build until `kelve stop`.

An agent's `kelve mcp` outlives both: it's the version it was when the agent started it until the agent restarts it or the person reconnects it. The first of its tools the agent calls after an update says so, once, so the agent can tell the person. Installed afresh under it, it reads kelve's new secret when its own is turned away, and starts kelve from the `kelve` on the PATH when the command it was started from is gone.

kelve asks GitHub for the newest release once a day, in the background, and says when there's a newer one: in settings, and to the person's agent, once for each version, which asks them whether to update and, on a yes, updates kelve with its `update` tool. `KELVE_UPDATES=off` stops the asking.

What kelve says of a release is the first line of its notes that isn't a heading: the line the person and their agent see. The release workflow writes the notes from the pull requests; once it's published, edit them on the release's page to put one plain sentence first, on what's new for the person. kelve asks once a day, so do it soon after.

Each release's migrations are new to stores made before it: kelve copies the store aside before running them (`kelve.db.before-<time>`, the last two kept), and an older kelve refuses a store a newer one has migrated, saying to run `kelve update`.

## Trying it before a release

The same build as a release's, on this computer, from the repo root:

```bash
cd web && bun install && bun run build && cd ..
cargo install --path crates/cli --locked
```
