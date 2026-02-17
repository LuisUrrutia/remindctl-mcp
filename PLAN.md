# remindctl MCP Plan

## Goal

Build a Rust MCP server that runs on macOS and safely wraps `remindctl`, so a Linux machine can manage Apple Reminders through this host.

## Transport and Access

- Use MCP Streamable HTTP transport for remote clients.
- Primary network model: Tailscale private network.
- Auth model:
  - `AUTH_REQUIRED=true` by default.
  - `AUTH_REQUIRED=false` disables API key validation.
  - If auth is required and `API_KEY` is missing, fail fast at startup.

## Security Model

- Execute commands with `tokio::process::Command` and argument vectors only.
- Never execute shell command strings.
- Always pass `--json --no-input --no-color` to `remindctl`.
- Set timeouts for command execution.
- Do not log secrets.
- Accept UTF-8 list names (including emoji) while rejecting control characters.

## MCP Surface (v1)

### Prompts

- None in v1.

### Resources

- `remindctl://status`
- `remindctl://lists`
- `remindctl://server/config`
- Templates:
  - `remindctl://reminders/{filter}`
  - `remindctl://lists/{list_id}/reminders`
  - `remindctl://lists/by-name/{list_name}/reminders`

### Tools

- `server_health`
- `lists_list`
- `reminders_list`
- `reminder_add`
- `reminder_edit`
- `reminder_complete`
- `reminder_delete`
- `list_create`
- `list_rename`
- `list_delete`

## Deterministic Mutation Rules

- Do not rely on numeric index semantics for mutations.
- Resolve any provided reminder/list reference to a unique full ID before write operations.
- If a short prefix matches multiple IDs, return a structured tool error with candidates.

## Project Structure

- `src/main.rs`: startup and server bootstrap
- `src/config.rs`: env parsing and validation
- `src/error.rs`: typed error definitions
- `src/models.rs`: serde models for `remindctl` JSON
- `src/remindctl.rs`: secure runner and argument builders
- `src/resolve.rs`: ID/name resolution logic
- `src/server.rs`: MCP handlers (tools/resources)

## Implementation Steps

1. Initialize Rust project and dependencies.
2. Implement configuration and auth mode behavior.
3. Implement secure `remindctl` runner with timeout and error mapping.
4. Implement models and parsing for list/reminder/status payloads.
5. Implement lookup/resolution logic for IDs and names.
6. Implement MCP tools.
7. Implement MCP resources.
8. Add tests for config parsing, validation, arg building, and resolution.
9. Run `cargo fmt`, `cargo clippy`, and `cargo test`.

## Done Criteria

- Server starts with sane defaults.
- All tools/resources respond with structured JSON-compatible output.
- Writes are deterministic and avoid index ambiguity.
- Auth behavior matches `AUTH_REQUIRED` and `API_KEY` rules.
- Tests pass.
