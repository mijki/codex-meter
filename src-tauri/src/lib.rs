pub mod app_server;
pub mod collector;
mod db;
mod domain;
pub mod hooks;
pub mod otel;
pub mod redaction;

use app_server::{HealthState, SupervisorEvent};
use collector::{CollectorHandle, CollectorNotification, CollectorResponse, CollectorRuntimeEvent};
use db::{Database, DbError};
use domain::{AppSettings, ChannelStatus, Dashboard};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

struct AppState {
    database: Arc<Mutex<Database>>,
    collector: CollectorHandle,
    data_dir: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeCollectorDiagnostics {
    health: String,
    restart_count: u32,
    pending_requests: usize,
    child_running: bool,
    executable: Option<String>,
    transport: String,
    database_path: Option<String>,
    log_path: Option<String>,
    schema_version: String,
    reset_interpretation: String,
    latest_error: Option<String>,
}

fn command_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[tauri::command]
fn get_dashboard(state: State<'_, AppState>) -> Result<Dashboard, String> {
    state
        .database
        .lock()
        .map_err(command_error)?
        .dashboard()
        .map_err(command_error)
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state
        .database
        .lock()
        .map_err(command_error)?
        .settings()
        .map_err(command_error)
}

#[tauri::command]
fn save_settings(settings: AppSettings, state: State<'_, AppState>) -> Result<(), String> {
    let collection_enabled = settings.collection_enabled;
    state
        .database
        .lock()
        .map_err(command_error)?
        .save_settings(&settings)
        .map_err(command_error)?;
    if collection_enabled {
        let _ = state.collector.resume();
    } else {
        let _ = state.collector.pause();
    }
    Ok(())
}

#[tauri::command]
fn get_collector_diagnostics(
    state: State<'_, AppState>,
) -> Result<NativeCollectorDiagnostics, String> {
    let status = state.collector.status();
    let dashboard = state
        .database
        .lock()
        .map_err(command_error)?
        .dashboard()
        .map_err(command_error)?;
    let reset_interpretation = if dashboard.quota.is_empty() {
        "No live reset value observed".to_string()
    } else {
        let mut interpretations = dashboard
            .quota
            .iter()
            .map(|quota| quota.reset_interpretation.clone())
            .collect::<Vec<_>>();
        interpretations.sort();
        interpretations.dedup();
        interpretations.join(", ")
    };
    let latest_error = dashboard.channels.iter().find_map(|channel| {
        channel
            .latest_error
            .as_ref()
            .filter(|error| !error.historical)
            .map(|error| error.message.clone())
    });

    Ok(NativeCollectorDiagnostics {
        health: health_name(&status.health).to_string(),
        restart_count: status.restart_count,
        pending_requests: status.pending_requests,
        child_running: status.child_running,
        executable: Some(codex_executable_label()),
        transport: "stdio".to_string(),
        database_path: Some(
            state
                .data_dir
                .join("codex-meter.sqlite")
                .display()
                .to_string(),
        ),
        log_path: None,
        schema_version: "0.144.1 · v1 + v2 + v3".to_string(),
        reset_interpretation,
        latest_error,
    })
}

#[tauri::command]
fn dismiss_alert(
    bucket_id: String,
    reset_window_id: String,
    threshold: i64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .database
        .lock()
        .map_err(command_error)?
        .dismiss_alert(&bucket_id, &reset_window_id, threshold)
        .map_err(command_error)
}

#[tauri::command]
fn delete_local_data(state: State<'_, AppState>) -> Result<(), String> {
    remove_local_tree(&state.data_dir, "exports")?;
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        remove_local_tree(&PathBuf::from(local_app_data).join("CodexMeter"), "spool")?;
    }
    state
        .database
        .lock()
        .map_err(command_error)?
        .delete_analytics()
        .map_err(command_error)
}

