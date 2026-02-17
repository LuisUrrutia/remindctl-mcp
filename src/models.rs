use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Reminder {
    pub id: String,
    pub title: String,
    #[serde(rename = "listID")]
    pub list_id: String,
    #[serde(rename = "listName")]
    pub list_name: String,
    #[serde(rename = "isCompleted")]
    pub is_completed: bool,
    pub priority: String,
    #[serde(rename = "dueDate")]
    pub due_date: Option<String>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReminderList {
    pub id: String,
    pub title: String,
    #[serde(rename = "reminderCount")]
    pub reminder_count: Option<i64>,
    #[serde(rename = "overdueCount")]
    pub overdue_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RemindctlStatus {
    pub authorized: bool,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ServerHealth {
    pub ok: bool,
    #[serde(rename = "authRequired")]
    pub auth_required: bool,
    #[serde(rename = "remindctlAuthorized")]
    pub remindctl_authorized: bool,
    #[serde(rename = "remindctlStatus")]
    pub remindctl_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReminderListResult {
    pub reminders: Vec<Reminder>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListsResult {
    pub lists: Vec<ReminderList>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteResult {
    #[serde(rename = "deletedIds")]
    pub deleted_ids: Vec<String>,
    #[serde(rename = "deletedReminders")]
    pub deleted_reminders: Vec<Reminder>,
    #[serde(rename = "alreadyAbsentRefs")]
    pub already_absent_refs: Vec<String>,
    #[serde(rename = "usedRecentReference")]
    pub used_recent_reference: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListDeleteResult {
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BatchActionResult {
    pub id: String,
    pub op: String,
    pub ok: bool,
    pub error: Option<String>,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BatchProcessResult {
    pub processed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub results: Vec<BatchActionResult>,
}
