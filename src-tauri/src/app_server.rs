use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;

pub const METHOD_INITIALIZE: &str = "initialize";
pub const METHOD_INITIALIZED: &str = "initialized";
pub const METHOD_RATE_LIMITS_READ: &str = "account/rateLimits/read";
pub const METHOD_ACCOUNT_USAGE_READ: &str = "account/usage/read";
pub const NOTIFICATION_RATE_LIMITS_UPDATED: &str = "account/rateLimits/updated";
pub const NOTIFICATION_THREAD_TOKEN_USAGE_UPDATED: &str = "thread/tokenUsage/updated";

const JSONRPC_VERSION: &str = "2.0";
const JSONRPC_METHOD_NOT_FOUND: i64 = -32_601;
const DEFAULT_MAX_MESSAGE_BYTES: usize = 16_384;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("message exceeds the configured size limit")]
    Oversized,
    #[error("malformed JSON-RPC message: {0}")]
    Malformed(String),
    #[error("JSON-RPC message is missing a method, result, or error")]
    UnknownShape,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CommandSpecError {
    #[error("app server executable must not be blank")]
    EmptyExecutable,
    #[error("app server executable contains an unsupported control character")]
    InvalidExecutable,
    #[error("app server argument {index} contains an unsupported control character")]
    InvalidArgument { index: usize },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RequestError {
    #[error("app server is not ready to accept requests")]
    NotReady,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Response {
        id: Value,
        result: Value,
    },
    Error {
        id: Value,
        code: i64,
        message: String,
    },
    Notification {
        method: String,
        params: Value,
    },
    Request {
        id: Value,
        method: String,
        params: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppServerCommand {
    executable: String,
    args: Vec<String>,
}

impl AppServerCommand {
    pub fn new<I, S>(executable: impl Into<String>, args: I) -> Result<Self, CommandSpecError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let executable = executable.into();
        if executable.trim().is_empty() {
            return Err(CommandSpecError::EmptyExecutable);
        }
        if contains_control_character(&executable) {
            return Err(CommandSpecError::InvalidExecutable);
        }

        let mut normalized_args = Vec::new();
        for (index, arg) in args.into_iter().enumerate() {
            let arg = arg.into();
            if contains_control_character(&arg) {
                return Err(CommandSpecError::InvalidArgument { index });
            }
            normalized_args.push(arg);
        }

        Ok(Self {
            executable,
            args: normalized_args,
        })
    }

    pub fn executable(&self) -> &str {
        &self.executable
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthState {
    Disabled,
    Starting,
    Initializing,
    Healthy,
    Degraded,
    BackingOff,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeoutKind {
    Startup,
    Initialize,
    Request { id: u64, method: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeoutPolicy {
    pub startup_timeout: Duration,
    pub initialize_timeout: Duration,
    pub request_timeout: Duration,
}

impl Default for TimeoutPolicy {
    fn default() -> Self {
        Self {
            startup_timeout: Duration::from_secs(5),
            initialize_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SupervisorAction {
    Spawn { command: AppServerCommand },
    SendLine(String),
    Terminate,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SupervisorEvent {
    SpawnRequested,
    InitializeRequested {
        id: u64,
        params: Value,
    },
    Initialized {
        result: Value,
    },
    Response {
        id: u64,
        method: String,
        result: Value,
    },
    ErrorResponse {
        id: Value,
        method: Option<String>,
        code: i64,
        message: String,
    },
    Notification {
        method: String,
        params: Value,
    },
    UnsupportedRequest {
        id: Value,
        method: String,
    },
    UnknownResponse {
        id: Value,
    },
    MalformedOutput {
        detail: String,
    },
    Timeout(TimeoutKind),
    Exited {
        status: Option<i32>,
    },
    RestartScheduled {
        attempt: u32,
        restart_at: Duration,
    },
    RestartLimitReached,
    Paused,
    Resumed,
    ShutdownRequested,
    Stopped,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct SupervisorUpdate {
    pub actions: Vec<SupervisorAction>,
    pub events: Vec<SupervisorEvent>,
}

impl SupervisorUpdate {
    fn push_action(&mut self, action: SupervisorAction) {
        self.actions.push(action);
    }

    fn push_event(&mut self, event: SupervisorEvent) {
        self.events.push(event);
    }

    fn extend(&mut self, other: SupervisorUpdate) {
        self.actions.extend(other.actions);
        self.events.extend(other.events);
    }
}

pub fn parse_message(line: &str, max_bytes: usize) -> Result<Message, ProtocolError> {
    if line.len() > max_bytes {
        return Err(ProtocolError::Oversized);
    }
    let value: Value =
        serde_json::from_str(line).map_err(|error| ProtocolError::Malformed(error.to_string()))?;
    let object = value.as_object().ok_or(ProtocolError::UnknownShape)?;
    let id = object.get("id").cloned();
    if let Some(method) = object.get("method").and_then(Value::as_str) {
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        return Ok(match id {
            Some(id) => Message::Request {
                id,
                method: method.to_owned(),
                params,
            },
            None => Message::Notification {
                method: method.to_owned(),
                params,
            },
        });
    }
    if let (Some(id), Some(result)) = (id.clone(), object.get("result")) {
        return Ok(Message::Response {
            id,
            result: result.clone(),
        });
    }
    if let (Some(id), Some(error)) = (id, object.get("error")) {
        return Ok(Message::Error {
            id,
            code: error.get("code").and_then(Value::as_i64).unwrap_or(-1),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("App Server error")
                .to_owned(),
        });
    }
    Err(ProtocolError::UnknownShape)
}

#[derive(Debug, Default)]
pub struct RequestTracker {
    next_id: u64,
    pending: HashMap<u64, String>,
}

impl RequestTracker {
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        self.pending.insert(self.next_id, method.to_owned());
        let mut request = json!({
            "jsonrpc": JSONRPC_VERSION,
            "id": self.next_id,
            "method": method
        });
        if !params.is_null() {
            request["params"] = params;
        }
        request
    }

    pub fn complete(&mut self, id: u64) -> Option<String> {
        self.pending.remove(&id)
    }

    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

#[derive(Debug, Clone)]
pub struct RestartPolicy {
    pub maximum_restarts: u32,
    pub base_delay: Duration,
    pub maximum_delay: Duration,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            maximum_restarts: 3,
            base_delay: Duration::from_millis(500),
            maximum_delay: Duration::from_secs(8),
        }
    }
}

impl RestartPolicy {
    pub fn delay_for(&self, attempt: u32) -> Option<Duration> {
        if attempt >= self.maximum_restarts {
            return None;
        }
        let factor = 2_u32.saturating_pow(attempt);
        Some((self.base_delay * factor).min(self.maximum_delay))
    }

    pub fn jittered_delay_for(&self, attempt: u32) -> Option<Duration> {
        let base = self.delay_for(attempt)?;
        let jitter_bound_ms = (base.as_millis() / 4) as u64;
        if jitter_bound_ms == 0 {
            return Some(base);
        }
        let jitter_ms = deterministic_jitter_ms(attempt, jitter_bound_ms);
        Some((base + Duration::from_millis(jitter_ms)).min(self.maximum_delay))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingRequestKind {
    Initialize,
    Standard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingRequest {
    method: String,
    deadline: Duration,
    kind: PendingRequestKind,
}

#[derive(Debug)]
pub struct AppServerSupervisor {
    command: AppServerCommand,
    initialize_params: Value,
    restart_policy: RestartPolicy,
    timeouts: TimeoutPolicy,
    request_tracker: RequestTracker,
    pending_requests: HashMap<u64, PendingRequest>,
    max_message_bytes: usize,
    health: HealthState,
    child_running: bool,
    restart_count: u32,
    startup_deadline: Option<Duration>,
    restart_at: Option<Duration>,
    shutdown_requested: bool,
}

impl AppServerSupervisor {
    pub fn new(command: AppServerCommand) -> Self {
        Self {
            command,
            initialize_params: default_initialize_params(),
            restart_policy: RestartPolicy::default(),
            timeouts: TimeoutPolicy::default(),
            request_tracker: RequestTracker::default(),
            pending_requests: HashMap::new(),
            max_message_bytes: DEFAULT_MAX_MESSAGE_BYTES,
            health: HealthState::Disabled,
            child_running: false,
            restart_count: 0,
            startup_deadline: None,
            restart_at: None,
            shutdown_requested: false,
        }
    }

    pub fn with_initialize_params(mut self, initialize_params: Value) -> Self {
        self.initialize_params = initialize_params;
        self
    }

    pub fn with_restart_policy(mut self, restart_policy: RestartPolicy) -> Self {
        self.restart_policy = restart_policy;
        self
    }

    pub fn with_timeouts(mut self, timeouts: TimeoutPolicy) -> Self {
        self.timeouts = timeouts;
        self
    }

    pub fn with_max_message_bytes(mut self, max_message_bytes: usize) -> Self {
        self.max_message_bytes = max_message_bytes;
        self
    }

    pub fn health(&self) -> HealthState {
        self.health.clone()
    }

    pub fn restart_count(&self) -> u32 {
        self.restart_count
    }

    pub fn pending_request_count(&self) -> usize {
        self.pending_requests.len()
    }

    pub fn next_restart_at(&self) -> Option<Duration> {
        self.restart_at
    }

    pub fn start(&mut self, now: Duration) -> SupervisorUpdate {
        self.shutdown_requested = false;
        self.begin_start(now, true)
    }

    pub fn on_child_started(&mut self, now: Duration) -> SupervisorUpdate {
        let mut update = SupervisorUpdate::default();
        self.child_running = true;

        match self.health {
            HealthState::Disabled | HealthState::Stopped | HealthState::BackingOff => {
                update.push_action(SupervisorAction::Terminate);
                return update;
            }
            HealthState::Starting | HealthState::Failed => {}
            HealthState::Initializing | HealthState::Healthy | HealthState::Degraded => {
                return update;
            }
        }

        self.startup_deadline = None;
        self.health = HealthState::Initializing;
        let (id, line) = self.register_request(
            METHOD_INITIALIZE,
            self.initialize_params.clone(),
            now + self.timeouts.initialize_timeout,
            PendingRequestKind::Initialize,
        );
        update.push_event(SupervisorEvent::InitializeRequested {
            id,
            params: self.initialize_params.clone(),
        });
        update.push_action(SupervisorAction::SendLine(line));
        update
    }

    pub fn send_request(
        &mut self,
        method: &str,
        params: Value,
        now: Duration,
    ) -> Result<SupervisorUpdate, RequestError> {
        if !matches!(self.health, HealthState::Healthy | HealthState::Degraded) {
            return Err(RequestError::NotReady);
        }

        let (_, line) = self.register_request(
            method,
            params,
            now + self.timeouts.request_timeout,
            PendingRequestKind::Standard,
        );
        Ok(SupervisorUpdate {
            actions: vec![SupervisorAction::SendLine(line)],
            events: Vec::new(),
        })
    }

    pub fn receive_line(&mut self, line: &str, now: Duration) -> SupervisorUpdate {
        let message = match parse_message(line, self.max_message_bytes) {
            Ok(message) => message,
            Err(error) => {
                self.degrade();
                return SupervisorUpdate {
                    actions: Vec::new(),
                    events: vec![SupervisorEvent::MalformedOutput {
                        detail: error.to_string(),
                    }],
                };
            }
        };

        match message {
            Message::Notification { method, params } => SupervisorUpdate {
                actions: Vec::new(),
                events: vec![SupervisorEvent::Notification { method, params }],
            },
            Message::Request { id, method, .. } => {
                self.degrade();
                SupervisorUpdate {
                    actions: vec![SupervisorAction::SendLine(method_not_found_line(
                        id.clone(),
                        format!("Unsupported server request: {method}"),
                    ))],
                    events: vec![SupervisorEvent::UnsupportedRequest { id, method }],
                }
            }
            Message::Response { id, result } => self.handle_response(id, result),
            Message::Error { id, code, message } => self.handle_error(id, code, message, now),
        }
    }

    pub fn poll(&mut self, now: Duration) -> SupervisorUpdate {
        let mut update = SupervisorUpdate::default();

        if matches!(self.health, HealthState::Starting)
            && self
                .startup_deadline
                .is_some_and(|deadline| now >= deadline)
        {
            update.push_event(SupervisorEvent::Timeout(TimeoutKind::Startup));
            self.schedule_restart(now, &mut update, false);
            return update;
        }

        if matches!(self.health, HealthState::Initializing) {
            if let Some((&id, pending)) = self
                .pending_requests
                .iter()
                .find(|(_, pending)| matches!(pending.kind, PendingRequestKind::Initialize))
            {
                if now >= pending.deadline {
                    self.pending_requests.remove(&id);
                    self.request_tracker.complete(id);
                    update.push_event(SupervisorEvent::Timeout(TimeoutKind::Initialize));
                    self.schedule_restart(now, &mut update, self.child_running);
                    return update;
                }
            }
        }

        let expired_requests: Vec<(u64, String)> = self
            .pending_requests
            .iter()
            .filter_map(|(&id, pending)| {
                if matches!(pending.kind, PendingRequestKind::Standard) && now >= pending.deadline {
                    Some((id, pending.method.clone()))
                } else {
                    None
                }
            })
            .collect();

        if !expired_requests.is_empty() {
            self.degrade();
            for (id, method) in expired_requests {
                self.pending_requests.remove(&id);
                self.request_tracker.complete(id);
                update.push_event(SupervisorEvent::Timeout(TimeoutKind::Request {
                    id,
                    method,
                }));
            }
        }

        if matches!(self.health, HealthState::BackingOff)
            && self.restart_at.is_some_and(|restart_at| now >= restart_at)
            && !self.shutdown_requested
        {
            self.restart_at = None;
            update.extend(self.begin_start(now, false));
        }

        update
    }

    pub fn on_child_exit(&mut self, status: Option<i32>, now: Duration) -> SupervisorUpdate {
        self.child_running = false;
        self.startup_deadline = None;
        self.restart_at = None;
        self.clear_in_flight();

        let mut update = SupervisorUpdate {
            actions: Vec::new(),
            events: vec![SupervisorEvent::Exited { status }],
        };

        if self.shutdown_requested || matches!(self.health, HealthState::Stopped) {
            self.health = HealthState::Stopped;
            update.push_event(SupervisorEvent::Stopped);
            return update;
        }

        if matches!(self.health, HealthState::Disabled) {
            return update;
        }

        self.schedule_restart(now, &mut update, false);
        update
    }

    pub fn pause(&mut self) -> SupervisorUpdate {
        self.shutdown_requested = false;
        self.startup_deadline = None;
        self.restart_at = None;
        self.clear_in_flight();
        self.health = HealthState::Disabled;

        let mut update = SupervisorUpdate {
            actions: Vec::new(),
            events: vec![SupervisorEvent::Paused],
        };
        if self.child_running {
            update.push_action(SupervisorAction::Terminate);
        }
        update
    }

    pub fn resume(&mut self, now: Duration) -> SupervisorUpdate {
        let mut update = SupervisorUpdate {
            actions: Vec::new(),
            events: vec![SupervisorEvent::Resumed],
        };
        self.shutdown_requested = false;
        update.extend(self.begin_start(now, true));
        update
    }

    pub fn shutdown(&mut self) -> SupervisorUpdate {
        self.shutdown_requested = true;
        self.startup_deadline = None;
        self.restart_at = None;
        self.clear_in_flight();
        self.health = HealthState::Stopped;

        let mut update = SupervisorUpdate {
            actions: Vec::new(),
            events: vec![SupervisorEvent::ShutdownRequested],
        };
        if self.child_running {
            update.push_action(SupervisorAction::Terminate);
        }
        update
    }

    fn begin_start(&mut self, now: Duration, reset_restart_count: bool) -> SupervisorUpdate {
        self.clear_in_flight();
        self.child_running = false;
        self.restart_at = None;
        self.health = HealthState::Starting;
        self.startup_deadline = Some(now + self.timeouts.startup_timeout);
        if reset_restart_count {
            self.restart_count = 0;
        }

        SupervisorUpdate {
            actions: vec![SupervisorAction::Spawn {
                command: self.command.clone(),
            }],
            events: vec![SupervisorEvent::SpawnRequested],
        }
    }

    fn register_request(
        &mut self,
        method: &str,
        params: Value,
        deadline: Duration,
        kind: PendingRequestKind,
    ) -> (u64, String) {
        let request = self.request_tracker.request(method, params);
        let id = request
            .get("id")
            .and_then(Value::as_u64)
            .expect("request ids must be monotonic u64 values");
        self.pending_requests.insert(
            id,
            PendingRequest {
                method: method.to_owned(),
                deadline,
                kind,
            },
        );
        (id, jsonrpc_line(request))
    }

    fn handle_response(&mut self, id: Value, result: Value) -> SupervisorUpdate {
        let mut update = SupervisorUpdate::default();
        let Some(id_u64) = response_id(&id) else {
            self.degrade();
            update.push_event(SupervisorEvent::UnknownResponse { id });
            return update;
        };

        let Some(pending) = self.pending_requests.remove(&id_u64) else {
            self.degrade();
            update.push_event(SupervisorEvent::UnknownResponse { id });
            return update;
        };
        self.request_tracker.complete(id_u64);

        match pending.kind {
            PendingRequestKind::Initialize => {
                self.health = HealthState::Healthy;
                update.push_event(SupervisorEvent::Initialized {
                    result: result.clone(),
                });
                update.push_action(SupervisorAction::SendLine(initialized_line()));
            }
            PendingRequestKind::Standard => {
                update.push_event(SupervisorEvent::Response {
                    id: id_u64,
                    method: pending.method,
                    result,
                });
            }
        }

        update
    }

    fn handle_error(
        &mut self,
        id: Value,
        code: i64,
        message: String,
        now: Duration,
    ) -> SupervisorUpdate {
        let mut update = SupervisorUpdate::default();
        let method = response_id(&id).and_then(|id_u64| {
            let pending = self.pending_requests.remove(&id_u64)?;
            self.request_tracker.complete(id_u64);
            Some(pending.method)
        });

        update.push_event(SupervisorEvent::ErrorResponse {
            id: id.clone(),
            method: method.clone(),
            code,
            message: message.clone(),
        });

        if method.as_deref() == Some(METHOD_INITIALIZE) {
            self.schedule_restart(now, &mut update, self.child_running);
            return update;
        }

        if method.is_none() {
            self.degrade();
            update.push_event(SupervisorEvent::UnknownResponse { id });
        }

        update
    }

    fn schedule_restart(
        &mut self,
        now: Duration,
        update: &mut SupervisorUpdate,
        terminate_child: bool,
    ) {
        self.child_running = false;
        self.startup_deadline = None;
        self.clear_in_flight();

        if terminate_child {
            update.push_action(SupervisorAction::Terminate);
        }

        let attempt_index = self.restart_count;
        let Some(delay) = self.restart_policy.jittered_delay_for(attempt_index) else {
            self.restart_at = None;
            self.health = HealthState::Failed;
            update.push_event(SupervisorEvent::RestartLimitReached);
            return;
        };

        self.restart_count = self.restart_count.saturating_add(1);
        let restart_at = now + delay;
        self.restart_at = Some(restart_at);
        self.health = HealthState::BackingOff;
        update.push_event(SupervisorEvent::RestartScheduled {
            attempt: self.restart_count,
            restart_at,
        });
    }

    fn clear_in_flight(&mut self) {
        self.pending_requests.clear();
        self.request_tracker.clear_pending();
    }

    fn degrade(&mut self) {
        if matches!(
            self.health,
            HealthState::Starting | HealthState::Initializing | HealthState::Healthy
        ) {
            self.health = HealthState::Degraded;
        }
    }
}

fn contains_control_character(value: &str) -> bool {
    value.contains('\0') || value.contains('\n') || value.contains('\r')
}

fn deterministic_jitter_ms(attempt: u32, upper_bound_ms: u64) -> u64 {
    let mixed = (attempt as u64)
        .wrapping_mul(1_103_515_245)
        .wrapping_add(12_345)
        .rotate_left(7);
    mixed % (upper_bound_ms + 1)
}

fn default_initialize_params() -> Value {
    json!({
        "clientInfo": {
            "name": "codex-meter",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "capabilities": Value::Null,
    })
}

fn response_id(id: &Value) -> Option<u64> {
    id.as_u64()
}

fn jsonrpc_line(value: Value) -> String {
    let mut encoded = value.to_string();
    encoded.push('\n');
    encoded
}

fn initialized_line() -> String {
    jsonrpc_line(json!({
        "jsonrpc": JSONRPC_VERSION,
        "method": METHOD_INITIALIZED,
    }))
}

fn method_not_found_line(id: Value, message: String) -> String {
    jsonrpc_line(json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "error": {
            "code": JSONRPC_METHOD_NOT_FOUND,
            "message": message,
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_command() -> AppServerCommand {
        AppServerCommand::new("codex", ["app-server", "--stdio"]).expect("valid command")
    }

    fn test_supervisor() -> AppServerSupervisor {
        AppServerSupervisor::new(test_command()).with_timeouts(TimeoutPolicy {
            startup_timeout: Duration::from_millis(10),
            initialize_timeout: Duration::from_millis(10),
            request_timeout: Duration::from_millis(10),
        })
    }

    fn initialize(supervisor: &mut AppServerSupervisor) {
        let start = supervisor.start(Duration::ZERO);
        assert!(matches!(
            start.actions.as_slice(),
            [SupervisorAction::Spawn { .. }]
        ));

        let started = supervisor.on_child_started(Duration::from_millis(1));
        let SupervisorAction::SendLine(line) = &started.actions[0] else {
            panic!("expected initialize line");
        };
        let message = parse_message(line.trim_end(), 16_384).expect("valid initialize request");
        let init_id = match message {
            Message::Request { id, method, .. } => {
                assert_eq!(method, METHOD_INITIALIZE);
                id.as_u64().expect("numeric initialize id")
            }
            other => panic!("unexpected initialize message: {other:?}"),
        };

        let response = supervisor.receive_line(
            &json!({
                "jsonrpc": "2.0",
                "id": init_id,
                "result": {
                    "userAgent": "codex-test",
                    "codexHome": "C:/tmp/codex",
                    "platformFamily": "windows",
                    "platformOs": "windows"
                }
            })
            .to_string(),
            Duration::from_millis(2),
        );
        assert!(matches!(supervisor.health(), HealthState::Healthy));
        assert!(response.actions.iter().any(|action| matches!(
            action,
            SupervisorAction::SendLine(line) if line.contains(METHOD_INITIALIZED)
        )));
    }

    #[test]
    fn startup_and_initialize_handshake_are_newline_delimited() {
        let mut supervisor = test_supervisor();

        let start = supervisor.start(Duration::ZERO);
        assert_eq!(supervisor.health(), HealthState::Starting);
        assert!(matches!(
            start.events.as_slice(),
            [SupervisorEvent::SpawnRequested]
        ));
        assert!(matches!(
            start.actions.as_slice(),
            [SupervisorAction::Spawn { command }]
                if command.executable() == "codex"
                && command.args() == ["app-server", "--stdio"]
        ));

        let started = supervisor.on_child_started(Duration::from_millis(1));
        assert_eq!(supervisor.health(), HealthState::Initializing);
        let SupervisorAction::SendLine(line) = &started.actions[0] else {
            panic!("expected initialize line");
        };
        assert!(line.ends_with('\n'));

        let message = parse_message(line.trim_end(), 16_384).expect("valid initialize request");
        let init_id = match message {
            Message::Request { id, method, params } => {
                assert_eq!(method, METHOD_INITIALIZE);
                assert_eq!(params["clientInfo"]["name"], "codex-meter");
                id.as_u64().expect("numeric initialize id")
            }
            other => panic!("unexpected initialize message: {other:?}"),
        };

        let response = supervisor.receive_line(
            &json!({
                "jsonrpc": "2.0",
                "id": init_id,
                "result": {
                    "userAgent": "codex-test",
                    "codexHome": "C:/tmp/codex",
                    "platformFamily": "windows",
                    "platformOs": "windows"
                }
            })
            .to_string(),
            Duration::from_millis(2),
        );
        assert_eq!(supervisor.health(), HealthState::Healthy);
        assert!(matches!(
            response.events.as_slice(),
            [SupervisorEvent::Initialized { .. }]
        ));
        assert!(matches!(
            response.actions.as_slice(),
            [SupervisorAction::SendLine(line)]
                if line.ends_with('\n')
                && line.contains(METHOD_INITIALIZED)
        ));
    }

    #[test]
    fn correlates_requests_and_keeps_ids_monotonic() {
        let mut supervisor = test_supervisor();
        initialize(&mut supervisor);

        let first = supervisor
            .send_request(
                METHOD_RATE_LIMITS_READ,
                Value::Null,
                Duration::from_millis(3),
            )
            .expect("request accepted");
        let first_id = match parse_message(
            match &first.actions[0] {
                SupervisorAction::SendLine(line) => line.trim_end(),
                _ => panic!("expected request line"),
            },
            16_384,
        )
        .expect("valid request")
        {
            Message::Request { id, .. } => id.as_u64().expect("numeric id"),
            other => panic!("unexpected request: {other:?}"),
        };

        let second = supervisor
            .send_request(
                METHOD_ACCOUNT_USAGE_READ,
                Value::Null,
                Duration::from_millis(4),
            )
            .expect("request accepted");
        let second_id = match parse_message(
            match &second.actions[0] {
                SupervisorAction::SendLine(line) => line.trim_end(),
                _ => panic!("expected request line"),
            },
            16_384,
        )
        .expect("valid request")
        {
            Message::Request { id, .. } => id.as_u64().expect("numeric id"),
            other => panic!("unexpected request: {other:?}"),
        };

        assert_eq!(first_id + 1, second_id);

        let response = supervisor.receive_line(
            &json!({
                "jsonrpc": "2.0",
                "id": first_id,
                "result": { "ok": true }
            })
            .to_string(),
            Duration::from_millis(5),
        );
        assert!(matches!(
            response.events.as_slice(),
            [SupervisorEvent::Response { id, method, .. }]
                if *id == first_id && method == METHOD_RATE_LIMITS_READ
        ));
        assert_eq!(supervisor.pending_request_count(), 1);
    }

    #[test]
    fn passes_notifications_without_degrading() {
        let mut supervisor = test_supervisor();
        initialize(&mut supervisor);

        let update = supervisor.receive_line(
            r#"{"jsonrpc":"2.0","method":"account/rateLimits/updated","params":{"limitId":"primary","usedPercent":12}}"#,
            Duration::from_millis(3),
        );
        assert_eq!(supervisor.health(), HealthState::Healthy);
        assert!(matches!(
            update.events.as_slice(),
            [SupervisorEvent::Notification { method, .. }]
                if method == NOTIFICATION_RATE_LIMITS_UPDATED
        ));
    }

    #[test]
    fn malformed_output_marks_supervisor_degraded() {
        let mut supervisor = test_supervisor();
        initialize(&mut supervisor);

        let update = supervisor.receive_line("{", Duration::from_millis(3));
        assert_eq!(supervisor.health(), HealthState::Degraded);
        assert!(matches!(
            update.events.as_slice(),
            [SupervisorEvent::MalformedOutput { .. }]
        ));
    }

    #[test]
    fn unsupported_server_requests_return_method_not_found() {
        let mut supervisor = test_supervisor();
        initialize(&mut supervisor);

        let update = supervisor.receive_line(
            r#"{"jsonrpc":"2.0","id":99,"method":"server/ping","params":{"x":1}}"#,
            Duration::from_millis(3),
        );
        assert_eq!(supervisor.health(), HealthState::Degraded);
        assert!(matches!(
            update.events.as_slice(),
            [SupervisorEvent::UnsupportedRequest { method, .. }]
                if method == "server/ping"
        ));
        assert!(matches!(
            update.actions.as_slice(),
            [SupervisorAction::SendLine(line)]
                if line.contains("\"code\":-32601")
        ));
    }

    #[test]
    fn request_timeout_degrades_and_clears_pending_request() {
        let mut supervisor = test_supervisor();
        initialize(&mut supervisor);

        supervisor
            .send_request(
                METHOD_RATE_LIMITS_READ,
                Value::Null,
                Duration::from_millis(3),
            )
            .expect("request accepted");
        let update = supervisor.poll(Duration::from_millis(20));
        assert_eq!(supervisor.health(), HealthState::Degraded);
        assert_eq!(supervisor.pending_request_count(), 0);
        assert!(matches!(
            update.events.as_slice(),
            [SupervisorEvent::Timeout(TimeoutKind::Request { method, .. })]
                if method == METHOD_RATE_LIMITS_READ
        ));
    }

    #[test]
    fn startup_timeout_enters_backoff_with_deterministic_jitter() {
        let mut supervisor = test_supervisor().with_restart_policy(RestartPolicy {
            maximum_restarts: 2,
            base_delay: Duration::from_millis(20),
            maximum_delay: Duration::from_millis(100),
        });

        supervisor.start(Duration::ZERO);
        let update = supervisor.poll(Duration::from_millis(11));
        assert_eq!(supervisor.health(), HealthState::BackingOff);
        assert_eq!(supervisor.restart_count(), 1);
        let restart_at = supervisor.next_restart_at().expect("restart scheduled");
        assert!(restart_at > Duration::from_millis(11));
        assert!(restart_at <= Duration::from_millis(36));
        assert!(matches!(
            update.events.as_slice(),
            [
                SupervisorEvent::Timeout(TimeoutKind::Startup),
                SupervisorEvent::RestartScheduled { attempt, .. }
            ] if *attempt == 1
        ));
    }

    #[test]
    fn initialize_timeout_terminates_child_and_restarts() {
        let mut supervisor = test_supervisor().with_restart_policy(RestartPolicy {
            maximum_restarts: 2,
            base_delay: Duration::from_millis(20),
            maximum_delay: Duration::from_millis(100),
        });

        supervisor.start(Duration::ZERO);
        supervisor.on_child_started(Duration::from_millis(1));

        let update = supervisor.poll(Duration::from_millis(20));
        assert_eq!(supervisor.health(), HealthState::BackingOff);
        assert!(update.actions.contains(&SupervisorAction::Terminate));
        assert!(matches!(
            update.events.as_slice(),
            [
                SupervisorEvent::Timeout(TimeoutKind::Initialize),
                SupervisorEvent::RestartScheduled { attempt, .. }
            ] if *attempt == 1
        ));
    }

    #[test]
    fn unexpected_exit_restarts_with_bounded_backoff() {
        let mut supervisor = test_supervisor().with_restart_policy(RestartPolicy {
            maximum_restarts: 2,
            base_delay: Duration::from_millis(20),
            maximum_delay: Duration::from_millis(60),
        });
        initialize(&mut supervisor);

        let exit = supervisor.on_child_exit(Some(1), Duration::from_millis(5));
        assert_eq!(supervisor.health(), HealthState::BackingOff);
        assert_eq!(supervisor.restart_count(), 1);
        assert!(matches!(
            exit.events.as_slice(),
            [
                SupervisorEvent::Exited { status: Some(1) },
                SupervisorEvent::RestartScheduled { attempt, .. }
            ] if *attempt == 1
        ));

        let before_due = supervisor.poll(Duration::from_millis(10));
        assert!(before_due.actions.is_empty());

        let restart_at = supervisor.next_restart_at().expect("restart scheduled");
        let restart = supervisor.poll(restart_at);
        assert_eq!(supervisor.health(), HealthState::Starting);
        assert!(matches!(
            restart.actions.as_slice(),
            [SupervisorAction::Spawn { .. }]
        ));
    }

    #[test]
    fn max_restarts_transition_to_failed() {
        let mut supervisor = test_supervisor().with_restart_policy(RestartPolicy {
            maximum_restarts: 1,
            base_delay: Duration::from_millis(20),
            maximum_delay: Duration::from_millis(60),
        });
        initialize(&mut supervisor);

        let first_exit = supervisor.on_child_exit(Some(1), Duration::from_millis(5));
        assert!(matches!(
            first_exit.events.as_slice(),
            [
                SupervisorEvent::Exited { status: Some(1) },
                SupervisorEvent::RestartScheduled { attempt, .. }
            ] if *attempt == 1
        ));

        let restart_at = supervisor.next_restart_at().expect("restart scheduled");
        supervisor.poll(restart_at);
        supervisor.on_child_started(restart_at + Duration::from_millis(1));
        let second_exit = supervisor.on_child_exit(Some(1), restart_at + Duration::from_millis(2));
        assert_eq!(supervisor.health(), HealthState::Failed);
        assert!(matches!(
            second_exit.events.as_slice(),
            [
                SupervisorEvent::Exited { status: Some(1) },
                SupervisorEvent::RestartLimitReached
            ]
        ));
    }

    #[test]
    fn pause_cancels_backoff_and_resume_restarts_immediately() {
        let mut supervisor = test_supervisor().with_restart_policy(RestartPolicy {
            maximum_restarts: 2,
            base_delay: Duration::from_millis(20),
            maximum_delay: Duration::from_millis(60),
        });
        initialize(&mut supervisor);
        supervisor.on_child_exit(Some(1), Duration::from_millis(5));

        let pause = supervisor.pause();
        assert_eq!(supervisor.health(), HealthState::Disabled);
        assert!(matches!(pause.events.as_slice(), [SupervisorEvent::Paused]));
        assert!(supervisor
            .poll(Duration::from_millis(100))
            .actions
            .is_empty());

        let resume = supervisor.resume(Duration::from_millis(101));
        assert_eq!(supervisor.health(), HealthState::Starting);
        assert!(resume.events.contains(&SupervisorEvent::Resumed));
        assert!(resume
            .actions
            .iter()
            .any(|action| matches!(action, SupervisorAction::Spawn { .. })));
    }

    #[test]
    fn graceful_shutdown_stops_and_prevents_restart() {
        let mut supervisor = test_supervisor();
        initialize(&mut supervisor);

        let shutdown = supervisor.shutdown();
        assert_eq!(supervisor.health(), HealthState::Stopped);
        assert!(matches!(
            shutdown.events.as_slice(),
            [SupervisorEvent::ShutdownRequested]
        ));
        assert!(shutdown.actions.contains(&SupervisorAction::Terminate));

        let exit = supervisor.on_child_exit(Some(0), Duration::from_millis(3));
        assert_eq!(supervisor.health(), HealthState::Stopped);
        assert!(matches!(
            exit.events.as_slice(),
            [
                SupervisorEvent::Exited { status: Some(0) },
                SupervisorEvent::Stopped
            ]
        ));
        assert!(supervisor
            .poll(Duration::from_millis(100))
            .actions
            .is_empty());
    }

    #[test]
    fn command_validation_rejects_control_characters() {
        assert!(matches!(
            AppServerCommand::new("", ["app-server"]),
            Err(CommandSpecError::EmptyExecutable)
        ));
        assert!(matches!(
            AppServerCommand::new("codex\n", ["app-server"]),
            Err(CommandSpecError::InvalidExecutable)
        ));
        assert!(matches!(
            AppServerCommand::new("codex", ["bad\0arg"]),
            Err(CommandSpecError::InvalidArgument { index: 0 })
        ));
    }

    #[test]
    fn restart_jitter_is_deterministic_and_bounded() {
        let policy = RestartPolicy {
            maximum_restarts: 3,
            base_delay: Duration::from_millis(40),
            maximum_delay: Duration::from_millis(100),
        };
        let first = policy.jittered_delay_for(1).expect("delay");
        let second = policy.jittered_delay_for(1).expect("delay");
        assert_eq!(first, second);
        assert!(first >= policy.delay_for(1).expect("base delay"));
        assert!(first <= policy.maximum_delay);
    }
}
