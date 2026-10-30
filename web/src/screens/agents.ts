// The coding agents kelve knows, and the one line that connects each.

import type { AgentKind } from '../api/types'

export type AgentChoice = {
  kind: AgentKind
  name: string
  /** Where the line goes, in a sentence. */
  how: string
  line: string
  /** Whether it has hooks, so it can be told of notes, and given lessons as it works. */
  hooks: boolean
  /** Where kelve's hook goes, and on which of its events: the change it shows before your yes. */
  hook?: { file: string; events: string[]; then?: string }
}

export const AGENTS: AgentChoice[] = [
  { kind: 'claudeCode', name: 'Claude Code', how: 'One line, in the terminal where you use it.', line: 'claude mcp add --scope user kelve -- kelve mcp', hooks: true, hook: { file: '~/.claude/settings.json', events: ['UserPromptSubmit', 'PostToolUse', 'PostToolUseFailure', 'PreToolUse', 'Stop'] } },
  { kind: 'codex', name: 'Codex', how: 'One line, in the terminal where you use it.', line: 'codex mcp add kelve -- kelve mcp', hooks: true, hook: { file: '~/.codex/hooks.json', events: ['UserPromptSubmit', 'PostToolUse', 'PreToolUse'], then: 'Then trust it in Codex, in /hooks: Codex runs a hook only once you have.' } },
  { kind: 'cursor', name: 'Cursor', how: 'Add this to .cursor/mcp.json in your project, or in your home folder.', line: '{ "mcpServers": { "kelve": { "command": "kelve", "args": ["mcp"] } } }', hooks: true, hook: { file: '~/.cursor/hooks.json', events: ['postToolUse', 'postToolUseFailure'] } },
  { kind: 'gemini', name: 'Gemini CLI', how: 'One line, in the terminal where you use it.', line: 'gemini mcp add --scope user kelve kelve mcp', hooks: true, hook: { file: '~/.gemini/settings.json', events: ['BeforeAgent', 'AfterTool'], then: 'Then restart Gemini CLI, so it reads it.' } },
  { kind: 'other', name: 'Another', how: 'Give your agent kelve as an MCP server that runs this command.', line: 'kelve mcp', hooks: false },
]

export const agentChoice = (kind: AgentKind | undefined) => AGENTS.find((a) => a.kind === kind) ?? AGENTS[0]

/** What the green dot by an agent means. */
export const INSTALLED = 'Installed here'

/** What to say to your agent, once it's connected. */
export const SHOW_ME = 'Show me what you did in kelve.'
export const SHOW_ROOTS = 'Show me what this rests on, in kelve.'
