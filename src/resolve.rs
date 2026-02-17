use crate::error::AppError;
use crate::models::{Reminder, ReminderList};

#[derive(Debug, Clone)]
pub struct ReminderResolution {
    pub resolved_ids: Vec<String>,
    pub missing_refs: Vec<String>,
}

pub fn validate_text_input(value: &str, field_name: &str, max_len: usize) -> Result<(), AppError> {
    if value.is_empty() {
        return Err(AppError::invalid_input(format!(
            "{field_name} cannot be empty"
        )));
    }
    if value.chars().count() > max_len {
        return Err(AppError::invalid_input(format!(
            "{field_name} exceeds max length {max_len}"
        )));
    }
    if value.chars().any(|ch| ch.is_control()) {
        return Err(AppError::invalid_input(format!(
            "{field_name} contains control characters"
        )));
    }
    Ok(())
}

pub fn resolve_list_name(
    lists: &[ReminderList],
    list_id: Option<&str>,
    list_name: Option<&str>,
) -> Result<Option<String>, AppError> {
    match (list_id, list_name) {
        (Some(id), Some(name)) => {
            let list = lists
                .iter()
                .find(|list| list.id.eq_ignore_ascii_case(id))
                .ok_or_else(|| AppError::invalid_input("list_id not found"))?;
            if list.title != name {
                return Err(AppError::invalid_input(
                    "list_id and list_name refer to different lists",
                ));
            }
            Ok(Some(name.to_owned()))
        }
        (Some(id), None) => {
            let list = lists
                .iter()
                .find(|list| list.id.eq_ignore_ascii_case(id))
                .ok_or_else(|| AppError::invalid_input("list_id not found"))?;
            Ok(Some(list.title.clone()))
        }
        (None, Some(name)) => {
            validate_text_input(name, "list_name", 120)?;
            Ok(Some(name.to_owned()))
        }
        (None, None) => Ok(None),
    }
}

pub fn resolve_reminder_ids(
    reminders: &[Reminder],
    raw_ids: &[String],
) -> Result<Vec<String>, AppError> {
    let mut resolved = Vec::with_capacity(raw_ids.len());

    for raw_id in raw_ids {
        if raw_id.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(AppError::invalid_input(format!(
                "ref '{raw_id}' looks like an index, provide id or unique prefix"
            )));
        }
        if raw_id.len() < 4 {
            return Err(AppError::invalid_input(format!(
                "ref '{raw_id}' is too short, use at least 4 chars"
            )));
        }

        let matches = reminders
            .iter()
            .filter(|reminder| {
                reminder
                    .id
                    .to_ascii_lowercase()
                    .starts_with(&raw_id.to_ascii_lowercase())
            })
            .map(|reminder| reminder.id.clone())
            .collect::<Vec<_>>();

        match matches.len() {
            0 => {
                return Err(AppError::invalid_input(format!(
                    "reminder ref '{raw_id}' not found"
                )));
            }
            1 => resolved.push(matches[0].clone()),
            _ => {
                let candidates = matches.join(", ");
                return Err(AppError::invalid_input(format!(
                    "reminder ref '{raw_id}' is ambiguous, candidates: {candidates}"
                )));
            }
        }
    }

    Ok(resolved)
}

pub fn resolve_reminder_ids_lenient(
    reminders: &[Reminder],
    raw_ids: &[String],
) -> Result<ReminderResolution, AppError> {
    let mut resolved = Vec::with_capacity(raw_ids.len());
    let mut missing = Vec::new();

    for raw_id in raw_ids {
        if raw_id.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(AppError::invalid_input(format!(
                "ref '{raw_id}' looks like an index, provide id or unique prefix"
            )));
        }
        if raw_id.len() < 4 {
            return Err(AppError::invalid_input(format!(
                "ref '{raw_id}' is too short, use at least 4 chars"
            )));
        }

        let matches = reminders
            .iter()
            .filter(|reminder| {
                reminder
                    .id
                    .to_ascii_lowercase()
                    .starts_with(&raw_id.to_ascii_lowercase())
            })
            .map(|reminder| reminder.id.clone())
            .collect::<Vec<_>>();

        match matches.len() {
            0 => missing.push(raw_id.clone()),
            1 => resolved.push(matches[0].clone()),
            _ => {
                let candidates = matches.join(", ");
                return Err(AppError::invalid_input(format!(
                    "reminder ref '{raw_id}' is ambiguous, candidates: {candidates}"
                )));
            }
        }
    }

    Ok(ReminderResolution {
        resolved_ids: resolved,
        missing_refs: missing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_reminder(id: &str) -> Reminder {
        Reminder {
            id: id.to_owned(),
            title: "x".to_owned(),
            list_id: "l1".to_owned(),
            list_name: "Reminders".to_owned(),
            is_completed: false,
            priority: "none".to_owned(),
            due_date: None,
            notes: String::new(),
        }
    }

    #[test]
    fn allows_emoji_list_names() {
        let result = validate_text_input("Reminders ⚠️", "list_name", 120);
        assert!(result.is_ok(), "emoji list names should be valid");
    }

    #[test]
    fn lenient_resolution_reports_missing_without_error() -> Result<(), String> {
        let reminders = vec![mk_reminder("AAAA-1111")];
        let refs = vec!["AAAA".to_owned(), "BBBB".to_owned()];
        let result = resolve_reminder_ids_lenient(&reminders, &refs)
            .map_err(|error| format!("lenient resolution unexpectedly failed: {error}"))?;

        assert_eq!(result.resolved_ids, vec!["AAAA-1111".to_owned()]);
        assert_eq!(result.missing_refs, vec!["BBBB".to_owned()]);
        Ok(())
    }
}
