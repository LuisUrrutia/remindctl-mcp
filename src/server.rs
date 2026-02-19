use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{
    ErrorData as McpError, Json, RoleServer, ServerHandler,
    model::{
        AnnotateAble, InitializeRequestParams, InitializeResult, ListResourceTemplatesResult,
        ListResourcesResult, PaginatedRequestParams, RawResourceTemplate,
        ReadResourceRequestParams, ReadResourceResult, ResourceContents, ResourceTemplate,
        ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::Config;
use crate::error::AppError;
use crate::models::{
    BatchActionResult, BatchProcessResult, DeleteResult, ListDeleteResult, ListsResult,
    RemindctlStatus, Reminder, ReminderList, ReminderListResult, ServerHealth,
};
use crate::remindctl::RemindctlRunner;
use crate::resolve::{
    resolve_list_name, resolve_reminder_ids, resolve_reminder_ids_lenient, validate_text_input,
};

pub struct RuntimeState {
    pub config: Config,
    pub runner: RemindctlRunner,
    recent_reminder_id: Mutex<Option<String>>,
}

impl RuntimeState {
    pub fn new(config: Config) -> Result<Self, AppError> {
        let runner = RemindctlRunner::new(
            config.remindctl_bin.clone(),
            config.read_timeout,
            config.write_timeout,
        );

        Ok(Self {
            config,
            runner,
            recent_reminder_id: Mutex::new(None),
        })
    }
}

#[derive(Clone)]
pub struct AppServer {
    state: Arc<RuntimeState>,
    tool_router: ToolRouter<Self>,
}

impl AppServer {
    pub fn new(state: Arc<RuntimeState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    async fn fetch_lists(&self) -> Result<Vec<ReminderList>, AppError> {
        self.state
            .runner
            .run_read_json::<Vec<ReminderList>>(vec!["list".to_owned()])
            .await
    }

    async fn fetch_all_reminders(&self) -> Result<Vec<Reminder>, AppError> {
        self.state
            .runner
            .run_read_json::<Vec<Reminder>>(vec!["show".to_owned(), "all".to_owned()])
            .await
    }

    fn infer_best_list_name(
        lists: &[ReminderList],
        title: &str,
        notes: Option<&str>,
    ) -> Option<String> {
        if lists.is_empty() {
            return None;
        }

        let reminder_text = format!("{title} {}", notes.unwrap_or_default()).to_ascii_lowercase();
        let reminder_tokens = tokenize(&reminder_text);
        let mut best: Option<(&ReminderList, i32)> = None;

        for list in lists {
            let name = list.title.to_ascii_lowercase();
            let name_tokens = tokenize(&name);
            let overlap = reminder_tokens.intersection(&name_tokens).count() as i32;
            let prefix_overlap = reminder_tokens
                .iter()
                .filter(|token| {
                    name_tokens.iter().any(|name_token| {
                        shared_prefix_len(token, name_token) >= 4
                            || shared_prefix_len(name_token, token) >= 4
                    })
                })
                .count() as i32;
            let contains_bonus = if !name.is_empty() && reminder_text.contains(&name) {
                20
            } else {
                0
            };

            let thematic_bonus = themed_list_bonus(&name, &reminder_text);

            let preferred_fallback_bonus = if matches!(
                name.as_str(),
                "reminders" | "inbox" | "todo" | "todos" | "tareas"
            ) {
                1
            } else {
                0
            };

            let score = overlap * 20
                + prefix_overlap * 12
                + contains_bonus
                + thematic_bonus
                + preferred_fallback_bonus;
            match best {
                Some((_, best_score)) if score <= best_score => {}
                _ => best = Some((list, score)),
            }
        }

        let (best_list, best_score) = best?;
        if best_score > 1 {
            return Some(best_list.title.clone());
        }

        lists
            .iter()
            .find(|list| {
                matches!(
                    list.title.to_ascii_lowercase().as_str(),
                    "reminders" | "inbox" | "todo" | "todos" | "tareas"
                )
            })
            .map(|list| list.title.clone())
            .or_else(|| lists.first().map(|list| list.title.clone()))
    }
}

fn tokenize(value: &str) -> HashSet<String> {
    value
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|part| part.chars().count() >= 2)
        .map(|part| part.to_ascii_lowercase())
        .collect()
}

fn shared_prefix_len(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

fn themed_list_bonus(list_name: &str, reminder_text: &str) -> i32 {
    let shopping_lists = [
        "compr", "shop", "groc", "super", "market", "tienda", "store",
    ];
    let shopping_terms = [
        "compr", "buy", "milk", "pan", "agua", "coca", "cola", "super", "market", "grocery",
    ];

    if shopping_lists.iter().any(|k| list_name.contains(k))
        && shopping_terms.iter().any(|k| reminder_text.contains(k))
    {
        return 24;
    }

    0
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReminderListInput {
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(rename = "includeCompleted", default)]
    pub include_completed: Option<bool>,
    #[serde(rename = "listId", default)]
    pub list_id: Option<String>,
    #[serde(rename = "listName", default)]
    pub list_name: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReminderAddInput {
    pub title: String,
    #[serde(rename = "listId", default)]
    pub list_id: Option<String>,
    #[serde(rename = "listName", default)]
    pub list_name: Option<String>,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReminderEditInput {
    #[serde(rename = "reminderId")]
    pub reminder_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(rename = "listId", default)]
    pub list_id: Option<String>,
    #[serde(rename = "listName", default)]
    pub list_name: Option<String>,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(rename = "clearDue", default)]
    pub clear_due: Option<bool>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub complete: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReminderMultiInput {
    #[serde(rename = "reminderIds", default)]
    pub reminder_ids: Vec<String>,
    #[serde(rename = "reminderId", default)]
    pub reminder_id: Option<String>,
    #[serde(rename = "dryRun", default)]
    pub dry_run: Option<bool>,
    #[serde(rename = "allowMissing", default)]
    pub allow_missing: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListCreateInput {
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListRenameInput {
    #[serde(rename = "listId", default)]
    pub list_id: Option<String>,
    #[serde(rename = "listName", default)]
    pub list_name: Option<String>,
    #[serde(rename = "newName")]
    pub new_name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListDeleteInput {
    #[serde(rename = "listId", default)]
    pub list_id: Option<String>,
    #[serde(rename = "listName", default)]
    pub list_name: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BatchActionInput {
    pub id: String,
    pub op: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BatchProcessInput {
    pub actions: Vec<BatchActionInput>,
    #[serde(rename = "stopOnError", default)]
    pub stop_on_error: Option<bool>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ServerConfigResource {
    #[serde(rename = "authRequired")]
    pub auth_required: bool,
    #[serde(rename = "bindAddr")]
    pub bind_addr: String,
    #[serde(rename = "readTimeoutSecs")]
    pub read_timeout_secs: u64,
    #[serde(rename = "writeTimeoutSecs")]
    pub write_timeout_secs: u64,
}

#[tool_router]
impl AppServer {
    #[tool(
        description = "Health check. Use this first when troubleshooting connectivity or permissions. Returns server auth mode and remindctl authorization state."
    )]
    async fn server_health(&self) -> Result<Json<ServerHealth>, String> {
        let status = self
            .state
            .runner
            .run_read_json::<RemindctlStatus>(vec!["status".to_owned()])
            .await
            .map_err(tool_error)?;

        Ok(Json(ServerHealth {
            ok: true,
            auth_required: self.state.config.auth_required,
            remindctl_authorized: status.authorized,
            remindctl_status: status.status,
        }))
    }

    #[tool(
        description = "List all Apple Reminders lists with IDs and counts. Use this before write operations when you need a stable listId."
    )]
    async fn lists_list(&self) -> Result<Json<ListsResult>, String> {
        let lists = self.fetch_lists().await.map_err(tool_error)?;
        Ok(Json(ListsResult { lists }))
    }

    #[tool(
        description = "Primary read tool for reminders. If filter is omitted, return pending reminders only. Supported filter values: pending, incomplete, today, tomorrow, week, overdue, upcoming, completed, all, or a date string in ISO 8601/RFC3339 format (for example 2026-03-01 or 2026-03-01T14:30:00Z). Prefer this tool over manual filtering."
    )]
    async fn reminders_list(
        &self,
        Parameters(input): Parameters<ReminderListInput>,
    ) -> Result<Json<ReminderListResult>, String> {
        let lists = self.fetch_lists().await.map_err(tool_error)?;
        let list_name =
            resolve_list_name(&lists, input.list_id.as_deref(), input.list_name.as_deref())
                .map_err(tool_error)?;

        let raw_filter = input
            .filter
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("pending");

        let mut args = vec!["show".to_owned()];
        let pending_mode = matches!(
            raw_filter.to_ascii_lowercase().as_str(),
            "pending" | "incomplete"
        );
        if pending_mode {
            args.push("all".to_owned());
        } else {
            args.push(raw_filter.to_owned());
        }

        if let Some(name) = list_name {
            args.push("--list".to_owned());
            args.push(name);
        }

        let mut reminders = self
            .state
            .runner
            .run_read_json::<Vec<Reminder>>(args)
            .await
            .map_err(tool_error)?;

        if pending_mode && !input.include_completed.unwrap_or(false) {
            reminders.retain(|reminder| !reminder.is_completed);
        }

        Ok(Json(ReminderListResult { reminders }))
    }

    #[tool(
        description = "Create a reminder from natural input. Use listId or listName when you need strict placement. For due dates, pass due as ISO 8601/RFC3339 (for example 2026-03-01 or 2026-03-01T14:30:00Z). If list is omitted, auto-route to the best matching existing list using title+notes semantic overlap; if no strong match exists, fall back to Reminders/Inbox/Todo/Tareas, then first available list."
    )]
    async fn reminder_add(
        &self,
        Parameters(input): Parameters<ReminderAddInput>,
    ) -> Result<Json<Reminder>, String> {
        validate_text_input(&input.title, "title", 300).map_err(tool_error)?;
        if let Some(notes) = &input.notes {
            validate_text_input(notes, "notes", 4000).map_err(tool_error)?;
        }

        let lists = self.fetch_lists().await.map_err(tool_error)?;
        let list_name =
            resolve_list_name(&lists, input.list_id.as_deref(), input.list_name.as_deref())
                .map_err(tool_error)?
                .or_else(|| {
                    Self::infer_best_list_name(&lists, &input.title, input.notes.as_deref())
                });

        let mut args = vec!["add".to_owned(), "--title".to_owned(), input.title];
        if let Some(name) = list_name {
            args.push("--list".to_owned());
            args.push(name);
        }
        if let Some(due) = input.due {
            args.push("--due".to_owned());
            args.push(due);
        }
        if let Some(notes) = input.notes {
            args.push("--notes".to_owned());
            args.push(notes);
        }
        if let Some(priority) = input.priority {
            args.push("--priority".to_owned());
            args.push(priority);
        }

        let reminder = self
            .state
            .runner
            .run_write_json::<Reminder>(args)
            .await
            .map_err(tool_error)?;

        if let Ok(mut recent) = self.state.recent_reminder_id.lock() {
            *recent = Some(reminder.id.clone());
        }

        Ok(Json(reminder))
    }

    #[tool(
        description = "Update an existing reminder by ID or unique ID prefix. Supports title, due date, notes, priority, completion state, and list move. For due, use ISO 8601/RFC3339 (for example 2026-03-01 or 2026-03-01T14:30:00Z). Never uses numeric index semantics."
    )]
    async fn reminder_edit(
        &self,
        Parameters(input): Parameters<ReminderEditInput>,
    ) -> Result<Json<Reminder>, String> {
        let all_reminders = self.fetch_all_reminders().await.map_err(tool_error)?;
        let resolved_id = resolve_reminder_ids(&all_reminders, &[input.reminder_id])
            .map_err(tool_error)?
            .remove(0);

        let lists = self.fetch_lists().await.map_err(tool_error)?;
        let list_name =
            resolve_list_name(&lists, input.list_id.as_deref(), input.list_name.as_deref())
                .map_err(tool_error)?;

        let mut args = vec!["edit".to_owned(), resolved_id];
        if let Some(title) = input.title {
            validate_text_input(&title, "title", 300).map_err(tool_error)?;
            args.push("--title".to_owned());
            args.push(title);
        }
        if let Some(name) = list_name {
            args.push("--list".to_owned());
            args.push(name);
        }
        if let Some(due) = input.due {
            args.push("--due".to_owned());
            args.push(due);
        }
        if input.clear_due.unwrap_or(false) {
            args.push("--clear-due".to_owned());
        }
        if let Some(notes) = input.notes {
            validate_text_input(&notes, "notes", 4000).map_err(tool_error)?;
            args.push("--notes".to_owned());
            args.push(notes);
        }
        if let Some(priority) = input.priority {
            args.push("--priority".to_owned());
            args.push(priority);
        }
        if let Some(complete) = input.complete {
            args.push(if complete {
                "--complete".to_owned()
            } else {
                "--incomplete".to_owned()
            });
        }

        let reminder = self
            .state
            .runner
            .run_write_json::<Reminder>(args)
            .await
            .map_err(tool_error)?;

        Ok(Json(reminder))
    }

    #[tool(
        description = "Mark one or more reminders complete using full IDs or unique ID prefixes. Reject numeric indexes. Use dryRun to preview changes. Treat a successful response as authoritative; do not verify via local filesystem inspection."
    )]
    async fn reminder_complete(
        &self,
        Parameters(input): Parameters<ReminderMultiInput>,
    ) -> Result<Json<ReminderListResult>, String> {
        let mut raw_ids = input.reminder_ids;
        if let Some(reminder_id) = input.reminder_id {
            raw_ids.push(reminder_id);
        }
        if raw_ids.is_empty() {
            return Err(tool_error(AppError::invalid_input(
                "reminderIds or reminderId is required",
            )));
        }

        let all_reminders = self.fetch_all_reminders().await.map_err(tool_error)?;
        let resolved_ids = resolve_reminder_ids(&all_reminders, &raw_ids).map_err(tool_error)?;

        let mut args = vec!["complete".to_owned()];
        args.extend(resolved_ids);
        if input.dry_run.unwrap_or(false) {
            args.push("--dry-run".to_owned());
        }

        let reminders = self
            .state
            .runner
            .run_write_json::<Vec<Reminder>>(args)
            .await
            .map_err(tool_error)?;

        Ok(Json(ReminderListResult { reminders }))
    }

    #[tool(
        description = "Delete reminders by full ID or unique prefix. Accepts reminderIds[] and/or reminderId. If no ID is provided, uses the most recently created reminder in this server session. Idempotent by default: missing reminders are reported in alreadyAbsentRefs instead of error (allowMissing=true). Treat this response as authoritative and avoid extra verification calls unless the tool returns an error."
    )]
    async fn reminder_delete(
        &self,
        Parameters(input): Parameters<ReminderMultiInput>,
    ) -> Result<Json<DeleteResult>, String> {
        let mut raw_ids = input.reminder_ids;
        if let Some(reminder_id) = input.reminder_id {
            raw_ids.push(reminder_id);
        }

        let mut used_recent_reference = false;
        if raw_ids.is_empty()
            && let Ok(recent) = self.state.recent_reminder_id.lock()
            && let Some(last_id) = recent.clone()
        {
            raw_ids.push(last_id);
            used_recent_reference = true;
        }

        if raw_ids.is_empty() {
            return Err(tool_error(AppError::invalid_input(
                "reminderIds or reminderId is required when there is no recent reminder context",
            )));
        }

        let all_reminders = self.fetch_all_reminders().await.map_err(tool_error)?;
        let resolution =
            resolve_reminder_ids_lenient(&all_reminders, &raw_ids).map_err(tool_error)?;
        let allow_missing = input.allow_missing.unwrap_or(true);

        if resolution.resolved_ids.is_empty() {
            if allow_missing {
                return Ok(Json(DeleteResult {
                    deleted_ids: Vec::new(),
                    deleted_reminders: Vec::new(),
                    already_absent_refs: resolution.missing_refs,
                    used_recent_reference,
                    message: "nothing to delete; all refs already absent".to_owned(),
                }));
            }

            return Err(tool_error(AppError::invalid_input(
                "none of the provided reminder refs exist",
            )));
        }

        let mut args = vec!["delete".to_owned()];
        args.extend(resolution.resolved_ids.clone());
        if input.dry_run.unwrap_or(false) {
            args.push("--dry-run".to_owned());
        } else {
            args.push("--force".to_owned());
        }

        let deleted_reminders = self
            .state
            .runner
            .run_write_json::<Vec<Reminder>>(args)
            .await
            .map_err(tool_error)?;

        Ok(Json(DeleteResult {
            deleted_ids: resolution.resolved_ids,
            deleted_reminders,
            already_absent_refs: resolution.missing_refs,
            used_recent_reference,
            message: "deletion applied; no extra verification required".to_owned(),
        }))
    }

    #[tool(
        description = "Process multiple queued reminder/list mutations in one call. Accepts actions with {id, op, args}. Supported ops: reminder_add, reminder_edit, reminder_complete, reminder_delete, list_create, list_rename, list_delete. Any due/datetime fields inside args must use ISO 8601/RFC3339 (for example 2026-03-01 or 2026-03-01T14:30:00Z). Returns per-action success/error so queue processors can update state without extra verification calls."
    )]
    async fn process_pending_actions(
        &self,
        Parameters(input): Parameters<BatchProcessInput>,
    ) -> Result<Json<BatchProcessResult>, String> {
        let mut results = Vec::with_capacity(input.actions.len());
        let stop_on_error = input.stop_on_error.unwrap_or(false);

        for action in input.actions {
            let op = action.op.to_ascii_lowercase();
            let action_result = match self.execute_batch_action(&op, action.args).await {
                Ok(value) => BatchActionResult {
                    id: action.id,
                    op,
                    ok: true,
                    error: None,
                    data: Some(value),
                },
                Err(error) => BatchActionResult {
                    id: action.id,
                    op,
                    ok: false,
                    error: Some(error),
                    data: None,
                },
            };

            let should_stop = stop_on_error && !action_result.ok;
            results.push(action_result);
            if should_stop {
                break;
            }
        }

        let processed = results.len() as i64;
        let succeeded = results.iter().filter(|result| result.ok).count() as i64;
        let failed = processed.saturating_sub(succeeded);

        Ok(Json(BatchProcessResult {
            processed,
            succeeded,
            failed,
            results,
        }))
    }

    async fn execute_batch_action(&self, op: &str, args: Value) -> Result<Value, String> {
        match op {
            "reminder_add" => {
                let input = serde_json::from_value::<ReminderAddInput>(args)
                    .map_err(|err| format!("invalid reminder_add args: {err}"))?;
                let result = self.reminder_add(Parameters(input)).await?;
                serde_json::to_value(result.0).map_err(|err| err.to_string())
            }
            "reminder_edit" => {
                let input = serde_json::from_value::<ReminderEditInput>(args)
                    .map_err(|err| format!("invalid reminder_edit args: {err}"))?;
                let result = self.reminder_edit(Parameters(input)).await?;
                serde_json::to_value(result.0).map_err(|err| err.to_string())
            }
            "reminder_complete" => {
                let input = serde_json::from_value::<ReminderMultiInput>(args)
                    .map_err(|err| format!("invalid reminder_complete args: {err}"))?;
                let result = self.reminder_complete(Parameters(input)).await?;
                serde_json::to_value(result.0).map_err(|err| err.to_string())
            }
            "reminder_delete" => {
                let input = serde_json::from_value::<ReminderMultiInput>(args)
                    .map_err(|err| format!("invalid reminder_delete args: {err}"))?;
                let result = self.reminder_delete(Parameters(input)).await?;
                serde_json::to_value(result.0).map_err(|err| err.to_string())
            }
            "list_create" => {
                let input = serde_json::from_value::<ListCreateInput>(args)
                    .map_err(|err| format!("invalid list_create args: {err}"))?;
                let result = self.list_create(Parameters(input)).await?;
                serde_json::to_value(result.0).map_err(|err| err.to_string())
            }
            "list_rename" => {
                let input = serde_json::from_value::<ListRenameInput>(args)
                    .map_err(|err| format!("invalid list_rename args: {err}"))?;
                let result = self.list_rename(Parameters(input)).await?;
                serde_json::to_value(result.0).map_err(|err| err.to_string())
            }
            "list_delete" => {
                let input = serde_json::from_value::<ListDeleteInput>(args)
                    .map_err(|err| format!("invalid list_delete args: {err}"))?;
                let result = self.list_delete(Parameters(input)).await?;
                serde_json::to_value(result.0).map_err(|err| err.to_string())
            }
            _ => Err(format!("unsupported op '{op}'")),
        }
    }

    #[tool(
        description = "Create a new reminders list by name. Returns the created list object with ID for follow-up operations."
    )]
    async fn list_create(
        &self,
        Parameters(input): Parameters<ListCreateInput>,
    ) -> Result<Json<ReminderList>, String> {
        validate_text_input(&input.name, "name", 120).map_err(tool_error)?;

        self.state
            .runner
            .run_write_no_output(vec![
                "list".to_owned(),
                input.name.clone(),
                "--create".to_owned(),
            ])
            .await
            .map_err(tool_error)?;

        let lists = self.fetch_lists().await.map_err(tool_error)?;
        let created = lists
            .into_iter()
            .find(|list| list.title == input.name)
            .ok_or_else(|| {
                tool_error(AppError::invalid_input(
                    "created list not found after operation",
                ))
            })?;

        Ok(Json(created))
    }

    #[tool(
        description = "Rename an existing list identified by listId or listName. Prefer listId when available. Returns the renamed list object."
    )]
    async fn list_rename(
        &self,
        Parameters(input): Parameters<ListRenameInput>,
    ) -> Result<Json<ReminderList>, String> {
        validate_text_input(&input.new_name, "new_name", 120).map_err(tool_error)?;

        let lists = self.fetch_lists().await.map_err(tool_error)?;
        let source_name =
            resolve_list_name(&lists, input.list_id.as_deref(), input.list_name.as_deref())
                .map_err(tool_error)?
                .ok_or_else(|| {
                    tool_error(AppError::invalid_input("list_id or list_name is required"))
                })?;

        self.state
            .runner
            .run_write_no_output(vec![
                "list".to_owned(),
                source_name,
                "--rename".to_owned(),
                input.new_name.clone(),
            ])
            .await
            .map_err(tool_error)?;

        let refreshed = self.fetch_lists().await.map_err(tool_error)?;
        let renamed = refreshed
            .into_iter()
            .find(|list| list.title == input.new_name)
            .ok_or_else(|| {
                tool_error(AppError::invalid_input(
                    "renamed list not found after operation",
                ))
            })?;

        Ok(Json(renamed))
    }

    #[tool(
        description = "Delete an existing list identified by listId or listName. This is destructive and force-deletes via remindctl."
    )]
    async fn list_delete(
        &self,
        Parameters(input): Parameters<ListDeleteInput>,
    ) -> Result<Json<ListDeleteResult>, String> {
        let lists = self.fetch_lists().await.map_err(tool_error)?;
        let source_name =
            resolve_list_name(&lists, input.list_id.as_deref(), input.list_name.as_deref())
                .map_err(tool_error)?
                .ok_or_else(|| {
                    tool_error(AppError::invalid_input("list_id or list_name is required"))
                })?;

        self.state
            .runner
            .run_write_no_output(vec![
                "list".to_owned(),
                source_name,
                "--delete".to_owned(),
                "--force".to_owned(),
            ])
            .await
            .map_err(tool_error)?;

        Ok(Json(ListDeleteResult { deleted: true }))
    }
}

