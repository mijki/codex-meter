use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

const MAX_OTLP_BYTES: usize = 1024 * 1024;

#[derive(Debug, Error)]
pub enum OtelError {
    #[error("invalid local collector authorization")]
    Unauthorized,
    #[error("OTLP payload exceeds 1 MiB")]
    Oversized,
    #[error("malformed OTLP JSON: {0}")]
    Malformed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedOtelEvent {
    pub external_event_id: String,
    pub event_name: String,
    pub observed_at_unix_nano: Option<String>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub input_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub duration_ms: Option<u64>,
    pub outcome: Option<String>,
}

pub fn validate_authorization(
    header: Option<&str>,
    expected_secret: &str,
) -> Result<(), OtelError> {
    let expected = format!("Bearer {expected_secret}");
    if header == Some(expected.as_str()) {
        Ok(())
    } else {
        Err(OtelError::Unauthorized)
    }
}

pub fn normalize_otlp_json(bytes: &[u8]) -> Result<Vec<NormalizedOtelEvent>, OtelError> {
    if bytes.len() > MAX_OTLP_BYTES {
        return Err(OtelError::Oversized);
    }
    let value: Value =
        serde_json::from_slice(bytes).map_err(|error| OtelError::Malformed(error.to_string()))?;
    let mut result = Vec::new();
    let resources = value
        .get("resourceLogs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten();
    for resource in resources {
        let scopes = resource
            .get("scopeLogs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten();
        for scope in scopes {
            let records = scope
                .get("logRecords")
                .and_then(Value::as_array)
                .into_iter()
                .flatten();
            for record in records {
                let attrs = attributes(record.get("attributes"));
                let event_name = string_attr(&attrs, "event.name")
                    .or_else(|| record.get("body").and_then(any_string))
                    .unwrap_or_else(|| "unknown".to_owned());
                if event_name == "unknown" {
                    continue;
                }
                let external_event_id = string_attr(&attrs, "event.id").unwrap_or_else(|| {
                    format!(
                        "{}:{}",
                        event_name,
                        record
                            .get("timeUnixNano")
                            .and_then(Value::as_str)
                            .unwrap_or("missing-time")
                    )
                });
                result.push(NormalizedOtelEvent {
                    external_event_id,
                    event_name,
                    observed_at_unix_nano: record
                        .get("timeUnixNano")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    thread_id: string_attr(&attrs, "conversation.id")
                        .or_else(|| string_attr(&attrs, "thread.id")),
                    turn_id: string_attr(&attrs, "turn.id"),
                    model: string_attr(&attrs, "gen_ai.request.model")
                        .or_else(|| string_attr(&attrs, "model")),
                    reasoning_effort: string_attr(&attrs, "codex.reasoning_effort"),
                    input_tokens: integer_attr(&attrs, "gen_ai.usage.input_tokens"),
                    cached_input_tokens: integer_attr(&attrs, "codex.usage.cached_input_tokens"),
                    output_tokens: integer_attr(&attrs, "gen_ai.usage.output_tokens"),
                    reasoning_output_tokens: integer_attr(
                        &attrs,
                        "codex.usage.reasoning_output_tokens",
                    ),
                    total_tokens: integer_attr(&attrs, "codex.usage.total_tokens"),
                    duration_ms: integer_attr(&attrs, "duration_ms"),
                    outcome: string_attr(&attrs, "outcome"),
                });
            }
        }
    }
    Ok(result)
}

fn attributes(value: Option<&Value>) -> HashMap<String, Value> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some((
                item.get("key")?.as_str()?.to_owned(),
                item.get("value")?.clone(),
            ))
        })
        .collect()
}

fn any_string(value: &Value) -> Option<String> {
    value
        .get("stringValue")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn string_attr(attrs: &HashMap<String, Value>, key: &str) -> Option<String> {
    attrs.get(key).and_then(any_string)
}

fn integer_attr(attrs: &HashMap<String, Value>, key: &str) -> Option<u64> {
    attrs.get(key).and_then(|value| {
        value.get("intValue").and_then(|number| {
            number
                .as_str()
                .and_then(|raw| raw.parse().ok())
                .or_else(|| number.as_u64())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_authorization() {
        assert!(validate_authorization(Some("Bearer wrong"), "right").is_err());
        assert!(validate_authorization(Some("Bearer right"), "right").is_ok());
    }

    #[test]
    fn extracts_tokens_and_discards_prompt_content() {
        let payload = br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[{
          "timeUnixNano":"1784721600000000000","body":{"stringValue":"codex.api_request"},
          "attributes":[
            {"key":"event.id","value":{"stringValue":"otel-1"}},
            {"key":"conversation.id","value":{"stringValue":"thread-1"}},
            {"key":"gen_ai.usage.input_tokens","value":{"intValue":"1200"}},
            {"key":"codex.usage.cached_input_tokens","value":{"intValue":"300"}},
            {"key":"user.prompt","value":{"stringValue":"private prompt"}}
          ]
        }]}]}]}"#;
        let events = normalize_otlp_json(payload).expect("valid payload");
        assert_eq!(events[0].input_tokens, Some(1200));
        let serialized = serde_json::to_string(&events).expect("serializes");
        assert!(!serialized.contains("private prompt"));
    }

    #[test]
    fn ignores_unknown_records_and_rejects_malformed_payloads() {
        let empty = normalize_otlp_json(br#"{"resourceLogs":[]}"#).expect("empty is valid");
        assert!(empty.is_empty());
        assert!(matches!(
            normalize_otlp_json(b"{"),
            Err(OtelError::Malformed(_))
        ));
    }
}