fn remove_local_tree(root: &Path, child: &str) -> Result<(), String> {
    let target = root.join(child);
    if !target.exists() {
        return Ok(());
    }
    let canonical_root = fs::canonicalize(root).map_err(command_error)?;
    let canonical_target = fs::canonicalize(&target).map_err(command_error)?;
    if !canonical_target.starts_with(&canonical_root) || canonical_target == canonical_root {
        return Err("Refused to delete a path outside the Codex Meter data directory".to_string());
    }
    fs::remove_dir_all(canonical_target).map_err(command_error)
}

#[tauri::command]
fn get_otel_config_snippet() -> String {
    include_str!("../../integrations/otel/codex-config.example.toml").to_owned()
}

#[tauri::command]
fn export_data(format: String, state: State<'_, AppState>) -> Result<String, String> {
    let (extension, contents) = {
        let database = state.database.lock().map_err(command_error)?;
        match format.as_str() {
            "json" => ("json", database.export_json().map_err(command_error)?),
            "csv" => ("csv", database.export_csv().map_err(command_error)?),
            "diagnostics" => ("json", database.diagnostics_json().map_err(command_error)?),
            _ => return Err("Unsupported export format".to_string()),
        }
    };
    let safe_name = if format == "diagnostics" {
        "diagnostic-export"
    } else {
        "codex-meter-export"
    };
    let path = write_export_file(&state.data_dir, safe_name, extension, &contents)?;
    Ok(path.display().to_string())
}

fn write_export_file(
    data_dir: &Path,
    safe_name: &str,
    extension: &str,
    contents: &str,
) -> Result<PathBuf, String> {
    let export_dir = data_dir.join("exports");
    fs::create_dir_all(&export_dir).map_err(command_error)?;
    let path = export_dir.join(format!(
        "{safe_name}-{}.{}",
        chrono::Utc::now().format("%Y%m%d-%H%M%S"),
        extension
    ));
    fs::write(&path, contents).map_err(command_error)?;
    Ok(path)
}