#[tool_handler]
impl ServerHandler for AppServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
            instructions: Some("Manage Apple Reminders through remindctl. Use reminders_list as the default read entry point (omit filter to get pending reminders). For writes, prefer listId/reminderId and never rely on numeric indexes. If a short ID prefix is ambiguous, resolve it first with reminders_list or lists_list. For reminder_delete and reminder_complete, treat tool output as authoritative and avoid extra verification calls unless the tool returns an error.".to_owned()),
            ..Default::default()
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        Ok(self.get_info())
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![
                rmcp::model::RawResource {
                    uri: "remindctl://status".to_owned(),
                    name: "status".to_owned(),
                    title: Some("Remindctl Status".to_owned()),
                    description: Some(
                        "Current remindctl authorization state and status summary.".to_owned(),
                    ),
                    mime_type: Some("application/json".to_owned()),
                    size: None,
                    icons: None,
                    meta: None,
                }
                .no_annotation(),
                rmcp::model::RawResource {
                    uri: "remindctl://lists".to_owned(),
                    name: "lists".to_owned(),
                    title: Some("Reminder Lists".to_owned()),
                    description: Some(
                        "All reminders lists with IDs and aggregate counts.".to_owned(),
                    ),
                    mime_type: Some("application/json".to_owned()),
                    size: None,
                    icons: None,
                    meta: None,
                }
                .no_annotation(),
                rmcp::model::RawResource {
                    uri: "remindctl://server/config".to_owned(),
                    name: "server_config".to_owned(),
                    title: Some("Server Runtime Config".to_owned()),
                    description: Some(
                        "Effective non-secret runtime config: bind address, auth mode, and timeouts."
                            .to_owned(),
                    ),
                    mime_type: Some("application/json".to_owned()),
                    size: None,
                    icons: None,
                    meta: None,
                }
                .no_annotation(),
            ],
            next_cursor: None,
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![
                ResourceTemplate::new(
                    RawResourceTemplate {
                        uri_template: "remindctl://reminders/{filter}".to_owned(),
                        name: "reminders_filter".to_owned(),
                        title: Some("Reminders by Filter".to_owned()),
                        description: Some(
                            "Read reminders by filter. Supported values: pending, incomplete, today, tomorrow, week, overdue, upcoming, completed, all, or a date string."
                                .to_owned(),
                        ),
                        mime_type: Some("application/json".to_owned()),
                        icons: None,
                    },
                    None,
                ),
                ResourceTemplate::new(
                    RawResourceTemplate {
                        uri_template: "remindctl://lists/{list_id}/reminders".to_owned(),
                        name: "list_reminders".to_owned(),
                        title: Some("Reminders by List ID".to_owned()),
                        description: Some(
                            "Read all reminders in a list identified by list_id.".to_owned(),
                        ),
                        mime_type: Some("application/json".to_owned()),
                        icons: None,
                    },
                    None,
                ),
                ResourceTemplate::new(
                    RawResourceTemplate {
                        uri_template: "remindctl://lists/by-name/{list_name}/reminders".to_owned(),
                        name: "list_name_reminders".to_owned(),
                        title: Some("Reminders by List Name".to_owned()),
                        description: Some(
                            "Read all reminders in a list identified by list_name. Prefer list_id when available."
                                .to_owned(),
                        ),
                        mime_type: Some("application/json".to_owned()),
                        icons: None,
                    },
                    None,
                ),
            ],
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let uri = request.uri;

        if uri.as_str() == "remindctl://status" {
            let status = self
                .state
                .runner
                .run_read_json::<RemindctlStatus>(vec!["status".to_owned()])
                .await
                .map_err(to_mcp_error)?;
            let text = serde_json::to_string(&status).map_err(to_mcp_error)?;
            return Ok(ReadResourceResult {
                contents: vec![ResourceContents::text(text, uri)],
            });
        }

        if uri.as_str() == "remindctl://lists" {
            let lists = self.fetch_lists().await.map_err(to_mcp_error)?;
            let text = serde_json::to_string(&lists).map_err(to_mcp_error)?;
            return Ok(ReadResourceResult {
                contents: vec![ResourceContents::text(text, uri)],
            });
        }

        if uri.as_str() == "remindctl://server/config" {
            let config = ServerConfigResource {
                auth_required: self.state.config.auth_required,
                bind_addr: self.state.config.bind_addr.to_string(),
                read_timeout_secs: self.state.config.read_timeout.as_secs(),
                write_timeout_secs: self.state.config.write_timeout.as_secs(),
            };
            let text = serde_json::to_string(&config).map_err(to_mcp_error)?;
            return Ok(ReadResourceResult {
                contents: vec![ResourceContents::text(text, uri)],
            });
        }

        if let Some(filter) = uri
            .as_str()
            .strip_prefix("remindctl://reminders/")
            .filter(|value| !value.is_empty())
        {
            let reminders = self
                .state
                .runner
                .run_read_json::<Vec<Reminder>>(vec!["show".to_owned(), filter.to_owned()])
                .await
                .map_err(to_mcp_error)?;
            let text = serde_json::to_string(&reminders).map_err(to_mcp_error)?;
            return Ok(ReadResourceResult {
                contents: vec![ResourceContents::text(text, uri)],
            });
        }

        if let Some(list_id) = uri
            .as_str()
            .strip_prefix("remindctl://lists/")
            .and_then(|rest| rest.strip_suffix("/reminders"))
            .filter(|value| !value.is_empty())
        {
            let lists = self.fetch_lists().await.map_err(to_mcp_error)?;
            let list_name = resolve_list_name(&lists, Some(list_id), None).map_err(to_mcp_error)?;
            let Some(list_name) = list_name else {
                return Err(to_mcp_error("list not found"));
            };
            let reminders = self
                .state
                .runner
                .run_read_json::<Vec<Reminder>>(vec![
                    "show".to_owned(),
                    "all".to_owned(),
                    "--list".to_owned(),
                    list_name,
                ])
                .await
                .map_err(to_mcp_error)?;
            let text = serde_json::to_string(&reminders).map_err(to_mcp_error)?;
            return Ok(ReadResourceResult {
                contents: vec![ResourceContents::text(text, uri)],
            });
        }

        if let Some(list_name) = uri
            .as_str()
            .strip_prefix("remindctl://lists/by-name/")
            .and_then(|rest| rest.strip_suffix("/reminders"))
            .filter(|value| !value.is_empty())
        {
            let reminders = self
                .state
                .runner
                .run_read_json::<Vec<Reminder>>(vec![
                    "show".to_owned(),
                    "all".to_owned(),
                    "--list".to_owned(),
                    list_name.to_owned(),
                ])
                .await
                .map_err(to_mcp_error)?;
            let text = serde_json::to_string(&reminders).map_err(to_mcp_error)?;
            return Ok(ReadResourceResult {
                contents: vec![ResourceContents::text(text, uri)],
            });
        }

        Err(McpError::resource_not_found(
            "resource_not_found",
            Some(serde_json::json!({ "uri": uri.as_str() })),
        ))
    }
}

