---
name: remindctl-mcp
description: Manage Apple Reminders from OpenClaw through MCPorter and the remindctl MCP server. Use for reminder/list read-write tasks (list, add, edit, complete, delete), especially when reminders run on a remote macOS host and availability can be intermittent. Trigger when users say phrases like "remind me", "remember to", "set a reminder", "recuérdame", or ask to create/manage reminder lists.
metadata: {"openclaw":{"emoji":"🗓️","requires":{"bins":["mcporter"]}}}
---

Use MCPorter as the only execution path.

## Workflow

1. Probe server health once.
2. If healthy, execute the requested MCP tool immediately.
3. If unhealthy:
   - for write/delete/edit/complete/list mutations, queue the action and inform the user,
   - for read-only requests, return unavailable and stop.

Health probe:

```bash
npx mcporter call remindctl.server_health
```

Connectivity check (when diagnosing):

```bash
npx mcporter list remindctl
npx mcporter call remindctl.server_health
```

## Queue behavior when MCP is down (required)

Do not use short retry loops for this skill. The macOS host can be offline for days.

Use deferred queueing and let workspace automation process it later.

Queue file (workspace-local, scoped per workspace):

`./.openclaw/{current_workspace}/remindctl-pending-actions.jsonl`

Each line is one JSON action object:

```json
{"id":"<uuid>","createdAt":"<iso>","op":"reminder_add","args":{"title":"..."},"attempts":0,"lastError":null}
```

When queueing due to downtime, tell the user explicitly:

"I cannot perform this action right now because no MacBook with the reminders MCP is available. I queued it and will keep retrying automatically until one is available, then notify you."

## HEARTBEAT.md integration (on demand)

Queue processor instructions are user-managed and applied once in the user workspace automation checklist.

- Never rewrite `HEARTBEAT.md` repeatedly.
- If user asks to add queue-processing instructions:
  1. Ask whether they want to add them to `HEARTBEAT.md`.
  2. If yes, check for equivalent existing lines first.
  3. Add only if missing.

Block to add:

```md
- If `./.openclaw/{current_workspace}/remindctl-pending-actions.jsonl` exists and has entries:
  - Call `remindctl.server_health` once.
  - If unhealthy, keep the queue unchanged and respond with `HEARTBEAT_OK`.
  - If healthy, send queued actions in one call to `remindctl.process_pending_actions`.
  - For each success result, remove that action from queue.
  - For each failure result, increment attempts and keep it queued with last error.
  - If one or more actions were applied, notify the user with a concise summary of what was completed.
  - If no actions were completed in this cycle, keep output concise and respond with `HEARTBEAT_OK`.
```

## Core commands

Use function-style calls when possible.

```bash
npx mcporter call 'remindctl.reminders_list(filter: "pending")'
npx mcporter call 'remindctl.reminder_add(title: "Comprar Coca Zero lata pequena")'
npx mcporter call 'remindctl.reminder_complete(reminderIds: ["<id>"])'
npx mcporter call 'remindctl.reminder_delete(reminderIds: ["<id>"])'
npx mcporter call 'remindctl.process_pending_actions(actions: [{id: "<queue-id>", op: "reminder_add", args: {title: "..."}}])'
```

## Tool usage rules

- Prefer one tool call per user intent.
- Do not use filesystem tools to verify reminder operations.
- Treat `reminder_delete` response as authoritative.
- Run follow-up `reminders_list` only on error/ambiguity.
- For "borrala", call `reminder_delete` without IDs once (server may use recent reminder context).

## List placement policy

- If user provides a target list, pass `listId` or `listName` explicitly.
- If user does not provide a list, let `reminder_add` auto-route.
- Do not force `Reminders` unless explicitly requested.

## Response policy

- Return concise confirmation with key fields only: title, list, id, due (if any).
- When operation is idempotent (already deleted/not found), explain clearly and continue without extra probes.
- When queuing due to downtime, always state: unavailable now, queued, automatic retries will continue, and user will be notified after processing.