fn detect_codex_version() -> Option<String> {
    let mut command = codex_cli_command();
    command.arg("--version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_owned())
}

fn codex_cli_command() -> Command {
    #[cfg(windows)]
    if let Some(script) = collector::codex_npm_script() {
        let mut command = Command::new("node");
        command.arg(script);
        return command;
    }
    Command::new("codex")
}

fn codex_executable_label() -> String {
    #[cfg(windows)]
    if collector::codex_npm_script().is_some() {
        return "Codex npm shim (node)".to_string();
    }
    "codex".to_string()
}

fn initialize_database(path: &Path) -> Result<Database, String> {
    let mut database = Database::open(path).map_err(command_error)?;
    if let Some(version) = detect_codex_version() {
        database
            .record_codex_installation(&version)
            .map_err(command_error)?;
    }
    Ok(database)
}

fn start_collector_event_bridge(
    database: Arc<Mutex<Database>>,
    app: AppHandle,
    events: std::sync::mpsc::Receiver<CollectorRuntimeEvent>,
) -> std::io::Result<()> {
    let session_started_at = chrono::Utc::now().to_rfc3339();
    let session_id = format!("collector-{}", chrono::Utc::now().timestamp_millis());
    thread::Builder::new()
        .name("collector-persistence".to_string())
        .spawn(move || {
            for event in events {
                let observed_at = chrono::Utc::now().to_rfc3339();
                if let Ok(mut database) = database.lock() {
                    let persistence_result = match &event {
                        CollectorRuntimeEvent::StatusChanged(status) => {
                            let health = health_name(&status.health);
                            let ended_at =
                                matches!(status.health, HealthState::Stopped | HealthState::Failed)
                                    .then_some(observed_at.as_str());
                            database
                                .record_collector_health(
                                    "app_server",
                                    channel_status(&status.health),
                                    &observed_at,
                                    health,
                                )
                                .and_then(|()| {
                                    database.record_collector_session(
                                        &session_id,
                                        &session_started_at,
                                        ended_at,
                                        health,
                                        i64::from(status.restart_count),
                                    )
                                })
                        }
                        CollectorRuntimeEvent::Response(CollectorResponse::RateLimits {
                            result,
                            ..
                        }) => {
                            persist_rate_limits_and_emit(&mut database, &app, result, &observed_at)
                        }
                        CollectorRuntimeEvent::Response(CollectorResponse::AccountUsage {
                            result,
                            ..
                        }) => database.record_account_usage(result, &observed_at, "app_server"),
                        CollectorRuntimeEvent::Notification(
                            CollectorNotification::RateLimitsUpdated { params },
                        ) => {
                            persist_rate_limits_and_emit(&mut database, &app, params, &observed_at)
                        }
                        CollectorRuntimeEvent::Notification(
                            CollectorNotification::ThreadTokenUsageUpdated { params },
                        ) => database.record_token_usage_notification(
                            params,
                            &observed_at,
                            "app_server",
                        ),
                        CollectorRuntimeEvent::Fault(fault) => database.record_collection_error(
                            "app_server",
                            &observed_at,
                            "runtime",
                            &format!("{fault:?}"),
                            true,
                        ),
                        CollectorRuntimeEvent::Supervisor(SupervisorEvent::MalformedOutput {
                            detail,
                        }) => database.record_collection_error(
                            "app_server",
                            &observed_at,
                            "malformed_output",
                            detail,
                            true,
                        ),
                        CollectorRuntimeEvent::Supervisor(SupervisorEvent::ErrorResponse {
                            code,
                            message,
                            ..
                        }) => database.record_collection_error(
                            "app_server",
                            &observed_at,
                            &format!("json_rpc_{code}"),
                            message,
                            false,
                        ),
                        CollectorRuntimeEvent::Supervisor(SupervisorEvent::Timeout(timeout)) => {
                            database.record_collection_error(
                                "app_server",
                                &observed_at,
                                "timeout",
                                &format!("{timeout:?}"),
                                true,
                            )
                        }
                        CollectorRuntimeEvent::Supervisor(SupervisorEvent::RestartLimitReached) => {
                            database.record_collection_error(
                                "app_server",
                                &observed_at,
                                "restart_limit",
                                "Collector restart limit reached",
                                false,
                            )
                        }
                        _ => Ok(()),
                    };

                    if let Err(error) = persistence_result {
                        let _ = database.record_collection_error(
                            "app_server",
                            &observed_at,
                            "persistence",
                            &error.to_string(),
                            false,
                        );
                    }
                }

                let _ = app.emit(
                    "collector-status",
                    serde_json::json!({ "event": collector_event_name(&event) }),
                );
            }
        })
        .map(|_| ())
}

fn persist_rate_limits_and_emit(
    database: &mut Database,
    app: &AppHandle,
    payload: &serde_json::Value,
    observed_at: &str,
) -> Result<(), DbError> {
    database.record_rate_limits(payload, observed_at, "app_server")?;
    for notification in database.claim_quota_threshold_notifications()? {
        let _ = app.emit("quota-threshold", notification);
    }
    if let Ok(dashboard) = database.dashboard() {
        let highest = dashboard
            .quota
            .iter()
            .filter_map(|window| window.used_percent)
            .fold(0.0_f64, f64::max);
        let tooltip = if highest >= 100.0 {
            "Codex Meter — quota exhausted".to_string()
        } else if highest >= 90.0 {
            format!("Codex Meter — critical quota state ({highest:.0}% used)")
        } else if highest >= 75.0 {
            format!("Codex Meter — quota warning ({highest:.0}% used)")
        } else {
            "Codex Meter — local telemetry".to_string()
        };
        if let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_tooltip(Some(tooltip));
        }
    }
    Ok(())
}

fn health_name(health: &HealthState) -> &'static str {
    match health {
        HealthState::Disabled => "disabled",
        HealthState::Starting => "starting",
        HealthState::Initializing => "initializing",
        HealthState::Healthy => "healthy",
        HealthState::Degraded => "degraded",
        HealthState::BackingOff => "backing_off",
        HealthState::Stopped => "stopped",
        HealthState::Failed => "failed",
    }
}

