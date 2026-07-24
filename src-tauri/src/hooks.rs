use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

const MAX_HOOK_BYTES: usize = 256 * 1024;

#[derive(Debug, Error)]
pub enum HookError {
    #[error("hook payload exceeds 256 KiB")]
    Oversized,
    #[error("hook payload is malformed: {0}")]
    Malformed(String),
    #[error("hook payload is missing {0}")]
    Missing(&'static str),
    #[error("unsupported hook event {0}")]
    Unsupported(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedHookEvent {
    pub external_event_id: String,
    pub event_type: String,
    pub occurred_at: DateTime<Utc>,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub tool_category: Option<String>,
    pub succeeded: Option<bool>,
    pub duration_ms: Option<u64>,
    pub subagent_id: Option<String>,
    pub subagent_type: Option<String>,
}

pub fn normalize_hook_json(bytes: &[u8]) -> Result<NormalizedHookEvent, HookError> {
    if bytes.len() > MAX_HOOK_BYTES {
        return Err(HookError::Oversized);
    }
    let value: Value =
        serde_json::from_slice(bytes).map_err(|error| HookError::Malformed(error.to_string()))?;
    let string = |snake: &str, camel: &str| {
        value
            .get(snake)
            .or_else(|| value.get(camel))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let event_type =
        string("hook_event_name", "hookEventName").ok_or(HookError::Missing("hook_event_name"))?;
    const SUPPORTED: &[&str] = &[
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PreCompact",
        "PostCompact",
        "SubagentStart",
        "SubagentStop",
        "Stop",
    ];
    if !SUPPORTED.contains(&event_type.as_str()) {
        return Err(HookError::Unsupported(event_type));
    }
    let occurred_at = string("timestamp", "timestamp")
        .and_then(|raw| raw.parse::<DateTime<Utc>>().ok())
        .unwrap_or_else(Utc::now);
    Ok(NormalizedHookEvent {
        external_event_id: string("event_id", "eventId").ok_or(HookError::Missing("event_id"))?,
        event_type,
        occurred_at,
        session_id: string("session_id", "sessionId").ok_or(HookError::Missing("session_id"))?,
        turn_id: string("turn_id", "turnId"),
        model: string("model", "model"),
        permission_mode: string("permission_mode", "permissionMode"),
        tool_category: string("tool_category", "toolCategory"),
        succeeded: value.get("succeeded").and_then(Value::as_bool),
        duration_ms: value
            .get("duration_ms")
            .or_else(|| value.get("durationMs"))
            .and_then(Value::as_u64),
        subagent_id: string("subagent_id", "subagentId"),
        subagent_type: string("subagent_type", "subagentType"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discards_prompt_and_tool_arguments() {
        let raw = br#"{
          "event_id":"hook-1","hook_event_name":"UserPromptSubmit","timestamp":"2026-07-22T10:00:00Z",
          "session_id":"thread-1","turn_id":"turn-1","model":"fixture-model",
          "prompt":"private prompt must disappear","tool_input":{"command":"secret command"}
        }"#;
        let normalized = normalize_hook_json(raw).expect("normalizes");
        let serialized = serde_json::to_string(&normalized).expect("serializes");
        assert!(!serialized.contains("private prompt"));
        assert!(!serialized.contains("secret command"));
    }

    #[test]
    fn rejects_unknown_or_malformed_events() {
        assert!(matches!(
            normalize_hook_json(b"{"),
            Err(HookError::Malformed(_))
        ));
        let unknown = br#"{"event_id":"1","hook_event_name":"FutureHook","session_id":"s"}"#;
        assert!(matches!(
            normalize_hook_json(unknown),
            Err(HookError::Unsupported(_))
        ));
    }
}
