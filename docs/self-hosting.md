# Running kelve

kelve runs on your own computer: the `kelve` command starts it in the background and opens it in your browser. Installing it is in [releasing.md](releasing.md); this page lists its settings.

## Settings

kelve reads these from the environment as it starts. Everything else is set in the app.

| Variable | What it does |
|---|---|
| `KELVE_URL` | Where you open kelve. Links are made from it, and kelve answers at its host. Default `http://kelve.localhost:4600`, or `kelve.localhost` on the port in `KELVE_BIND`. Browsers send any name under `.localhost` to the computer they run on. kelve also answers at `localhost` on its port. The command line sends its secret only over https, or over plain http to this computer. |
| `KELVE_BIND` | The address kelve listens on. Default `127.0.0.1:4600`. |
| `KELVE_DATA` | Where kelve keeps its files: its store, its sealing key, its log and reports of what went wrong. Default the folder the secret is in: `~/.config/kelve`, or `kelve` under `XDG_CONFIG_HOME`, or under `%APPDATA%` on Windows. |
| `KELVE_STORE` | The store's file. Default one in the data folder for each address of kelve's hosted service: `kelve.db` for kelve.ai's, `kelve-<host>-<port>.db` for any other, such as `kelve-localhost-4620.db`, and `kelve-off.db` for none. |
| `KELVE_SECRET_KEY` | 64 hex characters (`openssl rand -hex 32`). Seals the keys kelve keeps, your kelve.ai account's and your own model provider's, before they're kept. When it isn't set, kelve makes one the first time, in `KELVE_DATA/secret.key`. |
| `KELVE_SECRET_FILE` | Where the secret the command line and your agent's tools use lives. Default `~/.config/kelve/secret`, or under `XDG_CONFIG_HOME`, or `%APPDATA%` on Windows. |
| `KELVE_UTC_OFFSET_MINUTES` | Your time zone, as minutes from UTC, like `60` or `-300`: what "today" means on home, and "this week" for the weekly count. Default: your computer's own. |
| `KELVE_GRAPH_URL` | kelve's hosted service, for verified scores and your page: kelve.ai's, `https://graph.kelve.ai`, by default (named for the shared graph it once was). `off` uses none: nothing else changes. Each address has its own store, so its own sign-in: change this and restart kelve to change services, and change it back to find everything as it was. A build can set its own default with `KELVE_DEFAULT_GRAPH_URL` as it's built. |
| `KELVE_CLAUDE_DIR` | Claude Code's folder, whose `settings.json` gets kelve's hook when you turn it on. Default Claude Code's own `CLAUDE_CONFIG_DIR`, or `~/.claude`. |
| `KELVE_AGENTS_HOME` | The home folder other agents' settings are in, for kelve's hook in them. Default your home folder. |
| `KELVE_SKILLS` | The folder your own skills are kept in, a folder each with its `SKILL.md`. Default `~/.kelve/skills`. A project's skills are in its own `.kelve/skills/`. The sandbox and the tests set it, so they never touch yours. |
| `DO_NOT_TRACK` | Set to anything but `0` or `false`, kelve never sends anonymous counts of how it's used, whatever you chose. `KELVE_TELEMETRY=off` does the same. |
| `KELVE_UPDATES` | Where kelve asks, once a day, whether a newer version is out. Default GitHub's newest release of kelve, as the installer uses; a kelve built from a checkout doesn't ask. `off` stops it: settings and your agent then never hear of a newer kelve, and `kelve update` still works when you run it. An address asks there instead, in the shape of GitHub's API: for a mirror, or for tests. |
| `KELVE_NO_BROWSER` | Set to anything, kelve never opens a browser: for a machine without one. Run `kelve serve` in a terminal there, which shows each sign-in link instead. |
| `KELVE_PICKER` | A program kelve runs in place of the system's file picker, given `folder` or `file`, which prints the path. For the browser tests. |
| `RUST_LOG` | How much kelve logs, like `info` or `kelve_server=debug`. |

kelve has no model and no model key of its own: your own coding agent does its work. A verified run on kelve.ai is paid with your own model provider's key, which you give in the app when a run first needs it: it's sealed before it's kept, and sent with a run you confirm, and nowhere else.

## Keeping things private

kelve's data folder, its secret and its sealing key can be read only by you: kelve makes them that way, and won't use a secret others can read. On Windows, your profile folders are private by default; kelve doesn't set an access list of its own yet.