fn channel_status(health: &HealthState) -> ChannelStatus {
    match health {
        HealthState::Healthy => ChannelStatus::Healthy,
        HealthState::Degraded | HealthState::BackingOff => ChannelStatus::Degraded,
        HealthState::Failed => ChannelStatus::Unavailable,
        HealthState::Disabled
        | HealthState::Starting
        | HealthState::Initializing
        | HealthState::Stopped => ChannelStatus::Inactive,
    }
}

fn collector_event_name(event: &CollectorRuntimeEvent) -> &'static str {
    match event {
        CollectorRuntimeEvent::StatusChanged(_) => "status_changed",
        CollectorRuntimeEvent::Supervisor(_) => "supervisor",
        CollectorRuntimeEvent::Response(_) => "response",
        CollectorRuntimeEvent::Notification(_) => "notification",
        CollectorRuntimeEvent::Fault(_) => "fault",
    }
}

fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Codex Meter", true, None::<&str>)?;
    let health = MenuItem::with_id(
        app,
        "health",
        "Collector: local / idle",
        false,
        None::<&str>,
    )?;
    let pause = MenuItem::with_id(app, "pause", "Pause Collection", true, None::<&str>)?;
    let resume = MenuItem::with_id(app, "resume", "Resume Collection", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &health, &pause, &resume, &settings, &exit])?;
    let icon = app.default_window_icon().cloned();
    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("Codex Meter — local telemetry")
        .show_menu_on_left_click(false);
    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }
    builder
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" | "settings" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "pause" | "resume" => {
                let state = app.state::<AppState>();
                let should_resume = event.id.as_ref() == "resume";
                if let Ok(mut database) = state.database.lock() {
                    if let Ok(mut settings) = database.settings() {
                        settings.collection_enabled = should_resume;
                        let _ = database.save_settings(&settings);
                    }
                };
                if should_resume {
                    let _ = state.collector.resume();
                } else {
                    let _ = state.collector.pause();
                }
            }
            "exit" => {
                let state = app.state::<AppState>();
                let _ = state.collector.shutdown();
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            fs::create_dir_all(&data_dir)?;
            let database = initialize_database(&data_dir.join("codex-meter.sqlite"))
                .map_err(std::io::Error::other)?;
            let database = Arc::new(Mutex::new(database));
            let collection_enabled = database
                .lock()
                .map_err(|error| std::io::Error::other(error.to_string()))?
                .settings()
                .map_err(|error| std::io::Error::other(error.to_string()))?
                .collection_enabled;
            let (collector, events) = CollectorHandle::spawn(
                collector::default_supervisor().map_err(std::io::Error::other)?,
            )
            .map_err(std::io::Error::other)?;
            start_collector_event_bridge(database.clone(), app.handle().clone(), events)?;
            if !collection_enabled {
                collector.pause().map_err(std::io::Error::other)?;
            }
            app.manage(AppState {
                database,
                collector,
                data_dir,
            });
            create_tray(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let minimize = window
                    .state::<AppState>()
                    .database
                    .lock()
                    .ok()
                    .and_then(|database| database.settings().ok())
                    .map(|settings| settings.minimize_to_tray)
                    .unwrap_or(true);
                if minimize {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_dashboard,
            dismiss_alert,
            get_settings,
            get_collector_diagnostics,
            save_settings,
            delete_local_data,
            get_otel_config_snippet,
            export_data
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Codex Meter");
}

#[cfg(test)]
mod tests {
    use super::{remove_local_tree, write_export_file};
    use std::fs;

    fn temporary_data_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "codex-meter-{name}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    #[test]
    fn native_export_writes_utf8_and_local_tree_deletion_is_scoped() {
        let data_dir = temporary_data_dir("native-export");
        fs::create_dir_all(&data_dir).unwrap();

        let path = write_export_file(
            &data_dir,
            "codex-meter-export",
            "json",
            "{\"label\":\"quota \u{2713}\"}",
        )
        .unwrap();
        assert!(path.starts_with(data_dir.join("exports")));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\"label\":\"quota \u{2713}\"}"
        );

        remove_local_tree(&data_dir, "exports").unwrap();
        assert!(!data_dir.join("exports").exists());
        assert!(data_dir.exists());
        fs::remove_dir(&data_dir).unwrap();
    }
}
