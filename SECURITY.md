# Security

kelve runs on your own computer. It reads what your agent hands over, keeps your skills as files and what it learned in a store on your computer, and sends your skills to kelve's hosted service only for a verified run you ask for. Reports of anything that could expose someone's work or skills, or let someone else act as them, are the most useful thing anyone can send.

## Reporting a vulnerability

Email **[hello@kelve.ai](mailto:hello@kelve.ai)** with what you found, how to reproduce it, and what someone could do with it. Please don't open a public issue for it.

You'll hear back within three working days. Once it's fixed, the release notes credit you, unless you'd rather they didn't.

## What's in scope

The `kelve` command, the service it runs on your computer, the web app, and the release builds of this repository: signing a browser in, the secret the command line shares with kelve, the tools kelve gives your agent over MCP, the hook it adds to your agent's settings, the API, how keys are kept, reading what your agent hands over, installing and updating kelve, and what's sent to kelve's hosted service.

## How kelve keeps things safe

- kelve listens only on your own computer, and answers only requests addressed to its own name, which stops DNS rebinding.
- There are no passwords. A browser signs in with a one-time link that works once, for a minute, and gets a session cookie. Links and sessions are random, and only their SHA-256 is kept. Cookies are `HttpOnly` and `SameSite=Strict`, and `Secure` over https. Changes from a browser must come from kelve's own pages (`Sec-Fetch-Site`, `Origin`).
- The command line, your agent's tools and kelve's hook use a secret kept in a file only you can read; kelve refuses one others can read. With it they can open something, take kelve's jobs and hand in answers, read your skills, read the notes left for the agent and mark them done, read and write what kelve marks for you, pass on the hook's events, and ask kelve to stop. They can't read anything else kelve keeps, and can't sign a browser in.
- Outside your project, kelve changes only how your agent reaches it, with your yes: kelve as an MCP server in its settings, one hook that runs `kelve moment`, put in and taken out together, leaving the rest of the file as it was, and links to your skills in each agent's own skills folder.
- kelve has no model and no model key of its own. Work that needs one is a job for your own coding agent, and kelve checks each answer's shape before it takes it. A verified run is paid with your own provider's key, sent with that run alone.
- kelve runs nothing from what you open: no builds, hooks or scripts. Files that may hold secrets are never read. Every secret in what is read is masked as it's read, and again before anything goes to your agent or kelve's hosted service.
- Your work goes to no one but your own agent. Nothing of yours leaves, except your skills, to kelve's hosted service, for a verified run you ask for: with secrets and paths stripped first, every file shown to you before it goes, and deleted when the run ends. Your work, your messages to your agent, your edits and your notes never leave.
- Text from your files, your notes and your messages goes to your agent marked as material, never instructions.
- Nothing an agent writes becomes HTML or an image: kelve draws its markdown as its own elements, with links only to pages on the web. kelve's pages run only kelve's own scripts, and images from what you open are served so that nothing in them runs.
- The keys kelve keeps, your kelve.ai account's and your own provider's, are sealed with your install's key (XChaCha20-Poly1305) before they're kept, and never shown back.
- `kelve update` and the installers check each download against the release's `SHA256SUMS` before putting it in place.
- Logs never hold tokens, keys, passwords, secrets or file contents.