pub async fn auth_middleware(
    State(state): State<Arc<RuntimeState>>,
    headers: HeaderMap,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if !state.config.auth_required {
        return Ok(next.run(request).await);
    }

    let expected_key = match &state.config.api_key {
        Some(key) => key,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    let token = headers
        .get("authorization")
        .and_then(|header| header.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    match token {
        Some(value) if value == expected_key => Ok(next.run(request).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

fn tool_error(error: AppError) -> String {
    error.to_string()
}

fn to_mcp_error(error: impl ToString) -> McpError {
    McpError::internal_error(error.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_list(id: &str, title: &str) -> ReminderList {
        ReminderList {
            id: id.to_owned(),
            title: title.to_owned(),
            reminder_count: None,
            overdue_count: None,
        }
    }

    #[test]
    fn infer_best_list_prefers_keyword_overlap() {
        let lists = vec![
            mk_list("1", "Health"),
            mk_list("2", "Work"),
            mk_list("3", "Reminders"),
        ];

        let selected = AppServer::infer_best_list_name(
            &lists,
            "Book health check appointment",
            Some("find clinic next week"),
        );

        assert_eq!(selected.as_deref(), Some("Health"));
    }

    #[test]
    fn infer_best_list_falls_back_to_reminders() {
        let lists = vec![mk_list("1", "Reminders"), mk_list("2", "Work")];
        let selected = AppServer::infer_best_list_name(&lists, "Buy milk", None);
        assert_eq!(selected.as_deref(), Some("Reminders"));
    }

    #[test]
    fn infer_best_list_matches_spanish_prefix_comprar_compras() {
        let lists = vec![
            mk_list("1", "Compras"),
            mk_list("2", "Reminders"),
            mk_list("3", "Trabajo"),
        ];

        let selected = AppServer::infer_best_list_name(&lists, "Comprar Coca Zero lata", None);
        assert_eq!(selected.as_deref(), Some("Compras"));
    }
}
