# Running kelve

kelve runs on your own computer: the `kelve` command starts it in the background and opens it in your browser. Installing it is in [releasing.md](releasing.md); this page lists its settings.

## Settings

kelve reads these from the environment as it starts. Everything else is set in the app.

| Variable | What it does |
|---|---|
| `KELVE_URL` | Where you open kelve. Links are made from it, and kelve answers at its host. Default `http://kelve.localhost:4600`, or `kelve.localhost` on the port in `KELVE_BIND`. Browsers send any name under `.localhost` to the computer they run on. kelve also answers at `localhost` on its port. The command line sends its secret only over https, or over plain http to this computer. |
| `KELVE_BIND` | The address kelve listens on. Default `127.0.0.1:4600`. |
| `KELVE_DATA` | Where kelve keeps its files: its store, its sealing key, its log and reports of what went wrong. Default the folder the secret is in: `~/.config/kelve`, or `kelve` under `XDG_CONFIG_HOME`, or under `%APPDATA%` on Windows. |
| `KELVE_STORE` | The store's file. Default one in the data folder for each shared graph: `kelve.db` for kelve's own, `kelve-<host>-<port>.db` for any other, such as `kelve-localhost-4620.db`, and `kelve-off.db` for none. |
| `KELVE_SECRET_KEY` | 64 hex characters (`openssl rand -hex 32`). Seals the shared graph's key before it's kept. When it isn't set, kelve makes one the first time, in `KELVE_DATA/secret.key`. |
| `KELVE_SECRET_FILE` | Where the secret the command line and your agent's tools use lives. Default `~/.config/kelve/secret`, or under `XDG_CONFIG_HOME`, or `%APPDATA%` on Windows. |
| `KELVE_UTC_OFFSET_MINUTES` | Your time zone, as minutes from UTC, like `60` or `-300`: what "this week" means for the weekly count. Default: your computer's own. |
| `KELVE_GRAPH_URL` | The shared knowledge graph. Default `https://graph.kelve.ai`; `off` uses none. Each graph has its own store, so its own sign-in and what kelve kept from it: change this and restart kelve to change graphs, and change it back to find everything as it was. A build can set its own default with `KELVE_DEFAULT_GRAPH_URL` as it's built. |
| `KELVE_GRAPH_KEY` | A key for the shared graph, in place of signing in to it from the settings. It wins over signing in. Without either, kelve doesn't read the graph or add to it: your agent writes every entry for you alone. |
| `KELVE_WRITE_REAL_GRAPH` | `1` lets a kelve built from a checkout, rather than a release, add to the real shared graph. Without it, one signed in only reads it. |
| `KELVE_CLAUDE_DIR` | Claude Code's folder, whose `settings.json` gets kelve's hook when you turn it on. Default Claude Code's own `CLAUDE_CONFIG_DIR`, or `~/.claude`. |
| `KELVE_AGENTS_HOME` | The home folder other agents' settings are in, for kelve's hook in them. Default your home folder. |
| `KELVE_LESSON_HOLD` | How long, in seconds, a lesson your agent wrote waits before it goes to the shared graph, so you can stop it. Default `86400`, a day. Shorter, such as `30`, for trying lessons out. |
| `KELVE_HOLD_BACK` | How often a lesson that fits is held back from your agent, to measure the lessons, in thousandths, once it's been given a hundred times across everyone. Default `50`, one in twenty. `0` for never, as the sandbox and the tests use. |
| `DO_NOT_TRACK` | Set to anything but `0` or `false`, kelve never sends anonymous counts of how it's used, whatever you chose. `KELVE_TELEMETRY=off` does the same. |
| `KELVE_UPDATES` | Where kelve asks, once a day, whether a newer version is out. Default GitHub's newest release of kelve, as the installer uses; a kelve built from a checkout doesn't ask. `off` stops it: settings and your agent then never hear of a newer kelve, and `kelve update` still works when you run it. An address asks there instead, in the shape of GitHub's API: for a mirror, or for tests. |
| `KELVE_NO_BROWSER` | Set to anything, kelve never opens a browser: for a machine without one. Run `kelve serve` in a terminal there, which shows each sign-in link instead. |
| `KELVE_PICKER` | A program kelve runs in place of the system's file picker, given `folder` or `file`, which prints the path. For the browser tests. |
| `RUST_LOG` | How much kelve logs, like `info` or `kelve_server=debug`. |

kelve has no model and no model key: your own coding agent does its work.

## Keeping things private

kelve's data folder, its secret and its sealing key can be read only by you: kelve makes them that way, and won't use a secret others can read. On Windows, your profile folders are private by default; kelve doesn't set an access list of its own yet.
