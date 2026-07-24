use crate::app_server::{
    AppServerCommand, AppServerSupervisor, CommandSpecError, HealthState, SupervisorAction,
    SupervisorEvent, METHOD_ACCOUNT_USAGE_READ, METHOD_RATE_LIMITS_READ,
    NOTIFICATION_RATE_LIMITS_UPDATED, NOTIFICATION_THREAD_TOKEN_USAGE_UPDATED,
};
use serde_json::Value;
use std::io::{self, BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use thiserror::Error;

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(50);
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectorRuntimeConfig {
    pub poll_interval: Duration,
}

impl Default for CollectorRuntimeConfig {
    fn default() -> Self {
        Self {
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectorStatusSnapshot {
    pub health: HealthState,
    pub restart_count: u32,
    pub pending_requests: usize,
    pub next_restart_at: Option<Duration>,
    pub child_running: bool,
    pub shutdown_requested: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CollectorResponse {
    RateLimits {
        id: u64,
        result: Value,
    },
    AccountUsage {
        id: u64,
        result: Value,
    },
    Other {
        id: u64,
        method: String,
        result: Value,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CollectorNotification {
    RateLimitsUpdated { params: Value },
    ThreadTokenUsageUpdated { params: Value },
    Other { method: String, params: Value },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectorRuntimeFault {
    SpawnFailed { detail: String },
    SendFailed { detail: String },
    TerminateFailed { detail: String },
    WaitFailed { detail: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CollectorRuntimeEvent {
    StatusChanged(CollectorStatusSnapshot),
    Supervisor(SupervisorEvent),
    Response(CollectorResponse),
    Notification(CollectorNotification),
    Fault(CollectorRuntimeFault),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CollectorCommand {
    Pause,
    Resume,
    Shutdown,
}

#[derive(Debug)]
enum WorkerInput {
    Command(CollectorCommand),
    Process(ProcessOutput),
}

#[derive(Debug)]
enum ProcessOutput {
    StdoutLine(String),
    StdoutClosed,
}

#[derive(Debug, Error)]
pub enum CollectorSpawnError {
    #[error("invalid app server command: {0}")]
    InvalidCommand(#[from] CommandSpecError),
    #[error("failed to start collector worker: {0}")]
    WorkerStart(#[source] io::Error),
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CollectorCommandError {
    #[error("collector worker is no longer running")]
    WorkerStopped,
}

#[derive(Debug)]
pub struct CollectorHandle {
    inner: Arc<CollectorHandleInner>,
}

#[derive(Debug)]
struct CollectorHandleInner {
    input_tx: mpsc::Sender<WorkerInput>,
    status: Mutex<CollectorStatusSnapshot>,
    shutdown_sent: AtomicBool,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Clone for CollectorHandle {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Drop for CollectorHandle {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) > 2 {
            return;
        }

        self.inner.request_shutdown();
        let _ = self
            .inner
            .input_tx
            .send(WorkerInput::Command(CollectorCommand::Shutdown));
        if let Ok(mut worker) = self.inner.worker.lock() {
            if let Some(join_handle) = worker.take() {
                let _ = join_handle.join();
            }
        }
    }
}

impl CollectorHandle {
    pub fn spawn(
        supervisor: AppServerSupervisor,
    ) -> Result<(Self, mpsc::Receiver<CollectorRuntimeEvent>), CollectorSpawnError> {
        Self::spawn_with_factory(
            supervisor,
            CollectorRuntimeConfig::default(),
            Arc::new(RealProcessFactory),
        )
    }

    pub fn spawn_with_config(
        supervisor: AppServerSupervisor,
        config: CollectorRuntimeConfig,
    ) -> Result<(Self, mpsc::Receiver<CollectorRuntimeEvent>), CollectorSpawnError> {
        Self::spawn_with_factory(supervisor, config, Arc::new(RealProcessFactory))
    }

    pub fn status(&self) -> CollectorStatusSnapshot {
        self.inner
            .status
            .lock()
            .expect("collector status mutex poisoned")
            .clone()
    }

    pub fn pause(&self) -> Result<(), CollectorCommandError> {
        self.send_command(CollectorCommand::Pause)
    }

    pub fn resume(&self) -> Result<(), CollectorCommandError> {
        self.send_command(CollectorCommand::Resume)
    }

    pub fn shutdown(&self) -> Result<(), CollectorCommandError> {
        self.inner.request_shutdown();
        self.send_command(CollectorCommand::Shutdown)
    }

    fn send_command(&self, command: CollectorCommand) -> Result<(), CollectorCommandError> {
        self.inner
            .input_tx
            .send(WorkerInput::Command(command))
            .map_err(|_| CollectorCommandError::WorkerStopped)
    }

    fn spawn_with_factory(
        supervisor: AppServerSupervisor,
        config: CollectorRuntimeConfig,
        process_factory: Arc<dyn ProcessFactory>,
    ) -> Result<(Self, mpsc::Receiver<CollectorRuntimeEvent>), CollectorSpawnError> {
        let initial_status = snapshot(&supervisor, false, false);
        let (input_tx, input_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let inner = Arc::new(CollectorHandleInner {
            input_tx,
            status: Mutex::new(initial_status),
            shutdown_sent: AtomicBool::new(false),
            worker: Mutex::new(None),
        });

        let worker_inner = Arc::clone(&inner);
        let worker_input_tx = worker_inner.input_tx.clone();
        let worker = thread::Builder::new()
            .name("collector-runtime".to_string())
            .spawn(move || {
                let mut runtime = CollectorRuntime::new(
                    supervisor,
                    config,
                    process_factory,
                    worker_input_tx,
                    input_rx,
                    event_tx,
                    worker_inner,
                );
                runtime.run();
            })
            .map_err(CollectorSpawnError::WorkerStart)?;

        if let Ok(mut slot) = inner.worker.lock() {
            *slot = Some(worker);
        }

        Ok((Self { inner }, event_rx))
    }
}

impl CollectorHandleInner {
    fn request_shutdown(&self) {
        self.shutdown_sent.store(true, Ordering::SeqCst);
    }

    fn shutdown_requested(&self) -> bool {
        self.shutdown_sent.load(Ordering::SeqCst)
    }
}

pub fn default_supervisor() -> Result<AppServerSupervisor, CommandSpecError> {
    #[cfg(windows)]
    if let Some(script) = codex_npm_script() {
        let command = AppServerCommand::new(
            "node",
            [
                script.to_string_lossy().into_owned(),
                "app-server".to_string(),
                "--stdio".to_string(),
            ],
        )?;
        return Ok(AppServerSupervisor::new(command));
    }

    let command = AppServerCommand::new("codex", ["app-server", "--stdio"])?;
    Ok(AppServerSupervisor::new(command))
}

pub(crate) fn codex_npm_script() -> Option<std::path::PathBuf> {
    let script = std::path::PathBuf::from(std::env::var_os("APPDATA")?)
        .join("npm")
        .join("node_modules")
        .join("@openai")
        .join("codex")
        .join("bin")
        .join("codex.js");
    script.is_file().then_some(script)
}

trait ProcessFactory: Send + Sync + 'static {
    fn spawn(
        &self,
        command: &AppServerCommand,
        worker_tx: mpsc::Sender<WorkerInput>,
    ) -> io::Result<Box<dyn RunningProcess>>;
}

trait RunningProcess: Send {
    fn write_line(&mut self, line: &str) -> io::Result<()>;
    fn try_wait_code(&mut self) -> io::Result<Option<i32>>;
    fn terminate(&mut self) -> io::Result<()>;
    fn wait_code(&mut self) -> io::Result<Option<i32>>;
}

#[derive(Debug, Default)]
struct RealProcessFactory;

impl ProcessFactory for RealProcessFactory {
    fn spawn(
        &self,
        command: &AppServerCommand,
        worker_tx: mpsc::Sender<WorkerInput>,
    ) -> io::Result<Box<dyn RunningProcess>> {
        let mut process = Command::new(command.executable());
        process
            .args(command.args())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            process.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = process.spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "child stdin pipe was not available",
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "child stdout pipe was not available",
            )
        })?;
        let stdout_thread = spawn_stdout_reader(stdout, worker_tx);

        Ok(Box::new(RealRunningProcess {
            child,
            stdin: Some(stdin),
            stdout_thread: Some(stdout_thread),
        }))
    }
}

fn spawn_stdout_reader(
    stdout: std::process::ChildStdout,
    worker_tx: mpsc::Sender<WorkerInput>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("collector-stdout".to_string())
        .spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();

            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let message = line.clone();
                        if worker_tx
                            .send(WorkerInput::Process(ProcessOutput::StdoutLine(message)))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }

            let _ = worker_tx.send(WorkerInput::Process(ProcessOutput::StdoutClosed));
        })
        .expect("collector stdout reader thread should start")
}

#[derive(Debug)]
struct RealRunningProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout_thread: Option<JoinHandle<()>>,
}

impl RealRunningProcess {
    fn join_stdout_thread(&mut self) {
        if let Some(stdout_thread) = self.stdout_thread.take() {
            let _ = stdout_thread.join();
        }
    }
}

impl RunningProcess for RealRunningProcess {
    fn write_line(&mut self, line: &str) -> io::Result<()> {
        let stdin = self.stdin.as_mut().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "child stdin pipe is closed")
        })?;
        stdin.write_all(line.as_bytes())?;
        stdin.flush()
    }

    fn try_wait_code(&mut self) -> io::Result<Option<i32>> {
        self.child
            .try_wait()
            .map(|status| status.and_then(|value| value.code()))
    }

    fn terminate(&mut self) -> io::Result<()> {
        match self.child.kill() {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::InvalidInput => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn wait_code(&mut self) -> io::Result<Option<i32>> {
        let status = self.child.wait()?;
        self.join_stdout_thread();
        Ok(status.code())
    }
}

struct CollectorRuntime {
    supervisor: AppServerSupervisor,
    config: CollectorRuntimeConfig,
    process_factory: Arc<dyn ProcessFactory>,
    worker_tx: mpsc::Sender<WorkerInput>,
    input_rx: mpsc::Receiver<WorkerInput>,
    event_tx: mpsc::Sender<CollectorRuntimeEvent>,
    handle_inner: Arc<CollectorHandleInner>,
    started_at: Instant,
    child: Option<Box<dyn RunningProcess>>,
}

impl CollectorRuntime {
    fn new(
        supervisor: AppServerSupervisor,
        config: CollectorRuntimeConfig,
        process_factory: Arc<dyn ProcessFactory>,
        worker_tx: mpsc::Sender<WorkerInput>,
        input_rx: mpsc::Receiver<WorkerInput>,
        event_tx: mpsc::Sender<CollectorRuntimeEvent>,
        handle_inner: Arc<CollectorHandleInner>,
    ) -> Self {
        Self {
            supervisor,
            config,
            process_factory,
            worker_tx,
            input_rx,
            event_tx,
            handle_inner,
            started_at: Instant::now(),
            child: None,
        }
    }

    fn run(&mut self) {
        let initial = self.supervisor.start(Duration::ZERO);
        self.apply_update(initial, Duration::ZERO);
        self.emit_status_if_changed(false);

        loop {
            let mut should_stop = false;

            match self.input_rx.recv_timeout(self.config.poll_interval) {
                Ok(WorkerInput::Command(command)) => {
                    let now = self.now();
                    self.handle_command(command, now);
                }
                Ok(WorkerInput::Process(output)) => {
                    let now = self.now();
                    self.handle_process_output(output, now);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let now = self.now();
                    self.handle_command(CollectorCommand::Shutdown, now);
                    should_stop = true;
                }
            }

            let now = self.now();
            self.observe_child_exit(now);
            let update = self.supervisor.poll(now);
            self.apply_update(update, now);

            if self.handle_inner.shutdown_requested()
                && self.child.is_none()
                && matches!(
                    self.supervisor.health(),
                    HealthState::Stopped | HealthState::Failed
                )
            {
                should_stop = true;
            }

            if should_stop
                && self.child.is_none()
                && matches!(
                    self.supervisor.health(),
                    HealthState::Stopped | HealthState::Failed
                )
            {
                break;
            }
        }

        self.force_child_shutdown();
        self.emit_status_if_changed(true);
    }

    fn now(&self) -> Duration {
        self.started_at.elapsed()
    }

    fn handle_command(&mut self, command: CollectorCommand, now: Duration) {
        let update = match command {
            CollectorCommand::Pause => self.supervisor.pause(),
            CollectorCommand::Resume => self.supervisor.resume(now),
            CollectorCommand::Shutdown => {
                self.handle_inner.request_shutdown();
                self.supervisor.shutdown()
            }
        };
        self.apply_update(update, now);
    }

    fn handle_process_output(&mut self, output: ProcessOutput, now: Duration) {
        match output {
            ProcessOutput::StdoutLine(line) => {
                let trimmed = line.trim_end_matches(['\r', '\n']);
                if trimmed.is_empty() {
                    return;
                }
                let update = self.supervisor.receive_line(trimmed, now);
                self.apply_update(update, now);
            }
            ProcessOutput::StdoutClosed => {
                self.observe_child_exit(now);
            }
        }
    }

    fn observe_child_exit(&mut self, now: Duration) {
        let exited = match self.child.as_mut() {
            Some(child) => match child.try_wait_code() {
                Ok(status) => status,
                Err(error) => {
                    self.emit_fault(CollectorRuntimeFault::WaitFailed {
                        detail: error.to_string(),
                    });
                    let mut child = self
                        .child
                        .take()
                        .expect("child exists after try_wait failure");
                    if let Err(terminate_error) = child.terminate() {
                        self.emit_fault(CollectorRuntimeFault::TerminateFailed {
                            detail: terminate_error.to_string(),
                        });
                    }
                    let exit_status = match child.wait_code() {
                        Ok(status) => status,
                        Err(wait_error) => {
                            self.emit_fault(CollectorRuntimeFault::WaitFailed {
                                detail: wait_error.to_string(),
                            });
                            None
                        }
                    };
                    let update = self.supervisor.on_child_exit(exit_status, now);
                    self.apply_update(update, now);
                    return;
                }
            },
            None => None,
        };

        if let Some(status) = exited {
            let mut child = self
                .child
                .take()
                .expect("child exists while observing exit");
            let waited_status = match child.wait_code() {
                Ok(waited_status) => waited_status.or(Some(status)),
                Err(error) => {
                    self.emit_fault(CollectorRuntimeFault::WaitFailed {
                        detail: error.to_string(),
                    });
                    Some(status)
                }
            };
            let update = self.supervisor.on_child_exit(waited_status, now);
            self.apply_update(update, now);
        }
    }

    fn force_child_shutdown(&mut self) {
        if let Some(mut child) = self.child.take() {
            if let Err(error) = child.terminate() {
                self.emit_fault(CollectorRuntimeFault::TerminateFailed {
                    detail: error.to_string(),
                });
            }
            if let Err(error) = child.wait_code() {
                self.emit_fault(CollectorRuntimeFault::WaitFailed {
                    detail: error.to_string(),
                });
            }
        }
    }

    fn apply_update(&mut self, update: crate::app_server::SupervisorUpdate, now: Duration) {
        let mut send_initial_requests = false;
        for event in update.events {
            match event {
                SupervisorEvent::Response { id, method, result } => {
                    let response = match method.as_str() {
                        METHOD_RATE_LIMITS_READ => CollectorResponse::RateLimits { id, result },
                        METHOD_ACCOUNT_USAGE_READ => CollectorResponse::AccountUsage { id, result },
                        _ => CollectorResponse::Other { id, method, result },
                    };
                    let _ = self
                        .event_tx
                        .send(CollectorRuntimeEvent::Response(response));
                }
                SupervisorEvent::Notification { method, params } => {
                    let notification = match method.as_str() {
                        NOTIFICATION_RATE_LIMITS_UPDATED => {
                            CollectorNotification::RateLimitsUpdated { params }
                        }
                        NOTIFICATION_THREAD_TOKEN_USAGE_UPDATED => {
                            CollectorNotification::ThreadTokenUsageUpdated { params }
                        }
                        _ => CollectorNotification::Other { method, params },
                    };
                    let _ = self
                        .event_tx
                        .send(CollectorRuntimeEvent::Notification(notification));
                }
                SupervisorEvent::Initialized { result } => {
                    let _ = self.event_tx.send(CollectorRuntimeEvent::Supervisor(
                        SupervisorEvent::Initialized {
                            result: result.clone(),
                        },
                    ));
                    send_initial_requests = true;
                }
                other => {
                    let _ = self.event_tx.send(CollectorRuntimeEvent::Supervisor(other));
                }
            }
        }

        for action in update.actions {
            self.apply_action(action, now);
        }

        if send_initial_requests {
            self.send_initial_account_requests(now);
        }

        self.emit_status_if_changed(false);
    }

    fn send_initial_account_requests(&mut self, now: Duration) {
        for method in [METHOD_RATE_LIMITS_READ, METHOD_ACCOUNT_USAGE_READ] {
            match self.supervisor.send_request(method, Value::Null, now) {
                Ok(update) => self.apply_update(update, now),
                Err(_) => {
                    self.emit_fault(CollectorRuntimeFault::SendFailed {
                        detail: format!("supervisor rejected initial request for {method}"),
                    });
                }
            }
        }
    }

    fn apply_action(&mut self, action: SupervisorAction, now: Duration) {
        match action {
            SupervisorAction::Spawn { command } => {
                match self.process_factory.spawn(&command, self.worker_tx.clone()) {
                    Ok(child) => {
                        self.child = Some(child);
                        let update = self.supervisor.on_child_started(now);
                        self.apply_update(update, now);
                    }
                    Err(error) => {
                        self.emit_fault(CollectorRuntimeFault::SpawnFailed {
                            detail: error.to_string(),
                        });
                        let update = self.supervisor.on_child_exit(None, now);
                        self.apply_update(update, now);
                    }
                }
            }
            SupervisorAction::SendLine(line) => {
                let Some(child) = self.child.as_mut() else {
                    self.emit_fault(CollectorRuntimeFault::SendFailed {
                        detail: "child process is not running".to_string(),
                    });
                    return;
                };

                if let Err(error) = child.write_line(&line) {
                    self.emit_fault(CollectorRuntimeFault::SendFailed {
                        detail: error.to_string(),
                    });

                    let mut child = self.child.take().expect("child exists for failed send");
                    if let Err(terminate_error) = child.terminate() {
                        self.emit_fault(CollectorRuntimeFault::TerminateFailed {
                            detail: terminate_error.to_string(),
                        });
                    }
                    let exit_status = match child.wait_code() {
                        Ok(status) => status,
                        Err(wait_error) => {
                            self.emit_fault(CollectorRuntimeFault::WaitFailed {
                                detail: wait_error.to_string(),
                            });
                            None
                        }
                    };
                    let update = self.supervisor.on_child_exit(exit_status, now);
                    self.apply_update(update, now);
                }
            }
            SupervisorAction::Terminate => {
                let Some(mut child) = self.child.take() else {
                    return;
                };

                if let Err(error) = child.terminate() {
                    self.emit_fault(CollectorRuntimeFault::TerminateFailed {
                        detail: error.to_string(),
                    });
                }

                let exit_status = match child.wait_code() {
                    Ok(status) => status,
                    Err(error) => {
                        self.emit_fault(CollectorRuntimeFault::WaitFailed {
                            detail: error.to_string(),
                        });
                        None
                    }
                };
                let update = self.supervisor.on_child_exit(exit_status, now);
                self.apply_update(update, now);
            }
        }
    }

    fn emit_fault(&self, fault: CollectorRuntimeFault) {
        let _ = self.event_tx.send(CollectorRuntimeEvent::Fault(fault));
    }

    fn emit_status_if_changed(&self, force: bool) {
        let snapshot = snapshot(
            &self.supervisor,
            self.child.is_some(),
            self.handle_inner.shutdown_requested(),
        );
        let mut stored = self
            .handle_inner
            .status
            .lock()
            .expect("collector status mutex poisoned");
        if force || *stored != snapshot {
            *stored = snapshot.clone();
            let _ = self
                .event_tx
                .send(CollectorRuntimeEvent::StatusChanged(snapshot));
        }
    }
}

fn snapshot(
    supervisor: &AppServerSupervisor,
    child_running: bool,
    shutdown_requested: bool,
) -> CollectorStatusSnapshot {
    CollectorStatusSnapshot {
        health: supervisor.health(),
        restart_count: supervisor.restart_count(),
        pending_requests: supervisor.pending_request_count(),
        next_restart_at: supervisor.next_restart_at(),
        child_running,
        shutdown_requested,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_server::{
        parse_message, Message, RestartPolicy, TimeoutPolicy, NOTIFICATION_RATE_LIMITS_UPDATED,
    };
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Default)]
    struct FakeProcessFactory {
        state: Arc<FakeProcessState>,
    }

    #[derive(Debug, Default)]
    struct FakeProcessState {
        spawn_count: AtomicUsize,
        terminate_count: AtomicUsize,
        writes: Mutex<Vec<String>>,
        exit_code: Mutex<Option<i32>>,
        worker_tx: Mutex<Option<mpsc::Sender<WorkerInput>>>,
        commands: Mutex<Vec<(String, Vec<String>)>>,
        spawn_failures: Mutex<VecDeque<String>>,
    }

    #[derive(Debug)]
    struct FakeProcess {
        state: Arc<FakeProcessState>,
    }

    impl FakeProcessFactory {
        fn new() -> Self {
            Self::default()
        }

        fn push_line(&self, line: impl Into<String>) {
            let tx = self
                .state
                .worker_tx
                .lock()
                .expect("worker tx mutex poisoned")
                .clone()
                .expect("worker tx available");
            tx.send(WorkerInput::Process(ProcessOutput::StdoutLine(line.into())))
                .expect("line should reach worker");
        }

        fn close_stdout(&self) {
            if let Some(tx) = self
                .state
                .worker_tx
                .lock()
                .expect("worker tx mutex poisoned")
                .clone()
            {
                tx.send(WorkerInput::Process(ProcessOutput::StdoutClosed))
                    .expect("stdout closed should reach worker");
            }
        }

        fn set_exit_code(&self, code: i32) {
            *self
                .state
                .exit_code
                .lock()
                .expect("exit code mutex poisoned") = Some(code);
            self.close_stdout();
        }

        fn writes(&self) -> Vec<String> {
            self.state
                .writes
                .lock()
                .expect("writes mutex poisoned")
                .clone()
        }

        fn spawn_count(&self) -> usize {
            self.state.spawn_count.load(Ordering::SeqCst)
        }

        fn terminate_count(&self) -> usize {
            self.state.terminate_count.load(Ordering::SeqCst)
        }

        fn last_command(&self) -> Option<(String, Vec<String>)> {
            self.state
                .commands
                .lock()
                .expect("commands mutex poisoned")
                .last()
                .cloned()
        }

        fn fail_next_spawn(&self, detail: impl Into<String>) {
            self.state
                .spawn_failures
                .lock()
                .expect("spawn failures mutex poisoned")
                .push_back(detail.into());
        }
    }

    impl ProcessFactory for FakeProcessFactory {
        fn spawn(
            &self,
            command: &AppServerCommand,
            worker_tx: mpsc::Sender<WorkerInput>,
        ) -> io::Result<Box<dyn RunningProcess>> {
            if let Some(detail) = self
                .state
                .spawn_failures
                .lock()
                .expect("spawn failures mutex poisoned")
                .pop_front()
            {
                return Err(io::Error::other(detail));
            }

            self.state.spawn_count.fetch_add(1, Ordering::SeqCst);
            *self
                .state
                .worker_tx
                .lock()
                .expect("worker tx mutex poisoned") = Some(worker_tx);
            *self
                .state
                .exit_code
                .lock()
                .expect("exit code mutex poisoned") = None;
            self.state
                .commands
                .lock()
                .expect("commands mutex poisoned")
                .push((command.executable().to_string(), command.args().to_vec()));

            Ok(Box::new(FakeProcess {
                state: Arc::clone(&self.state),
            }))
        }
    }

    impl RunningProcess for FakeProcess {
        fn write_line(&mut self, line: &str) -> io::Result<()> {
            self.state
                .writes
                .lock()
                .expect("writes mutex poisoned")
                .push(line.to_string());
            Ok(())
        }

        fn try_wait_code(&mut self) -> io::Result<Option<i32>> {
            Ok(*self
                .state
                .exit_code
                .lock()
                .expect("exit code mutex poisoned"))
        }

        fn terminate(&mut self) -> io::Result<()> {
            self.state.terminate_count.fetch_add(1, Ordering::SeqCst);
            let mut exit_code = self
                .state
                .exit_code
                .lock()
                .expect("exit code mutex poisoned");
            if exit_code.is_none() {
                *exit_code = Some(0);
            }
            Ok(())
        }

        fn wait_code(&mut self) -> io::Result<Option<i32>> {
            Ok(*self
                .state
                .exit_code
                .lock()
                .expect("exit code mutex poisoned"))
        }
    }

    fn test_supervisor() -> AppServerSupervisor {
        AppServerSupervisor::new(
            AppServerCommand::new("codex", ["app-server", "--stdio"]).expect("valid test command"),
        )
        .with_timeouts(TimeoutPolicy {
            startup_timeout: Duration::from_millis(20),
            initialize_timeout: Duration::from_millis(20),
            request_timeout: Duration::from_millis(20),
        })
        .with_restart_policy(RestartPolicy {
            maximum_restarts: 1,
            base_delay: Duration::from_millis(2),
            maximum_delay: Duration::from_millis(2),
        })
    }

    fn spawn_runtime(
        supervisor: AppServerSupervisor,
        factory: Arc<FakeProcessFactory>,
    ) -> (CollectorHandle, mpsc::Receiver<CollectorRuntimeEvent>) {
        CollectorHandle::spawn_with_factory(
            supervisor,
            CollectorRuntimeConfig {
                poll_interval: Duration::from_millis(2),
            },
            factory,
        )
        .expect("runtime should spawn")
    }

    fn recv_event(
        rx: &mpsc::Receiver<CollectorRuntimeEvent>,
        timeout: Duration,
    ) -> CollectorRuntimeEvent {
        rx.recv_timeout(timeout)
            .expect("expected collector event before timeout")
    }

    fn recv_until<F>(
        rx: &mpsc::Receiver<CollectorRuntimeEvent>,
        timeout: Duration,
        predicate: F,
    ) -> CollectorRuntimeEvent
    where
        F: Fn(&CollectorRuntimeEvent) -> bool,
    {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let event = recv_event(rx, remaining.max(Duration::from_millis(1)));
            if predicate(&event) {
                return event;
            }
        }
    }

    fn wait_for<F>(timeout: Duration, mut predicate: F)
    where
        F: FnMut() -> bool,
    {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if predicate() {
                return;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(predicate(), "condition was not met before timeout");
    }

    fn initialize_runtime(
        factory: &FakeProcessFactory,
        rx: &mpsc::Receiver<CollectorRuntimeEvent>,
    ) -> u64 {
        recv_until(rx, Duration::from_millis(200), |event| {
            matches!(
                event,
                CollectorRuntimeEvent::Supervisor(SupervisorEvent::SpawnRequested)
            )
        });

        wait_for(Duration::from_millis(200), || !factory.writes().is_empty());
        let initialize_write = factory.writes()[0].clone();
        let initialize_id = match parse_message(initialize_write.trim_end(), 16_384)
            .expect("initialize request should parse")
        {
            Message::Request { id, method, .. } => {
                assert_eq!(method, "initialize");
                id.as_u64().expect("initialize id should be numeric")
            }
            other => panic!("unexpected initialize write: {other:?}"),
        };

        factory.push_line(
            json!({
                "jsonrpc": "2.0",
                "id": initialize_id,
                "result": {
                    "userAgent": "codex-test",
                    "platformFamily": "windows",
                    "platformOs": "windows"
                }
            })
            .to_string(),
        );

        recv_until(rx, Duration::from_millis(200), |event| {
            matches!(
                event,
                CollectorRuntimeEvent::Supervisor(SupervisorEvent::Initialized { .. })
            )
        });
        initialize_id
    }

    fn request_ids(factory: &FakeProcessFactory) -> Vec<(u64, String)> {
        factory
            .writes()
            .into_iter()
            .filter_map(|line| match parse_message(line.trim_end(), 16_384).ok()? {
                Message::Request { id, method, .. } => {
                    Some((id.as_u64().expect("request id should be numeric"), method))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn lifecycle_initializes_and_only_issues_account_reads() {
        let factory = Arc::new(FakeProcessFactory::new());
        let (handle, rx) = spawn_runtime(test_supervisor(), Arc::clone(&factory));

        wait_for(Duration::from_millis(200), || factory.spawn_count() == 1);
        assert_eq!(
            factory.last_command(),
            Some((
                "codex".to_string(),
                vec!["app-server".to_string(), "--stdio".to_string()]
            ))
        );

        initialize_runtime(&factory, &rx);
        wait_for(Duration::from_millis(200), || factory.writes().len() >= 4);

        let methods: Vec<String> = request_ids(&factory)
            .into_iter()
            .map(|(_, method)| method)
            .collect();
        assert_eq!(
            methods,
            vec![
                "initialize".to_string(),
                METHOD_RATE_LIMITS_READ.to_string(),
                METHOD_ACCOUNT_USAGE_READ.to_string(),
            ]
        );
        assert!(factory
            .writes()
            .iter()
            .any(|line| line.contains("\"method\":\"initialized\"")));

        let status = handle.status();
        assert_eq!(status.health, HealthState::Healthy);
        assert!(status.child_running);
        assert_eq!(status.pending_requests, 2);
    }

    #[test]
    fn pause_and_resume_cycle_the_runtime() {
        let factory = Arc::new(FakeProcessFactory::new());
        let (handle, rx) = spawn_runtime(test_supervisor(), Arc::clone(&factory));

        initialize_runtime(&factory, &rx);
        handle.pause().expect("pause should reach worker");
        recv_until(&rx, Duration::from_millis(200), |event| {
            matches!(
                event,
                CollectorRuntimeEvent::Supervisor(SupervisorEvent::Paused)
            )
        });
        recv_until(&rx, Duration::from_millis(200), |event| {
            matches!(
                event,
                CollectorRuntimeEvent::Supervisor(SupervisorEvent::Exited { status: Some(0) })
            )
        });
        wait_for(Duration::from_millis(200), || {
            factory.terminate_count() == 1
        });
        assert_eq!(handle.status().health, HealthState::Disabled);

        handle.resume().expect("resume should reach worker");
        recv_until(&rx, Duration::from_millis(200), |event| {
            matches!(
                event,
                CollectorRuntimeEvent::Supervisor(SupervisorEvent::Resumed)
            )
        });
        wait_for(Duration::from_millis(200), || factory.spawn_count() == 2);
        wait_for(Duration::from_millis(200), || {
            handle.status().health == HealthState::Initializing
        });
    }

    #[test]
    fn forwards_typed_responses_and_notifications() {
        let factory = Arc::new(FakeProcessFactory::new());
        let (_handle, rx) = spawn_runtime(test_supervisor(), Arc::clone(&factory));

        initialize_runtime(&factory, &rx);
        wait_for(Duration::from_millis(200), || factory.writes().len() >= 4);

        let ids = request_ids(&factory);
        let rate_limits_id = ids
            .iter()
            .find(|(_, method)| method == METHOD_RATE_LIMITS_READ)
            .map(|(id, _)| *id)
            .expect("rate limits request id");
        let account_usage_id = ids
            .iter()
            .find(|(_, method)| method == METHOD_ACCOUNT_USAGE_READ)
            .map(|(id, _)| *id)
            .expect("account usage request id");

        factory.push_line(
            json!({
                "jsonrpc": "2.0",
                "id": rate_limits_id,
                "result": { "window": "primary" }
            })
            .to_string(),
        );
        factory.push_line(
            json!({
                "jsonrpc": "2.0",
                "method": NOTIFICATION_RATE_LIMITS_UPDATED,
                "params": { "limitId": "primary" }
            })
            .to_string(),
        );
        factory.push_line(
            json!({
                "jsonrpc": "2.0",
                "id": account_usage_id,
                "result": { "usage": "ok" }
            })
            .to_string(),
        );

        assert!(matches!(
            recv_until(&rx, Duration::from_millis(200), |event| matches!(
                event,
                CollectorRuntimeEvent::Response(CollectorResponse::RateLimits { id, .. })
                    if *id == rate_limits_id
            )),
            CollectorRuntimeEvent::Response(CollectorResponse::RateLimits { .. })
        ));
        assert!(matches!(
            recv_until(&rx, Duration::from_millis(200), |event| matches!(
                event,
                CollectorRuntimeEvent::Notification(
                    CollectorNotification::RateLimitsUpdated { .. }
                )
            )),
            CollectorRuntimeEvent::Notification(CollectorNotification::RateLimitsUpdated { .. })
        ));
        assert!(matches!(
            recv_until(&rx, Duration::from_millis(200), |event| matches!(
                event,
                CollectorRuntimeEvent::Response(CollectorResponse::AccountUsage { id, .. })
                    if *id == account_usage_id
            )),
            CollectorRuntimeEvent::Response(CollectorResponse::AccountUsage { .. })
        ));
    }

    #[test]
    fn unexpected_exit_restarts_once_then_fails() {
        let factory = Arc::new(FakeProcessFactory::new());
        let (handle, rx) = spawn_runtime(test_supervisor(), Arc::clone(&factory));

        wait_for(Duration::from_millis(200), || factory.spawn_count() == 1);
        factory.set_exit_code(1);

        recv_until(&rx, Duration::from_millis(200), |event| {
            matches!(
                event,
                CollectorRuntimeEvent::Supervisor(SupervisorEvent::RestartScheduled {
                    attempt: 1,
                    ..
                })
            )
        });
        wait_for(Duration::from_millis(200), || factory.spawn_count() == 2);

        factory.set_exit_code(1);
        recv_until(&rx, Duration::from_millis(200), |event| {
            matches!(
                event,
                CollectorRuntimeEvent::Supervisor(SupervisorEvent::RestartLimitReached)
            )
        });

        wait_for(Duration::from_millis(200), || {
            handle.status().health == HealthState::Failed
        });
        assert_eq!(handle.status().restart_count, 1);
    }

    #[test]
    fn shutdown_terminates_child_and_marks_stopped() {
        let factory = Arc::new(FakeProcessFactory::new());
        let (handle, rx) = spawn_runtime(test_supervisor(), Arc::clone(&factory));

        initialize_runtime(&factory, &rx);
        handle.shutdown().expect("shutdown should reach worker");

        recv_until(&rx, Duration::from_millis(200), |event| {
            matches!(
                event,
                CollectorRuntimeEvent::Supervisor(SupervisorEvent::ShutdownRequested)
            )
        });
        recv_until(&rx, Duration::from_millis(200), |event| {
            matches!(
                event,
                CollectorRuntimeEvent::Supervisor(SupervisorEvent::Stopped)
            )
        });

        wait_for(Duration::from_millis(200), || {
            factory.terminate_count() == 1
        });
        assert_eq!(handle.status().health, HealthState::Stopped);
        assert!(handle.status().shutdown_requested);
    }

    #[test]
    fn drop_of_last_handle_requests_shutdown() {
        let factory = Arc::new(FakeProcessFactory::new());
        let (handle, rx) = spawn_runtime(test_supervisor(), Arc::clone(&factory));
        let clone = handle.clone();

        initialize_runtime(&factory, &rx);
        drop(clone);
        assert_eq!(factory.terminate_count(), 0);

        drop(handle);
        wait_for(Duration::from_millis(200), || {
            factory.terminate_count() == 1
        });
    }

    #[test]
    fn spawn_failures_are_forwarded_as_faults() {
        let factory = Arc::new(FakeProcessFactory::new());
        factory.fail_next_spawn("spawn failed");
        let (_handle, rx) = spawn_runtime(test_supervisor(), Arc::clone(&factory));

        assert!(matches!(
            recv_until(&rx, Duration::from_millis(200), |event| matches!(
                event,
                CollectorRuntimeEvent::Fault(CollectorRuntimeFault::SpawnFailed { .. })
            )),
            CollectorRuntimeEvent::Fault(CollectorRuntimeFault::SpawnFailed { .. })
        ));
    }
}
