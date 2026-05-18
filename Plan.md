# Plan

## Feature Development

### MCP Integration (#1)
Integrate `rmcp` crate for real MCP server connections. Current `src/mcp/mod.rs` is stub-only.
Implement stdio/SSE transport protocols.

### Session Restore (#17)
`load_session_messages` exists in `src/session.rs` but no `/resume` command in TUI.
Add `/resume <session_id>` slash command to load and continue a past session.

### `/retry` Command (#16)
Add `/retry` slash command to resend the last user message when LLM output is unsatisfactory.

### Tool Preview Improvements (#6)
`read_file`, `list_dir`, `query_sessions`, `update_agent_memory`, `update_user_profile` return `Ok(None)` for preview.
Add meaningful previews: file content length/line count for writes, diff summaries for memory updates.

### API Key Fallback (#10)
Add generic `API_KEY` env var as final fallback after `DEEPSEEK_API_KEY` and `OPENAI_API_KEY`.
Support other OpenAI-compatible providers (Groq, Together, Ollama proxy).

## Startup & Validation

### Config Validation (#5)
Validate critical config on startup: non-empty API key (after env fallback), valid model name, reasonable max_tokens.
Give clear error messages instead of cryptic API 401 errors.

## UI Polish

### Approval Mode TTY Indicator (#7)
`is_tty_command` and `command` fields in `AppMode::Approval` are used functionally but not rendered.
Add visual indicator for interactive commands (sudo/ssh).

### Streaming Render Performance (#18)
`build_all_lines` rebuilds entire render tree on every SSE delta.
Consider incremental rendering or virtual list for long conversations.

## Code Quality

### Empty tui/event.rs (#11)
`src/tui/event.rs` is just a 2-line comment. Either delete or implement event type module to reduce `app.rs` burden.

### needs_tty Expansion (#13)
Only checks sudo/su/ssh/passwd. Add vi/vim/nano/emacs/less/more/top/htop/docker exec -it/screen/tmux.
Note: `apt-get` does NOT need a TTY (despite original report).

### Memory Truncation Edge Case (#9)
If header exceeds `max_bytes`, raw byte truncation can cut mid-markdown. Consider smarter fallback.

### Test Coverage (#12)
- Add integration tests for full ReAct loop
- Add multi-tool combination tests (batched edits, mixed read/write)
- Add TUI unit tests (message filtering, input handling)
