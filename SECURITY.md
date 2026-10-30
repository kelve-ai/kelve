# Security

kelve runs on your own computer. It reads what you open, keeps what it marks in a store on your computer, and adds general knowledge to a shared graph. Reports of anything that could expose what someone opens, or let someone else act as them, are the most useful thing anyone can send.

## Reporting a vulnerability

Email **[hello@kelve.ai](mailto:hello@kelve.ai)** with what you found, how to reproduce it, and what someone could do with it. Please don't open a public issue for it.

You'll hear back within three working days. Once it's fixed, the release notes credit you, unless you'd rather they didn't.

## What's in scope

The `kelve` command, the service it runs on your computer, the web app, and the release builds of this repository: signing a browser in, the secret the command line shares with kelve, the tools kelve gives your agent over MCP, the hook it adds to your agent's settings, the API, how keys are kept, reading what you open, installing and updating kelve, and what's sent to the shared graph.

## How kelve keeps things safe

- kelve listens only on your own computer, and answers only requests addressed to its own name, which stops DNS rebinding.
- There are no passwords. A browser signs in with a one-time link that works once, for a minute, and gets a session cookie. Links and sessions are random, and only their SHA-256 is kept. Cookies are `HttpOnly` and `SameSite=Strict`, and `Secure` over https. Changes from a browser must come from kelve's own pages (`Sec-Fetch-Site`, `Origin`).
- The command line, your agent's tools and kelve's hook use a secret kept in a file only you can read; kelve refuses one others can read. With it they can open something, take kelve's jobs and hand in answers, read the notes left for the agent and mark them done, read and write what kelve marks for you, ask for lessons and say whether one held, pass on the hook's events, and ask kelve to stop. They can't read anything else kelve keeps, and can't sign a browser in.
- Outside your project, kelve changes only how your agent reaches it, with your yes: kelve as an MCP server in its settings, and one hook that runs `kelve moment`, put in and taken out together, leaving the rest of the file as it was.
- kelve has no model and no model key. Work that needs one is a job for your own coding agent, and kelve checks each answer's shape before it takes it.
- kelve runs nothing from what you open: no builds, hooks or scripts. Files that may hold secrets are never read. Every secret in what is read is masked as it's read, and again before anything goes to your agent or the shared graph.
- What you open goes to no one but your own agent. The shared graph gets general knowledge only: ideas, their entries and how they relate, and lessons with what came of them. Never file contents, names or paths, never which ideas your files use, and never your notes. Names of ideas are looked up by an anonymous bucket of the graph's name index, so a name the graph has exactly is never sent.
- Each read of the shared graph spends a token the graph signed without seeing it, so a read says someone signed in is reading, never who.
- Text from your files or the web goes to your agent marked as material, never instructions.
- Nothing an agent writes becomes HTML, a link or an image. kelve's pages run only kelve's own scripts, and images from what you open are served so that nothing in them runs.
- The key for adding to the shared graph is sealed with your install's key (XChaCha20-Poly1305) before it's kept, and never shown back. Keys given in the environment never reach the store.
- `kelve update` and the installers check each download against the release's `SHA256SUMS` before putting it in place.
- Logs never hold tokens, keys, passwords, secrets or file contents.
