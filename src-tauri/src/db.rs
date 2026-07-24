use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use rusqlite::types::Type;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;
use serde_json::{json, Value};
use thiserror::Error;

use crate::domain::{
    AccountUsageDailyBucket, AccountUsageSummary, Accuracy, AlertCenter, AlertSeverity,
    AppSettings, BurnAnalysis, BurnConfidence, CapabilityStatus, ChannelHealth, ChannelStatus,
    CollectorDiagnostics, CollectorErrorDiagnostic, CollectorSessionDiagnostic, Dashboard,
    DiagnosticsReport, DistributionPoint, ExpensiveTurn, ForecastConfidence, ForecastModel,
    ForecastRisk, MissingSignal, QuotaAlert, QuotaForecast, QuotaObservation, QuotaThresholdEvent,
    QuotaWindow, SourceError, SourceStatus, TelemetryState, TelemetryStatus, TokenTotals,
};
use crate::forecast::{calculate_forecast, ForecastInput};
use crate::redaction::redact_diagnostic;

const MIGRATION_VERSION: i64 = 4;
const INITIAL_MIGRATION_SQL: &str = include_str!("../migrations/0001_initial.sql");
const LIVE_ACCOUNT_MIGRATION_SQL: &str =
    include_str!("../migrations/0002_live_account_telemetry.sql");
const ALERTS_AND_SOURCE_HEALTH_MIGRATION_SQL: &str =
    include_str!("../migrations/0003_alerts_and_source_health.sql");
const QUOTA_FORECASTS_MIGRATION_SQL: &str = include_str!("../migrations/0004_quota_forecasts.sql");
const MIGRATIONS: &[(i64, &str, &str)] = &[
    (1, "0001_initial", INITIAL_MIGRATION_SQL),
    (2, "0002_live_account_telemetry", LIVE_ACCOUNT_MIGRATION_SQL),
    (
        3,
        "0003_alerts_and_source_health",
        ALERTS_AND_SOURCE_HEALTH_MIGRATION_SQL,
    ),
    (4, "0004_quota_forecasts", QUOTA_FORECASTS_MIGRATION_SQL),
];
const DEFAULT_CODEX_VERSION: &str = "0.144.1";
const FIXTURE_OCCURRED_AT: &str = "2026-07-22T12:42:00Z";
const FIXTURE_RESET_PRIMARY: &str = "2026-07-22T15:00:00Z";
const FIXTURE_RESET_SECONDARY: &str = "2026-07-27T00:00:00Z";
const FIXTURE_TURN_STARTED: &str = "2026-07-22T09:41:00Z";
const FIXTURE_TURN_COMPLETED: &str = "2026-07-22T12:42:00Z";
const FIXTURE_COMPACT_STARTED: &str = "2026-07-22T10:11:00Z";
const FIXTURE_COMPACT_COMPLETED: &str = "2026-07-22T10:25:02Z";

pub struct Database {
    conn: Connection,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("time parse error: {0}")]
    TimeParse(#[from] chrono::ParseError),
    #[error("invalid data: {0}")]
    InvalidData(String),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportEnvelope {
    app_version: String,
    exported_at: String,
    redacted: bool,
    dashboard: Dashboard,
    settings: AppSettings,
    capabilities: Vec<CapabilityStatus>,
    sources: Vec<SourceStatus>,
    counts: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, PartialEq)]
struct BurnEvidence {
    headline: String,
    factors: Vec<String>,
    missing_signals: Vec<String>,
    method: String,
    confidence: BurnConfidence,
}

#[derive(Debug)]
struct BurnContext {
    turn: ExpensiveTurn,
    cached_input: i64,
    input: i64,
    compaction_count: i64,
    subagent_count: i64,
}

#[allow(dead_code)]
impl Database {
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        let db = Self { conn };
        db.migrate()?;
        db.clear_legacy_persisted_fixture()?;
        db.seed_defaults()?;
        db.prune_by_retention()?;
        Ok(db)
    }

    fn clear_legacy_persisted_fixture(&self) -> Result<(), DbError> {
        if !self.demo_mode()? {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        Self::clear_analytics_tx(&tx)?;
        Self::reset_sources_tx(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn record_codex_installation(&mut self, version: &str) -> Result<(), DbError> {
        let normalized = version
            .trim()
            .strip_prefix("codex-cli ")
            .unwrap_or(version.trim());
        if normalized.is_empty() {
            return Err(DbError::InvalidData("Codex version was empty".to_string()));
        }
        self.conn.execute(
            "INSERT INTO codex_installations(id, version, schema_version, executable_path, detected_at)
             VALUES('local-codex', ?1, ?1, NULL, ?2)
             ON CONFLICT(id) DO UPDATE SET
               version = excluded.version,
               schema_version = excluded.schema_version,
               detected_at = excluded.detected_at",
            params![normalized, now_rfc3339()],
        )?;
        Ok(())
    }

    fn sync_forecast_alerts(
        &self,
        forecast: &QuotaForecast,
        reset_window_id: &str,
    ) -> Result<(), DbError> {
        let (Some(used_percent), Some(remaining_percent)) =
            (forecast.current_used_percent, forecast.remaining_percent)
        else {
            return Ok(());
        };
        let exhaustion_hours = forecast
            .predicted_exhaustion_at
            .as_deref()
            .and_then(|value| {
                let predicted = chrono::DateTime::parse_from_rfc3339(value).ok()?;
                let generated =
                    chrono::DateTime::parse_from_rfc3339(&forecast.generated_at).ok()?;
                Some((predicted - generated).num_seconds() as f64 / 3600.0)
            });
        let candidates = [
            (
                "pace_exceeds_safe",
                1001,
                forecast.pace_ratio.is_some_and(|ratio| ratio >= 1.0),
                if forecast.pace_ratio.is_some_and(|ratio| ratio > 1.5) {
                    AlertSeverity::Critical
                } else {
                    AlertSeverity::Warning
                },
            ),
            (
                "forecast_exhaustion_before_reset",
                1002,
                forecast.exhaustion_before_reset == Some(true),
                AlertSeverity::Critical,
            ),
            (
                "forecast_exhaustion_within_4h",
                1003,
                exhaustion_hours.is_some_and(|hours| (1.0..=4.0).contains(&hours)),
                AlertSeverity::Warning,
            ),
            (
                "forecast_exhaustion_within_1h",
                1004,
                exhaustion_hours.is_some_and(|hours| (0.0..1.0).contains(&hours)),
                AlertSeverity::Critical,
            ),
        ];
        let now = now_rfc3339();
        for (alert_type, code, active, severity) in candidates {
            if !active {
                self.conn.execute(
                    "UPDATE alert_history
                     SET resolved_at = COALESCE(resolved_at, ?4), unread = 0
                     WHERE bucket_id = ?1 AND reset_window_id = ?2 AND alert_type = ?3",
                    params![forecast.bucket_id, reset_window_id, alert_type, now],
                )?;
                continue;
            }
            let id = alert_id(&forecast.bucket_id, reset_window_id, code);
            self.conn.execute(
                "INSERT INTO alert_history(
                    id, bucket_id, bucket_name, reset_window_id, resets_at, threshold,
                    used_percent, remaining_percent, severity, accuracy,
                    collection_timestamp, created_at, dismissed_at, resolved_at, unread,
                    alert_type, alert_source, predicted_exhaustion_at, forecast_confidence
                 )
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'estimated',
                        ?10, ?10, NULL, NULL, 1, ?11, 'local_forecast', ?12, ?13)
                 ON CONFLICT(bucket_id, reset_window_id, threshold) DO UPDATE SET
                   used_percent = excluded.used_percent,
                   remaining_percent = excluded.remaining_percent,
                   severity = excluded.severity,
                   collection_timestamp = excluded.collection_timestamp,
                   resolved_at = NULL,
                   predicted_exhaustion_at = excluded.predicted_exhaustion_at,
                   forecast_confidence = excluded.forecast_confidence",
                params![
                    id,
                    forecast.bucket_id,
                    forecast.bucket_name,
                    reset_window_id,
                    forecast.resets_at,
                    code,
                    used_percent,
                    remaining_percent,
                    severity_str(severity),
                    now,
                    alert_type,
                    forecast.predicted_exhaustion_at,
                    forecast_confidence_str(forecast.quality.confidence),
                ],
            )?;
        }
        Ok(())
    }

    pub fn dashboard(&self) -> Result<Dashboard, DbError> {
        let codex_version = self.codex_version()?;
        let mut dashboard = Dashboard::empty(codex_version.clone());
        dashboard.demo_mode = self.demo_mode()?;
        dashboard.collector_state = self.collector_state()?;
        dashboard.last_event_at = self.last_event_at()?;
        dashboard.quota = self.quota_windows()?;
        dashboard.quota_status =
            self.account_signal_status("quota_snapshots", "Quota windows", "Codex App Server")?;
        dashboard.forecasts = self.quota_forecasts()?;
        dashboard.forecast_status = dashboard
            .forecasts
            .iter()
            .find(|forecast| forecast.status.state == TelemetryState::Live)
            .or_else(|| dashboard.forecasts.first())
            .map(|forecast| forecast.status.clone())
            .unwrap_or_else(|| Dashboard::empty(codex_version.clone()).forecast_status);
        if !dashboard.quota.is_empty()
            && dashboard
                .quota
                .iter()
                .all(|window| window.used_percent.is_none())
        {
            dashboard.quota_status.state = TelemetryState::Unavailable;
            dashboard.quota_status.accuracy = Accuracy::Unavailable;
            dashboard.quota_status.reason = Some(
                "The latest quota snapshot did not include a reliable used percentage.".into(),
            );
        }
        dashboard.account_usage = self.account_usage_summary()?;
        dashboard.account_usage_status = self.account_signal_status(
            "account_usage_snapshots",
            "Account activity",
            "Codex App Server",
        )?;
        if dashboard
            .account_usage
            .as_ref()
            .is_some_and(|summary| summary.accuracy == Accuracy::Unavailable)
        {
            dashboard.account_usage_status.state = TelemetryState::Unavailable;
            dashboard.account_usage_status.accuracy = Accuracy::Unavailable;
            dashboard.account_usage_status.reason =
                Some("The latest account response did not include a reliable usage value.".into());
        }
        dashboard.today = self.today_totals()?;
        dashboard.token_status = self.detailed_signal_status(
            "usage_records",
            "Token composition",
            Accuracy::ReportedExact,
        )?;
        if dashboard.today.total.is_none() && dashboard.token_status.state == TelemetryState::Live {
            dashboard.token_status.state = TelemetryState::Unavailable;
            dashboard.token_status.accuracy = Accuracy::Unavailable;
            dashboard.token_status.reason =
                Some("Token telemetry arrived without a reliable total.".to_string());
        }
        dashboard.turns = self.expensive_turns()?;
        dashboard.turns_status = self.detailed_signal_status(
            "usage_records",
            "Recent expensive turns",
            Accuracy::ReportedExact,
        )?;
        dashboard.burn = self.burn_analysis(&dashboard.turns)?;
        dashboard.burn_status = self.burn_signal_status(&dashboard.burn)?;
        for signal in &mut dashboard.burn.missing_signals {
            let status = match signal.name.as_str() {
                "Token usage" => Some(&dashboard.token_status),
                "Quota snapshots" => Some(&dashboard.quota_status),
                "Completed turns" => Some(&dashboard.turns_status),
                _ => None,
            };
            if let Some(status) = status {
                signal.setup_state = status.state;
                if let Some(reason) = &status.reason {
                    signal.detail = reason.clone();
                }
            }
        }
        dashboard.channels = self.channel_health()?;
        dashboard.collector_diagnostics = self.collector_diagnostics()?;
        dashboard.model_distribution = self.distribution_by_model()?;
        dashboard.reasoning_distribution = self.distribution_by_reasoning()?;
        dashboard.model_status = self.detailed_signal_status(
            "usage_records",
            "Models and reasoning",
            Accuracy::ReportedExact,
        )?;
        dashboard.alerts = self.alert_center()?;
        dashboard.warnings = self.dashboard_warnings(&dashboard);
        Ok(dashboard)
    }

    pub fn record_rate_limits(
        &mut self,
        result_or_notification: &Value,
        observed_at: &str,
        source_channel: &str,
    ) -> Result<(), DbError> {
        let source = SourceIdentity::from_input(source_channel);
        let snapshots = rate_limit_snapshots(result_or_notification)?;
        let tx = self.conn.transaction()?;
        Self::ensure_source_tx(&tx, &source)?;

        for snapshot in snapshots {
            for window in ["primary", "secondary"] {
                let Some(window_value) = snapshot.value.get(window) else {
                    continue;
                };
                if window_value.is_null() {
                    continue;
                }
                let effective_limit_key = snapshot
                    .limit_id
                    .clone()
                    .unwrap_or_else(|| snapshot.storage_key.clone());
                let bucket_id =
                    quota_bucket_id(source.id.as_str(), effective_limit_key.as_str(), window);
                let existing_bucket = Self::existing_bucket_tx(&tx, &bucket_id)?;
                let name = snapshot
                    .limit_name
                    .clone()
                    .or_else(|| existing_bucket.as_ref().map(|bucket| bucket.name.clone()))
                    .unwrap_or_else(|| snapshot.storage_key.clone());
                let window_duration_minutes = nullable_i64(window_value.get("windowDurationMins"))
                    .or_else(|| existing_bucket.as_ref().and_then(|bucket| bucket.duration));
                tx.execute(
                    "INSERT INTO quota_buckets(
                        id, source_id, external_limit_id, name, window_kind, window_duration_minutes
                     )
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(id) DO UPDATE SET
                       source_id = excluded.source_id,
                       external_limit_id = excluded.external_limit_id,
                       name = excluded.name,
                       window_kind = excluded.window_kind,
                       window_duration_minutes = excluded.window_duration_minutes",
                    params![
                        bucket_id,
                        source.id.as_str(),
                        snapshot.limit_id.as_deref(),
                        name,
                        window,
                        window_duration_minutes
                    ],
                )?;

                let used_percent = exact_percent(window_value.get("usedPercent"));
                let remaining_percent = used_percent.map(|used| 100.0 - used);
                let raw_reset = nullable_i64(window_value.get("resetsAt"));
                let normalized_reset = normalize_reset_timestamp(raw_reset);
                let accuracy = if used_percent.is_some() {
                    Accuracy::ReportedExact
                } else {
                    Accuracy::Unavailable
                };

                let candidate = QuotaSnapshotCandidate {
                    used_percent,
                    remaining_percent,
                    resets_at: normalized_reset.normalized.as_deref(),
                    resets_at_raw: raw_reset,
                    reset_interpretation: normalized_reset.interpretation.as_str(),
                    accuracy,
                };
                if Self::quota_snapshot_is_duplicate_tx(&tx, bucket_id.as_str(), &candidate)? {
                    continue;
                }

                let snapshot_id = format!("{bucket_id}:{observed_at}");
                tx.execute(
                    "INSERT INTO quota_snapshots(
                        id, bucket_id, observed_at, used_percent, remaining_percent, resets_at,
                        accuracy, resets_at_raw, reset_interpretation
                     )
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                     ON CONFLICT(bucket_id, observed_at) DO UPDATE SET
                       used_percent = excluded.used_percent,
                       remaining_percent = excluded.remaining_percent,
                       resets_at = excluded.resets_at,
                       accuracy = excluded.accuracy,
                       resets_at_raw = excluded.resets_at_raw,
                       reset_interpretation = excluded.reset_interpretation",
                    params![
                        snapshot_id,
                        bucket_id,
                        observed_at,
                        used_percent,
                        remaining_percent,
                        normalized_reset.normalized,
                        accuracy_str(accuracy),
                        raw_reset,
                        normalized_reset.interpretation
                    ],
                )?;
            }
        }

        Self::update_source_tx(
            &tx,
            source.id.as_str(),
            ChannelStatus::Healthy,
            observed_at,
            "Live rate limits recorded",
        )?;
        tx.commit()?;
        self.evaluate_completed_forecasts()?;
        Ok(())
    }

    pub fn record_account_usage(
        &mut self,
        result: &Value,
        observed_at: &str,
        source_channel: &str,
    ) -> Result<(), DbError> {
        let source = SourceIdentity::from_input(source_channel);
        let tx = self.conn.transaction()?;
        Self::ensure_source_tx(&tx, &source)?;

        let summary = result.get("summary").unwrap_or(&Value::Null);
        let daily_usage = result
            .get("dailyUsageBuckets")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let lifetime_tokens = nullable_i64(summary.get("lifetimeTokens"));
        let peak_daily_tokens = nullable_i64(summary.get("peakDailyTokens"));
        let longest_running_turn_sec = nullable_i64(summary.get("longestRunningTurnSec"));
        let current_streak_days = nullable_i64(summary.get("currentStreakDays"));
        let longest_streak_days = nullable_i64(summary.get("longestStreakDays"));
        let accuracy = if [
            lifetime_tokens,
            peak_daily_tokens,
            longest_running_turn_sec,
            current_streak_days,
            longest_streak_days,
        ]
        .into_iter()
        .any(|value| value.is_some())
            || !daily_usage.is_empty()
        {
            Accuracy::ReportedExact
        } else {
            Accuracy::Unavailable
        };

        tx.execute(
            "INSERT INTO account_usage_snapshots(
                id, source_id, observed_at, lifetime_tokens, peak_daily_tokens,
                longest_running_turn_sec, current_streak_days, longest_streak_days, accuracy
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
               source_id = excluded.source_id,
               observed_at = excluded.observed_at,
               lifetime_tokens = excluded.lifetime_tokens,
               peak_daily_tokens = excluded.peak_daily_tokens,
               longest_running_turn_sec = excluded.longest_running_turn_sec,
               current_streak_days = excluded.current_streak_days,
               longest_streak_days = excluded.longest_streak_days,
               accuracy = excluded.accuracy",
            params![
                format!("account-usage:{}:{observed_at}", source.id),
                source.id.as_str(),
                observed_at,
                lifetime_tokens,
                peak_daily_tokens,
                longest_running_turn_sec,
                current_streak_days,
                longest_streak_days,
                accuracy_str(accuracy)
            ],
        )?;

        for bucket in daily_usage {
            let Some(start_date) = bucket.get("startDate").and_then(Value::as_str) else {
                continue;
            };
            let Some(tokens) = nullable_i64(bucket.get("tokens")) else {
                continue;
            };
            tx.execute(
                "INSERT INTO account_usage_daily(
                    source_id, start_date, tokens, observed_at, accuracy
                 )
                 VALUES(?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(source_id, start_date) DO UPDATE SET
                   tokens = excluded.tokens,
                   observed_at = excluded.observed_at,
                   accuracy = excluded.accuracy",
                params![
                    source.id.as_str(),
                    start_date,
                    tokens,
                    observed_at,
                    accuracy_str(Accuracy::ReportedExact)
                ],
            )?;
        }

        Self::update_source_tx(
            &tx,
            source.id.as_str(),
            ChannelStatus::Healthy,
            observed_at,
            "Live account usage recorded",
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn claim_quota_threshold_notifications(
        &mut self,
    ) -> Result<Vec<QuotaThresholdEvent>, DbError> {
        if self.demo_mode()? {
            return Ok(Vec::new());
        }

        let thresholds = self.settings()?.quota_thresholds;
        let tx = self.conn.transaction()?;
        let candidates = {
            let mut statement = tx.prepare(
                "SELECT
                    qb.id,
                    qb.name,
                    qs.used_percent,
                    qs.remaining_percent,
                    qs.resets_at,
                    qs.resets_at_raw,
                    qs.accuracy,
                    qs.observed_at
                 FROM quota_buckets qb
                 JOIN quota_snapshots qs ON qs.id = (
                    SELECT latest.id
                    FROM quota_snapshots latest
                    WHERE latest.bucket_id = qb.id
                    ORDER BY latest.observed_at DESC
                    LIMIT 1
                 )",
            )?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<f64>>(2)?,
                        row.get::<_, Option<f64>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };

        let mut notifications = Vec::new();
        for (
            bucket_id,
            bucket_name,
            used_percent,
            remaining_percent,
            resets_at,
            resets_at_raw,
            accuracy,
            collection_timestamp,
        ) in candidates
        {
            let (Some(used_percent), Some(remaining_percent), Some(resets_at_raw)) =
                (used_percent, remaining_percent, resets_at_raw)
            else {
                continue;
            };
            if accuracy != accuracy_str(Accuracy::ReportedExact) {
                continue;
            }
            let reset_key = format!("raw:{resets_at_raw}");
            for threshold in &thresholds {
                if used_percent < *threshold as f64 {
                    continue;
                }
                let created_at = now_rfc3339();
                let severity = alert_severity(used_percent);
                let alert_id = alert_id(&bucket_id, &reset_key, *threshold);
                tx.execute(
                    "INSERT INTO alert_history(
                        id, bucket_id, bucket_name, reset_window_id, resets_at, threshold,
                        used_percent, remaining_percent, severity, accuracy,
                        collection_timestamp, created_at, dismissed_at
                     )
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL)
                     ON CONFLICT(bucket_id, reset_window_id, threshold) DO UPDATE SET
                       used_percent = excluded.used_percent,
                       remaining_percent = excluded.remaining_percent,
                       severity = excluded.severity,
                       collection_timestamp = excluded.collection_timestamp",
                    params![
                        alert_id,
                        bucket_id,
                        bucket_name,
                        reset_key,
                        resets_at,
                        threshold,
                        used_percent,
                        remaining_percent,
                        severity_str(severity),
                        accuracy_str(Accuracy::ReportedExact),
                        collection_timestamp,
                        created_at,
                    ],
                )?;
                let inserted = tx.execute(
                    "INSERT INTO notification_state(bucket_id, reset_key, threshold, notified_at)
                     VALUES(?1, ?2, ?3, ?4)
                     ON CONFLICT(bucket_id, reset_key, threshold) DO NOTHING",
                    params![bucket_id, reset_key, threshold, created_at],
                )?;
                if inserted == 1 {
                    notifications.push(QuotaThresholdEvent {
                        bucket_id: bucket_id.clone(),
                        bucket_name: bucket_name.clone(),
                        threshold: *threshold,
                        used_percent,
                        resets_at: resets_at.clone(),
                        remaining_percent,
                        collection_timestamp: collection_timestamp.clone(),
                        severity,
                        accuracy: Accuracy::ReportedExact,
                    });
                }
            }
        }
        tx.commit()?;
        Ok(notifications)
    }

    pub fn dismiss_alert(
        &mut self,
        bucket_id: &str,
        reset_window_id: &str,
        threshold: i64,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE alert_history
             SET dismissed_at = COALESCE(dismissed_at, ?4), unread = 0
             WHERE bucket_id = ?1 AND reset_window_id = ?2 AND threshold = ?3",
            params![bucket_id, reset_window_id, threshold, now_rfc3339()],
        )?;
        Ok(())
    }

    pub fn mark_alerts_read(&mut self) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE alert_history
             SET unread = 0
             WHERE unread = 1 AND dismissed_at IS NULL AND resolved_at IS NULL",
            [],
        )?;
        Ok(())
    }

    pub fn record_token_usage_notification(
        &mut self,
        notification: &Value,
        observed_at: &str,
        source_channel: &str,
    ) -> Result<(), DbError> {
        let source = SourceIdentity::from_input(source_channel);
        let thread_id = notification
            .get("threadId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                DbError::InvalidData("thread/tokenUsage/updated missing threadId".to_string())
            })?;
        let turn_id = notification
            .get("turnId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                DbError::InvalidData("thread/tokenUsage/updated missing turnId".to_string())
            })?;
        let last = notification
            .get("tokenUsage")
            .and_then(|usage| usage.get("last"))
            .ok_or_else(|| {
                DbError::InvalidData(
                    "thread/tokenUsage/updated missing tokenUsage.last".to_string(),
                )
            })?;

        let tx = self.conn.transaction()?;
        Self::ensure_source_tx(&tx, &source)?;
        Self::ensure_thread_turn_stub_tx(&tx, &source, thread_id, turn_id, observed_at)?;
        let external_event_id = format!("thread-token-usage:{thread_id}:{turn_id}");
        tx.execute(
            "INSERT INTO usage_records(
                id, source_id, external_event_id, thread_id, turn_id, model_id, observed_at,
                input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens,
                total_tokens, accuracy
             )
             VALUES(?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(source_id, external_event_id) DO UPDATE SET
               thread_id = excluded.thread_id,
               turn_id = excluded.turn_id,
               observed_at = excluded.observed_at,
               input_tokens = excluded.input_tokens,
               cached_input_tokens = excluded.cached_input_tokens,
               output_tokens = excluded.output_tokens,
               reasoning_output_tokens = excluded.reasoning_output_tokens,
               total_tokens = excluded.total_tokens,
               accuracy = excluded.accuracy",
            params![
                format!("usage:{}:{thread_id}:{turn_id}", source.id),
                source.id.as_str(),
                external_event_id,
                thread_id,
                turn_id,
                observed_at,
                required_i64(last.get("inputTokens"), "inputTokens")?,
                required_i64(last.get("cachedInputTokens"), "cachedInputTokens")?,
                required_i64(last.get("outputTokens"), "outputTokens")?,
                required_i64(last.get("reasoningOutputTokens"), "reasoningOutputTokens")?,
                required_i64(last.get("totalTokens"), "totalTokens")?,
                accuracy_str(Accuracy::ReportedExact)
            ],
        )?;
        Self::update_source_tx(
            &tx,
            source.id.as_str(),
            ChannelStatus::Healthy,
            observed_at,
            "Live token usage recorded",
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn record_collector_health(
        &mut self,
        source_channel: &str,
        status: ChannelStatus,
        observed_at: &str,
        detail: &str,
    ) -> Result<(), DbError> {
        let source = SourceIdentity::from_input(source_channel);
        let tx = self.conn.transaction()?;
        Self::ensure_source_tx(&tx, &source)?;
        Self::update_source_tx(&tx, source.id.as_str(), status, observed_at, detail)?;
        tx.commit()?;
        Ok(())
    }

    pub fn record_collector_session(
        &mut self,
        session_id: &str,
        started_at: &str,
        ended_at: Option<&str>,
        health: &str,
        restart_count: i64,
    ) -> Result<(), DbError> {
        let tx = self.conn.transaction()?;
        let redacted_health = redact_diagnostic(health);
        tx.execute(
            "INSERT INTO collector_sessions(id, started_at, ended_at, health, restart_count)
             VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               started_at = excluded.started_at,
               ended_at = excluded.ended_at,
               health = excluded.health,
               restart_count = excluded.restart_count",
            params![
                session_id,
                started_at,
                ended_at,
                redacted_health.as_str(),
                restart_count.max(0)
            ],
        )?;
        Self::set_metadata_json(&tx, "collector_state", &redacted_health)?;
        tx.commit()?;
        Ok(())
    }

    pub fn record_collection_error(
        &mut self,
        source_channel: &str,
        occurred_at: &str,
        category: &str,
        message: &str,
        retryable: bool,
    ) -> Result<(), DbError> {
        let source = SourceIdentity::from_input(source_channel);
        let tx = self.conn.transaction()?;
        Self::ensure_source_tx(&tx, &source)?;
        tx.execute(
            "INSERT INTO collection_errors(
                source_id, occurred_at, category, redacted_message, retryable
             )
             VALUES(?1, ?2, ?3, ?4, ?5)",
            params![
                source.id,
                occurred_at,
                category,
                redact_diagnostic(message),
                retryable as i64
            ],
        )?;
        Self::update_source_tx(
            &tx,
            source.id.as_str(),
            ChannelStatus::Degraded,
            occurred_at,
            "Collection failed; see Diagnostics for the redacted error.",
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn load_synthetic_fixture(&mut self) -> Result<(), DbError> {
        let tx = self.conn.transaction()?;
        Self::seed_defaults_tx(&tx)?;
        Self::clear_analytics_tx(&tx)?;
        Self::upsert_fixture_installation(&tx)?;
        Self::upsert_fixture_projects(&tx)?;
        Self::upsert_fixture_turns(&tx)?;
        Self::upsert_fixture_usage(&tx)?;
        Self::upsert_fixture_quota(&tx)?;
        Self::upsert_fixture_account_usage(&tx)?;
        Self::upsert_fixture_events(&tx)?;
        Self::upsert_fixture_burn_analysis(&tx)?;
        Self::upsert_fixture_collector_state(&tx)?;
        Self::set_metadata_json(&tx, "demo_mode", &true)?;
        tx.commit()?;
        Ok(())
    }

    pub fn settings(&self) -> Result<AppSettings, DbError> {
        let default = AppSettings::default();
        Ok(AppSettings {
            collection_enabled: self
                .setting_value("collectionEnabled", default.collection_enabled)?,
            minimize_to_tray: self.setting_value("minimizeToTray", default.minimize_to_tray)?,
            launch_at_login: self.setting_value("launchAtLogin", default.launch_at_login)?,
            retention_days: self.setting_value("retentionDays", default.retention_days)?,
            quota_thresholds: self
                .setting_value("quotaThresholds", default.quota_thresholds.clone())?,
            raw_event_retention_enabled: self.setting_value(
                "rawEventRetentionEnabled",
                default.raw_event_retention_enabled,
            )?,
        })
    }

    pub fn save_settings(&mut self, settings: &AppSettings) -> Result<(), DbError> {
        let sanitized = sanitize_settings(settings);
        let tx = self.conn.transaction()?;
        Self::set_setting_json(&tx, "collectionEnabled", &sanitized.collection_enabled)?;
        Self::set_setting_json(&tx, "minimizeToTray", &sanitized.minimize_to_tray)?;
        Self::set_setting_json(&tx, "launchAtLogin", &sanitized.launch_at_login)?;
        Self::set_setting_json(&tx, "retentionDays", &sanitized.retention_days)?;
        Self::set_setting_json(&tx, "quotaThresholds", &sanitized.quota_thresholds)?;
        Self::set_setting_json(
            &tx,
            "rawEventRetentionEnabled",
            &sanitized.raw_event_retention_enabled,
        )?;
        tx.commit()?;
        self.prune_by_retention()?;
        Ok(())
    }

    pub fn delete_analytics(&mut self) -> Result<(), DbError> {
        let tx = self.conn.transaction()?;
        Self::clear_analytics_tx(&tx)?;
        Self::reset_sources_tx(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn export_json(&self) -> Result<String, DbError> {
        let envelope = ExportEnvelope {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            exported_at: now_rfc3339(),
            redacted: true,
            dashboard: self.dashboard()?,
            settings: self.settings()?,
            capabilities: self.capabilities()?,
            sources: self.sources()?,
            counts: self.table_counts()?,
        };
        Ok(serde_json::to_string_pretty(&envelope)?)
    }

    pub fn export_csv(&self) -> Result<String, DbError> {
        let dashboard = self.dashboard()?;
        let mut lines = Vec::new();
        lines.push(csv_row(&[
            "section",
            "id",
            "label",
            "value",
            "accuracy",
            "detail",
            "secondary",
            "tertiary",
            "quaternary",
        ]));

        for quota in &dashboard.quota {
            lines.push(csv_row(&[
                "quota",
                &quota.id,
                &quota.name,
                &quota
                    .remaining_percent
                    .map(format_percent)
                    .unwrap_or_else(|| "".to_string()),
                accuracy_str(quota.accuracy),
                &quota.window_label,
                &quota
                    .used_percent
                    .map(format_percent)
                    .unwrap_or_else(|| "".to_string()),
                quota.resets_at.as_deref().unwrap_or(""),
                quota.reset_interpretation.as_str(),
            ]));
        }

        for forecast in &dashboard.forecasts {
            lines.push(csv_row(&[
                "quota_forecast",
                &forecast.bucket_id,
                &forecast.window_label,
                &forecast
                    .selected_rate_pph
                    .map(|value| format!("{value:.4}"))
                    .unwrap_or_default(),
                accuracy_str(Accuracy::Estimated),
                forecast_risk_str(forecast.risk),
                forecast.predicted_exhaustion_at.as_deref().unwrap_or(""),
                &forecast
                    .projected_usage_at_reset
                    .map(format_percent)
                    .unwrap_or_default(),
                forecast_confidence_str(forecast.quality.confidence),
            ]));
            lines.push(csv_row(&[
                "quota_safe_rate",
                &forecast.bucket_id,
                &forecast.window_label,
                &forecast
                    .safe_rate_pph
                    .map(|value| format!("{value:.4}"))
                    .unwrap_or_default(),
                accuracy_str(Accuracy::DerivedExact),
                "percentage points per hour",
                forecast.resets_at.as_deref().unwrap_or(""),
                "",
                "",
            ]));
        }

        for alert in &dashboard.alerts.history {
            lines.push(csv_row(&[
                "alert",
                &alert.id,
                &alert.alert_type,
                &format_percent(alert.used_percent),
                accuracy_str(alert.accuracy),
                &alert.alert_source,
                &alert.bucket_id,
                alert.resets_at.as_deref().unwrap_or(""),
                alert.resolved_at.as_deref().unwrap_or(""),
            ]));
        }

        if let Some(account_usage) = &dashboard.account_usage {
            for (label, value) in [
                ("lifetimeTokens", account_usage.lifetime_tokens),
                ("peakDailyTokens", account_usage.peak_daily_tokens),
                (
                    "longestRunningTurnSec",
                    account_usage.longest_running_turn_sec,
                ),
                ("currentStreakDays", account_usage.current_streak_days),
                ("longestStreakDays", account_usage.longest_streak_days),
            ] {
                lines.push(csv_row(&[
                    "account_usage",
                    &account_usage.observed_at,
                    label,
                    &value.map(|item| item.to_string()).unwrap_or_default(),
                    accuracy_str(account_usage.accuracy),
                    "",
                    "",
                    "",
                    "",
                ]));
            }
            for bucket in &account_usage.daily_usage_buckets {
                lines.push(csv_row(&[
                    "account_usage_day",
                    &bucket.start_date,
                    "tokens",
                    &bucket.tokens.to_string(),
                    accuracy_str(bucket.accuracy),
                    &bucket.observed_at,
                    "",
                    "",
                    "",
                ]));
            }
        }

        for turn in &dashboard.turns {
            lines.push(csv_row(&[
                "turn",
                &turn.id,
                turn.project.as_deref().unwrap_or("Unattributed"),
                &turn.tokens.to_string(),
                accuracy_str(turn.accuracy),
                &turn.thread_id,
                turn.model.as_deref().unwrap_or(""),
                turn.reasoning_effort.as_deref().unwrap_or(""),
                &turn
                    .duration_ms
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            ]));
        }

        for channel in &dashboard.channels {
            lines.push(csv_row(&[
                "channel",
                &channel.name,
                status_str(channel.status),
                "",
                "",
                &channel.detail,
                "",
                "",
                "",
            ]));
        }

        for session in &dashboard.collector_diagnostics.recent_sessions {
            lines.push(csv_row(&[
                "collector_session",
                &session.id,
                &session.health,
                &session.restart_count.to_string(),
                "",
                &session.started_at,
                session.ended_at.as_deref().unwrap_or(""),
                "",
                "",
            ]));
        }

        for error in &dashboard.collector_diagnostics.recent_errors {
            lines.push(csv_row(&[
                "collector_error",
                error.source_id.as_deref().unwrap_or(""),
                &error.category,
                "",
                "",
                &error.redacted_message,
                &error.occurred_at,
                if error.retryable {
                    "retryable"
                } else {
                    "terminal"
                },
                "",
            ]));
        }

        for warning in &dashboard.warnings {
            lines.push(csv_row(&["warning", "", "", "", "", warning, "", "", ""]));
        }

        Ok(lines.join("\n"))
    }

    pub fn diagnostics_json(&self) -> Result<String, DbError> {
        Ok(serde_json::to_string_pretty(&self.diagnostics_report()?)?)
    }

    fn migrate(&self) -> Result<(), DbError> {
        let tx = self.conn.unchecked_transaction()?;
        for (version, name, sql) in MIGRATIONS {
            apply_migration(&tx, *version, name, sql)?;
        }
        tx.commit()?;
        Ok(())
    }

    fn seed_defaults(&self) -> Result<(), DbError> {
        let tx = self.conn.unchecked_transaction()?;
        Self::seed_defaults_tx(&tx)?;
        tx.commit()?;
        Ok(())
    }

    fn seed_defaults_tx(tx: &Transaction<'_>) -> Result<(), DbError> {
        Self::seed_sources_tx(tx)?;
        let now = now_rfc3339();
        for capability in default_capabilities() {
            tx.execute(
                "INSERT INTO capabilities(
                    metric, source_channel, source_method, source_field, availability,
                    accuracy, limitation, updated_at
                 )
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(metric) DO UPDATE SET
                   source_channel = excluded.source_channel,
                   source_method = excluded.source_method,
                   source_field = excluded.source_field,
                   availability = excluded.availability,
                   accuracy = excluded.accuracy,
                   limitation = excluded.limitation,
                   updated_at = excluded.updated_at",
                params![
                    capability.metric,
                    capability.source_channel,
                    capability.source_method,
                    capability.source_field,
                    capability.availability,
                    accuracy_str(capability.accuracy),
                    capability.limitation,
                    now,
                ],
            )?;
        }

        let defaults = AppSettings::default();
        Self::set_setting_if_missing(tx, "collectionEnabled", &defaults.collection_enabled)?;
        Self::set_setting_if_missing(tx, "minimizeToTray", &defaults.minimize_to_tray)?;
        Self::set_setting_if_missing(tx, "launchAtLogin", &defaults.launch_at_login)?;
        Self::set_setting_if_missing(tx, "retentionDays", &defaults.retention_days)?;
        Self::set_setting_if_missing(tx, "quotaThresholds", &defaults.quota_thresholds)?;
        Self::set_setting_if_missing(
            tx,
            "rawEventRetentionEnabled",
            &defaults.raw_event_retention_enabled,
        )?;
        Self::set_metadata_if_missing(tx, "demo_mode", &false)?;
        Ok(())
    }

    fn clear_analytics_tx(tx: &Transaction<'_>) -> Result<(), DbError> {
        for statement in [
            "DELETE FROM notification_state",
            "DELETE FROM alert_history",
            "DELETE FROM quota_forecast_history",
            "DELETE FROM burn_analyses",
            "DELETE FROM quota_snapshots",
            "DELETE FROM quota_buckets",
            "DELETE FROM account_usage_daily",
            "DELETE FROM account_usage_snapshots",
            "DELETE FROM usage_records",
            "DELETE FROM hook_events",
            "DELETE FROM otel_events",
            "DELETE FROM turns",
            "DELETE FROM threads",
            "DELETE FROM worktrees",
            "DELETE FROM repository_locations",
            "DELETE FROM projects",
            "DELETE FROM models",
            "DELETE FROM reasoning_settings",
            "DELETE FROM codex_installations",
            "DELETE FROM collector_sessions",
            "DELETE FROM collection_errors",
            "DELETE FROM exports",
            "DELETE FROM app_metadata WHERE key IN ('demo_mode', 'collector_state')",
        ] {
            tx.execute(statement, [])?;
        }
        Self::set_metadata_json(tx, "demo_mode", &false)?;
        Ok(())
    }

    fn reset_sources_tx(tx: &Transaction<'_>) -> Result<(), DbError> {
        let defaults = [
            (
                "app-server",
                "app_server",
                true,
                ChannelStatus::Inactive,
                "Waiting for collector startup",
            ),
            (
                "lifecycle-hooks",
                "lifecycle_hooks",
                false,
                ChannelStatus::Inactive,
                "Not configured",
            ),
            (
                "opentelemetry",
                "opentelemetry",
                false,
                ChannelStatus::Inactive,
                "Not configured",
            ),
        ];

        for (id, channel, enabled, health, detail) in defaults {
            tx.execute(
                "INSERT INTO sources(
                    id, channel, enabled, health, last_event_at, detail, last_success_at
                 )
                 VALUES(?1, ?2, ?3, ?4, NULL, ?5, NULL)
                 ON CONFLICT(id) DO UPDATE SET
                   channel = excluded.channel,
                   enabled = excluded.enabled,
                   health = excluded.health,
                   last_event_at = excluded.last_event_at,
                   detail = excluded.detail,
                   last_success_at = NULL",
                params![id, channel, enabled as i64, status_str(health), detail],
            )?;
        }
        Ok(())
    }

    fn seed_sources_tx(tx: &Transaction<'_>) -> Result<(), DbError> {
        let defaults = [
            (
                "app-server",
                "app_server",
                true,
                "Waiting for collector startup",
            ),
            (
                "lifecycle-hooks",
                "lifecycle_hooks",
                false,
                "Not configured",
            ),
            ("opentelemetry", "opentelemetry", false, "Not configured"),
        ];
        for (id, channel, enabled, detail) in defaults {
            tx.execute(
                "INSERT INTO sources(
                    id, channel, enabled, health, last_event_at, detail, last_success_at
                 )
                 VALUES(?1, ?2, ?3, 'inactive', NULL, ?4, NULL)
                 ON CONFLICT(id) DO NOTHING",
                params![id, channel, enabled as i64, detail],
            )?;
        }
        Ok(())
    }

    fn upsert_fixture_installation(tx: &Transaction<'_>) -> Result<(), DbError> {
        tx.execute(
            "INSERT INTO codex_installations(id, version, schema_version, executable_path, detected_at)
             VALUES(?1, ?2, ?3, NULL, ?4)
             ON CONFLICT(id) DO UPDATE SET
               version = excluded.version,
               schema_version = excluded.schema_version,
               detected_at = excluded.detected_at",
            params![
                "fixture-installation",
                DEFAULT_CODEX_VERSION,
                "v1+v2",
                FIXTURE_OCCURRED_AT
            ],
        )?;
        Ok(())
    }

    fn upsert_fixture_projects(tx: &Transaction<'_>) -> Result<(), DbError> {
        tx.execute(
            "INSERT INTO projects(id, name, normalized_root, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               normalized_root = excluded.normalized_root,
               updated_at = excluded.updated_at",
            params![
                "project-example",
                "Example Project",
                "local/example-project",
                FIXTURE_TURN_STARTED,
                FIXTURE_OCCURRED_AT
            ],
        )?;
        tx.execute(
            "INSERT INTO repository_locations(id, project_id, root_path, remote_url_redacted)
             VALUES(?1, ?2, ?3, NULL)
             ON CONFLICT(id) DO UPDATE SET
               project_id = excluded.project_id,
               root_path = excluded.root_path",
            params!["repo-example", "project-example", "local/example-project"],
        )?;
        tx.execute(
            "INSERT INTO worktrees(id, project_id, repository_location_id, path, branch, last_seen_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
               project_id = excluded.project_id,
               repository_location_id = excluded.repository_location_id,
               path = excluded.path,
               branch = excluded.branch,
               last_seen_at = excluded.last_seen_at",
            params![
                "worktree-example",
                "project-example",
                "repo-example",
                "local/example-project",
                "main",
                FIXTURE_OCCURRED_AT
            ],
        )?;
        Ok(())
    }

    fn upsert_fixture_turns(tx: &Transaction<'_>) -> Result<(), DbError> {
        tx.execute(
            "INSERT INTO models(id, display_name, provider, first_seen_at, last_seen_at)
             VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               display_name = excluded.display_name,
               provider = excluded.provider,
               last_seen_at = excluded.last_seen_at",
            params![
                "model-gpt-5.4-sol",
                "gpt-5.4-sol",
                "openai",
                FIXTURE_TURN_STARTED,
                FIXTURE_OCCURRED_AT
            ],
        )?;
        tx.execute(
            "INSERT INTO models(id, display_name, provider, first_seen_at, last_seen_at)
             VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               display_name = excluded.display_name,
               provider = excluded.provider,
               last_seen_at = excluded.last_seen_at",
            params![
                "model-gpt-5.4",
                "gpt-5.4",
                "openai",
                FIXTURE_COMPACT_STARTED,
                FIXTURE_OCCURRED_AT
            ],
        )?;
        tx.execute(
            "INSERT INTO reasoning_settings(id, effort, summary)
             VALUES(?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
               effort = excluded.effort,
               summary = excluded.summary",
            params!["reasoning-max", "max", Option::<String>::None],
        )?;
        tx.execute(
            "INSERT INTO reasoning_settings(id, effort, summary)
             VALUES(?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
               effort = excluded.effort,
               summary = excluded.summary",
            params!["reasoning-high", "high", Option::<String>::None],
        )?;
        tx.execute(
            "INSERT INTO threads(id, source_id, project_id, worktree_id, title, created_at, updated_at, status, accuracy)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
               source_id = excluded.source_id,
               project_id = excluded.project_id,
               worktree_id = excluded.worktree_id,
               title = excluded.title,
               updated_at = excluded.updated_at,
               status = excluded.status,
               accuracy = excluded.accuracy",
            params![
                "thread-fixture-sol",
                "lifecycle-hooks",
                "project-example",
                "worktree-example",
                "Fixture Sol Session",
                FIXTURE_TURN_STARTED,
                FIXTURE_OCCURRED_AT,
                "completed",
                accuracy_str(Accuracy::ReportedExact)
            ],
        )?;
        tx.execute(
            "INSERT INTO threads(id, source_id, project_id, worktree_id, title, created_at, updated_at, status, accuracy)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
               source_id = excluded.source_id,
               project_id = excluded.project_id,
               worktree_id = excluded.worktree_id,
               title = excluded.title,
               updated_at = excluded.updated_at,
               status = excluded.status,
               accuracy = excluded.accuracy",
            params![
                "thread-fixture-compact",
                "opentelemetry",
                "project-example",
                "worktree-example",
                "Fixture Compact Session",
                FIXTURE_COMPACT_STARTED,
                FIXTURE_COMPACT_COMPLETED,
                "completed",
                accuracy_str(Accuracy::ReportedExact)
            ],
        )?;
        tx.execute(
            "INSERT INTO turns(
                id, thread_id, model_id, reasoning_setting_id, started_at, completed_at,
                duration_ms, status, tool_count, compaction_count, subagent_count, retry_count, accuracy
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
               thread_id = excluded.thread_id,
               model_id = excluded.model_id,
               reasoning_setting_id = excluded.reasoning_setting_id,
               started_at = excluded.started_at,
               completed_at = excluded.completed_at,
               duration_ms = excluded.duration_ms,
               status = excluded.status,
               tool_count = excluded.tool_count,
               compaction_count = excluded.compaction_count,
               subagent_count = excluded.subagent_count,
               retry_count = excluded.retry_count,
               accuracy = excluded.accuracy",
            params![
                "turn-fixture-003",
                "thread-fixture-sol",
                "model-gpt-5.4-sol",
                "reasoning-max",
                FIXTURE_TURN_STARTED,
                FIXTURE_TURN_COMPLETED,
                10_860_000i64,
                "completed",
                18i64,
                4i64,
                3i64,
                0i64,
                accuracy_str(Accuracy::ReportedExact)
            ],
        )?;
        tx.execute(
            "INSERT INTO turns(
                id, thread_id, model_id, reasoning_setting_id, started_at, completed_at,
                duration_ms, status, tool_count, compaction_count, subagent_count, retry_count, accuracy
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
               thread_id = excluded.thread_id,
               model_id = excluded.model_id,
               reasoning_setting_id = excluded.reasoning_setting_id,
               started_at = excluded.started_at,
               completed_at = excluded.completed_at,
               duration_ms = excluded.duration_ms,
               status = excluded.status,
               tool_count = excluded.tool_count,
               compaction_count = excluded.compaction_count,
               subagent_count = excluded.subagent_count,
               retry_count = excluded.retry_count,
               accuracy = excluded.accuracy",
            params![
                "turn-fixture-002",
                "thread-fixture-compact",
                "model-gpt-5.4",
                "reasoning-high",
                FIXTURE_COMPACT_STARTED,
                FIXTURE_COMPACT_COMPLETED,
                842_000i64,
                "completed",
                6i64,
                1i64,
                0i64,
                0i64,
                accuracy_str(Accuracy::ReportedExact)
            ],
        )?;
        Ok(())
    }

    fn upsert_fixture_usage(tx: &Transaction<'_>) -> Result<(), DbError> {
        tx.execute(
            "INSERT INTO usage_records(
                id, source_id, external_event_id, thread_id, turn_id, model_id, observed_at,
                input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens, accuracy
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(source_id, external_event_id) DO UPDATE SET
               thread_id = excluded.thread_id,
               turn_id = excluded.turn_id,
               model_id = excluded.model_id,
               observed_at = excluded.observed_at,
               input_tokens = excluded.input_tokens,
               cached_input_tokens = excluded.cached_input_tokens,
               output_tokens = excluded.output_tokens,
               reasoning_output_tokens = excluded.reasoning_output_tokens,
               total_tokens = excluded.total_tokens,
               accuracy = excluded.accuracy",
            params![
                "usage-fixture-003",
                "opentelemetry",
                "usage-fixture-003",
                "thread-fixture-sol",
                "turn-fixture-003",
                "model-gpt-5.4-sol",
                FIXTURE_OCCURRED_AT,
                1_840_000i64,
                404_800i64,
                82_600i64,
                31_400i64,
                1_954_000i64,
                accuracy_str(Accuracy::ReportedExact)
            ],
        )?;
        tx.execute(
            "INSERT INTO usage_records(
                id, source_id, external_event_id, thread_id, turn_id, model_id, observed_at,
                input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens, accuracy
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(source_id, external_event_id) DO UPDATE SET
               thread_id = excluded.thread_id,
               turn_id = excluded.turn_id,
               model_id = excluded.model_id,
               observed_at = excluded.observed_at,
               input_tokens = excluded.input_tokens,
               cached_input_tokens = excluded.cached_input_tokens,
               output_tokens = excluded.output_tokens,
               reasoning_output_tokens = excluded.reasoning_output_tokens,
               total_tokens = excluded.total_tokens,
               accuracy = excluded.accuracy",
            params![
                "usage-fixture-002",
                "opentelemetry",
                "usage-fixture-002",
                "thread-fixture-compact",
                "turn-fixture-002",
                "model-gpt-5.4",
                FIXTURE_COMPACT_COMPLETED,
                120_000i64,
                24_000i64,
                40_200i64,
                0i64,
                184_200i64,
                accuracy_str(Accuracy::ReportedExact)
            ],
        )?;
        Ok(())
    }

    fn upsert_fixture_quota(tx: &Transaction<'_>) -> Result<(), DbError> {
        tx.execute(
            "INSERT INTO quota_buckets(id, source_id, external_limit_id, name, window_kind, window_duration_minutes)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
               source_id = excluded.source_id,
               external_limit_id = excluded.external_limit_id,
               name = excluded.name,
               window_kind = excluded.window_kind,
               window_duration_minutes = excluded.window_duration_minutes",
            params!["codex:primary", "app-server", "fixture-primary", "Codex", "rolling", 300i64],
        )?;
        tx.execute(
            "INSERT INTO quota_buckets(id, source_id, external_limit_id, name, window_kind, window_duration_minutes)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
               source_id = excluded.source_id,
               external_limit_id = excluded.external_limit_id,
               name = excluded.name,
               window_kind = excluded.window_kind,
               window_duration_minutes = excluded.window_duration_minutes",
            params![
                "codex:secondary",
                "app-server",
                "fixture-secondary",
                "Codex",
                "weekly",
                10_080i64
            ],
        )?;
        tx.execute(
            "INSERT INTO quota_snapshots(
                id, bucket_id, observed_at, used_percent, remaining_percent, resets_at, accuracy,
                resets_at_raw, reset_interpretation
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(bucket_id, observed_at) DO UPDATE SET
               used_percent = excluded.used_percent,
               remaining_percent = excluded.remaining_percent,
               resets_at = excluded.resets_at,
               accuracy = excluded.accuracy,
               resets_at_raw = excluded.resets_at_raw,
               reset_interpretation = excluded.reset_interpretation",
            params![
                "quota-snapshot-primary",
                "codex:primary",
                FIXTURE_OCCURRED_AT,
                68.0f64,
                32.0f64,
                FIXTURE_RESET_PRIMARY,
                accuracy_str(Accuracy::ReportedExact),
                1_753_255_200i64,
                "unix_seconds"
            ],
        )?;
        tx.execute(
            "INSERT INTO quota_snapshots(
                id, bucket_id, observed_at, used_percent, remaining_percent, resets_at, accuracy,
                resets_at_raw, reset_interpretation
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(bucket_id, observed_at) DO UPDATE SET
               used_percent = excluded.used_percent,
               remaining_percent = excluded.remaining_percent,
               resets_at = excluded.resets_at,
               accuracy = excluded.accuracy,
               resets_at_raw = excluded.resets_at_raw,
               reset_interpretation = excluded.reset_interpretation",
            params![
                "quota-snapshot-secondary",
                "codex:secondary",
                FIXTURE_OCCURRED_AT,
                41.0f64,
                59.0f64,
                FIXTURE_RESET_SECONDARY,
                accuracy_str(Accuracy::ReportedExact),
                1_753_660_800_000i64,
                "unix_milliseconds"
            ],
        )?;
        Ok(())
    }

    fn upsert_fixture_account_usage(tx: &Transaction<'_>) -> Result<(), DbError> {
        tx.execute(
            "INSERT INTO account_usage_snapshots(
                id, source_id, observed_at, lifetime_tokens, peak_daily_tokens,
                longest_running_turn_sec, current_streak_days, longest_streak_days, accuracy
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
               source_id = excluded.source_id,
               observed_at = excluded.observed_at,
               lifetime_tokens = excluded.lifetime_tokens,
               peak_daily_tokens = excluded.peak_daily_tokens,
               longest_running_turn_sec = excluded.longest_running_turn_sec,
               current_streak_days = excluded.current_streak_days,
               longest_streak_days = excluded.longest_streak_days,
               accuracy = excluded.accuracy",
            params![
                "account-usage-fixture",
                "app-server",
                FIXTURE_OCCURRED_AT,
                3_482_100i64,
                2_138_200i64,
                10_860i64,
                7i64,
                12i64,
                accuracy_str(Accuracy::ReportedExact)
            ],
        )?;
        for (start_date, tokens) in [("2026-07-21", 1_344_000i64), ("2026-07-22", 2_138_200i64)] {
            tx.execute(
                "INSERT INTO account_usage_daily(
                    source_id, start_date, tokens, observed_at, accuracy
                 )
                 VALUES(?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(source_id, start_date) DO UPDATE SET
                   tokens = excluded.tokens,
                   observed_at = excluded.observed_at,
                   accuracy = excluded.accuracy",
                params![
                    "app-server",
                    start_date,
                    tokens,
                    FIXTURE_OCCURRED_AT,
                    accuracy_str(Accuracy::ReportedExact)
                ],
            )?;
        }
        Ok(())
    }

    fn upsert_fixture_events(tx: &Transaction<'_>) -> Result<(), DbError> {
        tx.execute(
            "INSERT INTO hook_events(
                id, source_id, external_event_id, thread_id, turn_id, event_type, occurred_at,
                tool_category, succeeded, duration_ms
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(source_id, external_event_id) DO UPDATE SET
               thread_id = excluded.thread_id,
               turn_id = excluded.turn_id,
               event_type = excluded.event_type,
               occurred_at = excluded.occurred_at,
               tool_category = excluded.tool_category,
               succeeded = excluded.succeeded,
               duration_ms = excluded.duration_ms",
            params![
                "hook-fixture-sol",
                "lifecycle-hooks",
                "hook-fixture-sol",
                "thread-fixture-sol",
                "turn-fixture-003",
                "turn.completed",
                FIXTURE_OCCURRED_AT,
                "multi_tool_use",
                1i64,
                10_860_000i64
            ],
        )?;
        tx.execute(
            "INSERT INTO otel_events(
                id, source_id, external_event_id, thread_id, turn_id, event_name, occurred_at,
                request_duration_ms, outcome
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(source_id, external_event_id) DO UPDATE SET
               thread_id = excluded.thread_id,
               turn_id = excluded.turn_id,
               event_name = excluded.event_name,
               occurred_at = excluded.occurred_at,
               request_duration_ms = excluded.request_duration_ms,
               outcome = excluded.outcome",
            params![
                "otel-fixture-sol",
                "opentelemetry",
                "otel-fixture-sol",
                "thread-fixture-sol",
                "turn-fixture-003",
                "thread.tokenUsage.updated",
                FIXTURE_OCCURRED_AT,
                1_420i64,
                "ok"
            ],
        )?;
        tx.execute(
            "UPDATE sources
             SET health = ?2, last_event_at = ?3, detail = ?4
             WHERE id = ?1",
            params![
                "app-server",
                status_str(ChannelStatus::Inactive),
                FIXTURE_OCCURRED_AT,
                "0.144.1 schema verified; not connected"
            ],
        )?;
        tx.execute(
            "UPDATE sources
             SET health = ?2, last_event_at = ?3, detail = ?4
             WHERE id = ?1",
            params![
                "lifecycle-hooks",
                status_str(ChannelStatus::Healthy),
                FIXTURE_OCCURRED_AT,
                "Synthetic fixture accepted"
            ],
        )?;
        tx.execute(
            "UPDATE sources
             SET health = ?2, last_event_at = ?3, detail = ?4
             WHERE id = ?1",
            params![
                "opentelemetry",
                status_str(ChannelStatus::Healthy),
                FIXTURE_OCCURRED_AT,
                "Synthetic OTLP fixture accepted"
            ],
        )?;
        Ok(())
    }

    fn upsert_fixture_burn_analysis(tx: &Transaction<'_>) -> Result<(), DbError> {
        let evidence = json!({
            "headline": "High usage correlates with a large uncached context and extended reasoning.",
            "factors": [
                "1.84 million reported input tokens",
                "22% cache reuse",
                "Sol model usage with Max reasoning configured",
                "four compactions and three subagent sessions"
            ],
            "missingSignals": ["Exact quota units", "Direct project-level quota attribution"],
            "method": "Ranks observed signals; no causal claim is made.",
            "confidence": "medium"
        });
        tx.execute(
            "INSERT INTO burn_analyses(
                id, scope_type, scope_id, period_start, period_end, method_version,
                evidence_json, explanation, accuracy, created_at
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
               scope_type = excluded.scope_type,
               scope_id = excluded.scope_id,
               period_start = excluded.period_start,
               period_end = excluded.period_end,
               method_version = excluded.method_version,
               evidence_json = excluded.evidence_json,
               explanation = excluded.explanation,
               accuracy = excluded.accuracy,
               created_at = excluded.created_at",
            params![
                "burn-analysis-fixture",
                "account",
                Option::<String>::None,
                FIXTURE_TURN_STARTED,
                FIXTURE_OCCURRED_AT,
                "fixture-v1",
                serde_json::to_string(&evidence)?,
                "Fixture analysis seeded from deterministic normalized records.",
                accuracy_str(Accuracy::Estimated),
                FIXTURE_OCCURRED_AT
            ],
        )?;
        Ok(())
    }

    fn upsert_fixture_collector_state(tx: &Transaction<'_>) -> Result<(), DbError> {
        tx.execute(
            "INSERT INTO collector_sessions(id, started_at, ended_at, health, restart_count)
             VALUES(?1, ?2, NULL, ?3, 0)
             ON CONFLICT(id) DO UPDATE SET
               started_at = excluded.started_at,
               ended_at = excluded.ended_at,
               health = excluded.health,
               restart_count = excluded.restart_count",
            params![
                "fixture-session",
                FIXTURE_TURN_STARTED,
                "fixture data loaded"
            ],
        )?;
        Self::set_metadata_json(tx, "collector_state", &"fixture data loaded")?;
        Ok(())
    }

    fn codex_version(&self) -> Result<String, DbError> {
        Ok(self
            .scalar_optional_string_with_params(
                "SELECT version FROM codex_installations ORDER BY detected_at DESC LIMIT 1",
                [],
            )?
            .unwrap_or_else(|| DEFAULT_CODEX_VERSION.to_string()))
    }

    fn demo_mode(&self) -> Result<bool, DbError> {
        self.metadata_value("demo_mode", false)
    }

    fn collector_state(&self) -> Result<String, DbError> {
        if let Some(state) = self.scalar_optional_string_with_params(
            "SELECT health FROM collector_sessions ORDER BY started_at DESC LIMIT 1",
            [],
        )? {
            return Ok(state);
        }
        if let Some(state) = self.scalar_optional_string_with_params(
            "SELECT value FROM app_metadata WHERE key = 'collector_state' LIMIT 1",
            [],
        )? {
            return Ok(serde_json::from_str::<String>(&state).unwrap_or(state));
        }
        Ok("ready - no telemetry received".to_string())
    }

    fn last_event_at(&self) -> Result<Option<String>, DbError> {
        self.scalar_optional_string_with_params(
            "SELECT MAX(value) FROM (
                SELECT last_event_at AS value FROM sources
                UNION ALL
                SELECT observed_at AS value FROM usage_records
                UNION ALL
                SELECT observed_at AS value FROM quota_snapshots
                UNION ALL
                SELECT observed_at AS value FROM account_usage_snapshots
                UNION ALL
                SELECT observed_at AS value FROM account_usage_daily
                UNION ALL
                SELECT occurred_at AS value FROM hook_events
                UNION ALL
                SELECT occurred_at AS value FROM otel_events
                UNION ALL
                SELECT occurred_at AS value FROM collection_errors
             )",
            [],
        )
    }

    fn quota_windows(&self) -> Result<Vec<QuotaWindow>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT
                qb.id,
                qb.external_limit_id,
                qb.name,
                qb.window_kind,
                qb.window_duration_minutes,
                qs.used_percent,
                qs.remaining_percent,
                qs.resets_at,
                qs.resets_at_raw,
                qs.reset_interpretation,
                qs.accuracy
             FROM quota_buckets qb
             JOIN quota_snapshots qs
               ON qs.id = (
                   SELECT id
                   FROM quota_snapshots
                   WHERE bucket_id = qb.id
                   ORDER BY observed_at DESC
                   LIMIT 1
               )
             ORDER BY qb.id",
        )?;
        let rows = stmt.query_map([], |row| {
            let window_kind = row.get::<_, String>(3)?;
            let window_duration_minutes = row.get::<_, Option<i64>>(4)?;
            Ok(QuotaWindow {
                id: row.get(0)?,
                source_limit_id: row.get(1)?,
                name: row.get(2)?,
                window_kind: window_kind.clone(),
                window_duration_minutes,
                window_label: window_label(window_kind.as_str(), window_duration_minutes),
                used_percent: row.get(5)?,
                remaining_percent: row.get(6)?,
                resets_at: row.get(7)?,
                resets_at_raw: row.get(8)?,
                reset_interpretation: row.get(9)?,
                accuracy: parse_accuracy(row.get::<_, String>(10)?.as_str())?,
            })
        })?;
        collect_rows(rows)
    }

    fn quota_forecasts(&self) -> Result<Vec<QuotaForecast>, DbError> {
        let windows = self.quota_windows()?;
        let mut forecasts = Vec::with_capacity(windows.len());
        for window in windows {
            let reset_window_id = window
                .resets_at_raw
                .map(|raw| format!("raw:{raw}"))
                .or_else(|| {
                    window
                        .resets_at
                        .as_ref()
                        .map(|value| format!("iso:{value}"))
                })
                .unwrap_or_else(|| "reset:unavailable".to_string());
            let reset_identity = window
                .resets_at_raw
                .map(|raw| raw.to_string())
                .or_else(|| window.resets_at.clone())
                .unwrap_or_else(|| "none".to_string());
            let mut statement = self.conn.prepare(
                "SELECT observed_at, used_percent, accuracy
                 FROM quota_snapshots
                 WHERE bucket_id = ?1
                   AND COALESCE(CAST(resets_at_raw AS TEXT), resets_at, 'none') = ?2
                   AND used_percent IS NOT NULL
                 ORDER BY observed_at ASC",
            )?;
            let rows = statement.query_map(params![window.id.as_str(), reset_identity], |row| {
                Ok(QuotaObservation {
                    observed_at: row.get(0)?,
                    used_percent: row.get(1)?,
                    accuracy: parse_accuracy(row.get::<_, String>(2)?.as_str())?,
                })
            })?;
            let observations = collect_rows(rows)?;
            let resets_at = window
                .resets_at
                .as_deref()
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                .map(|value| value.with_timezone(&Utc));
            let forecast = calculate_forecast(ForecastInput {
                bucket_id: window.id.clone(),
                bucket_name: window.name.clone(),
                window_label: window.window_label.clone(),
                generated_at: Utc::now(),
                resets_at,
                observations,
            });
            if !self.demo_mode()? {
                self.persist_forecast(&forecast, &reset_window_id)?;
            }
            forecasts.push(forecast);
        }
        forecasts.sort_by(|left, right| {
            forecast_priority(right)
                .partial_cmp(&forecast_priority(left))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(forecasts)
    }

    fn persist_forecast(
        &self,
        forecast: &QuotaForecast,
        reset_window_id: &str,
    ) -> Result<(), DbError> {
        let latest_observation = forecast
            .status
            .last_observed_at
            .as_deref()
            .unwrap_or(forecast.generated_at.as_str());
        let id = format!(
            "{}|{}|{}",
            forecast.bucket_id, reset_window_id, latest_observation
        );
        self.conn.execute(
            "INSERT INTO quota_forecast_history(
                id, bucket_id, reset_window_id, generated_at, forecast_model,
                predicted_exhaustion_at, projected_usage_at_reset, safe_rate_pph,
                selected_burn_rate_pph, pace_ratio, confidence, sample_count,
                coverage_duration_sec, largest_gap_sec, invalidation_reason,
                exhaustion_before_reset
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT(id) DO UPDATE SET
               generated_at = excluded.generated_at,
               forecast_model = excluded.forecast_model,
               predicted_exhaustion_at = excluded.predicted_exhaustion_at,
               projected_usage_at_reset = excluded.projected_usage_at_reset,
               safe_rate_pph = excluded.safe_rate_pph,
               selected_burn_rate_pph = excluded.selected_burn_rate_pph,
               pace_ratio = excluded.pace_ratio,
               confidence = excluded.confidence,
               sample_count = excluded.sample_count,
               coverage_duration_sec = excluded.coverage_duration_sec,
               largest_gap_sec = excluded.largest_gap_sec,
               invalidation_reason = excluded.invalidation_reason,
               exhaustion_before_reset = excluded.exhaustion_before_reset",
            params![
                id,
                forecast.bucket_id,
                reset_window_id,
                forecast.generated_at,
                forecast_model_str(forecast.quality.selected_model),
                forecast.predicted_exhaustion_at,
                forecast.projected_usage_at_reset,
                forecast.safe_rate_pph,
                forecast.selected_rate_pph,
                forecast.pace_ratio,
                forecast_confidence_str(forecast.quality.confidence),
                forecast.quality.observation_count as i64,
                (forecast.quality.coverage_duration_minutes * 60.0).round() as i64,
                (forecast.quality.largest_gap_minutes * 60.0).round() as i64,
                forecast.quality.invalidation_reason,
                forecast.exhaustion_before_reset,
            ],
        )?;
        self.sync_forecast_alerts(forecast, reset_window_id)?;
        Ok(())
    }

    fn evaluate_completed_forecasts(&self) -> Result<(), DbError> {
        let pending = {
            let mut statement = self.conn.prepare(
                "SELECT id, bucket_id, reset_window_id, generated_at,
                        predicted_exhaustion_at, projected_usage_at_reset,
                        exhaustion_before_reset
                 FROM quota_forecast_history
                 WHERE evaluated_at IS NULL AND reset_window_id LIKE 'raw:%'",
            )?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<f64>>(5)?,
                        row.get::<_, Option<bool>>(6)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        for (
            id,
            bucket_id,
            reset_window_id,
            generated_at,
            predicted_exhaustion_at,
            projected_usage_at_reset,
            exhaustion_before_reset,
        ) in pending
        {
            let Some(raw_reset) = reset_window_id
                .strip_prefix("raw:")
                .and_then(|value| value.parse::<i64>().ok())
            else {
                continue;
            };
            let normalized_reset = normalize_reset_timestamp(Some(raw_reset));
            let Some(reset_at) = normalized_reset.normalized else {
                continue;
            };
            let outcome_crossed = self.conn.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM quota_snapshots
                    WHERE bucket_id = ?1
                      AND COALESCE(resets_at_raw, -1) <> ?2
                      AND observed_at >= ?3
                 )",
                params![bucket_id, raw_reset, reset_at],
                |row| row.get::<_, bool>(0),
            )?;
            if !outcome_crossed {
                continue;
            }
            let final_observation = self
                .conn
                .query_row(
                    "SELECT observed_at, used_percent
                     FROM quota_snapshots
                     WHERE bucket_id = ?1 AND resets_at_raw = ?2 AND used_percent IS NOT NULL
                     ORDER BY observed_at DESC
                     LIMIT 1",
                    params![bucket_id, raw_reset],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?)),
                )
                .optional()?;
            let Some((final_observed_at, final_usage)) = final_observation else {
                continue;
            };
            let exhaustion_observed_at = self
                .conn
                .query_row(
                    "SELECT observed_at
                     FROM quota_snapshots
                     WHERE bucket_id = ?1 AND resets_at_raw = ?2 AND used_percent >= 100
                     ORDER BY observed_at ASC
                     LIMIT 1",
                    params![bucket_id, raw_reset],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            let observed_exhaustion = exhaustion_observed_at.is_some();
            let forecast_error_minutes = match (
                predicted_exhaustion_at.as_deref(),
                exhaustion_observed_at.as_deref(),
            ) {
                (Some(predicted), Some(observed)) => {
                    let predicted = chrono::DateTime::parse_from_rfc3339(predicted)?;
                    let observed = chrono::DateTime::parse_from_rfc3339(observed)?;
                    Some((predicted - observed).num_seconds() as f64 / 60.0)
                }
                _ => None,
            };
            let outcome_at = exhaustion_observed_at
                .as_deref()
                .unwrap_or(&final_observed_at);
            let generated = chrono::DateTime::parse_from_rfc3339(&generated_at)?;
            let outcome = chrono::DateTime::parse_from_rfc3339(outcome_at)?;
            let lead_time_minutes = Some((outcome - generated).num_seconds() as f64 / 60.0);
            let projected_usage_error =
                projected_usage_at_reset.map(|projected| projected - final_usage);
            let classification_correct =
                exhaustion_before_reset.map(|predicted| predicted == observed_exhaustion);
            self.conn.execute(
                "UPDATE quota_forecast_history
                 SET observed_final_usage = ?2,
                     evaluated_at = ?3,
                     forecast_error_minutes = ?4,
                     forecast_lead_time_minutes = ?5,
                     projected_usage_error = ?6,
                     classification_correct = ?7
                 WHERE id = ?1",
                params![
                    id,
                    final_usage,
                    now_rfc3339(),
                    forecast_error_minutes,
                    lead_time_minutes,
                    projected_usage_error,
                    classification_correct,
                ],
            )?;
        }
        Ok(())
    }

    fn account_usage_summary(&self) -> Result<Option<AccountUsageSummary>, DbError> {
        let latest = self
            .conn
            .query_row(
                "SELECT
                    source_id,
                    observed_at,
                    lifetime_tokens,
                    peak_daily_tokens,
                    longest_running_turn_sec,
                    current_streak_days,
                    longest_streak_days,
                    accuracy
                 FROM account_usage_snapshots
                 ORDER BY observed_at DESC
                 LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                        row.get::<_, Option<i64>>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            source_id,
            observed_at,
            lifetime_tokens,
            peak_daily_tokens,
            longest_running_turn_sec,
            current_streak_days,
            longest_streak_days,
            accuracy,
        )) = latest
        else {
            return Ok(None);
        };

        let mut stmt = self.conn.prepare(
            "SELECT start_date, tokens, observed_at, accuracy
             FROM account_usage_daily
             WHERE source_id = ?1
             ORDER BY start_date DESC",
        )?;
        let rows = stmt.query_map(params![source_id], |row| {
            Ok(AccountUsageDailyBucket {
                start_date: row.get(0)?,
                tokens: row.get(1)?,
                observed_at: row.get(2)?,
                accuracy: parse_accuracy(row.get::<_, String>(3)?.as_str())?,
            })
        })?;

        Ok(Some(AccountUsageSummary {
            observed_at,
            lifetime_tokens,
            peak_daily_tokens,
            longest_running_turn_sec,
            current_streak_days,
            longest_streak_days,
            daily_usage_buckets: collect_rows(rows)?,
            accuracy: parse_accuracy(accuracy.as_str())?,
        }))
    }

    fn today_totals(&self) -> Result<TokenTotals, DbError> {
        let observed_day = self.scalar_optional_string_with_params(
            "SELECT substr(MAX(observed_at), 1, 10) FROM usage_records",
            [],
        )?;
        let Some(day) = observed_day else {
            return Ok(Dashboard::empty(self.codex_version()?).today);
        };

        self.conn
            .query_row(
                "SELECT
                SUM(input_tokens),
                SUM(cached_input_tokens),
                SUM(output_tokens),
                SUM(reasoning_output_tokens),
                SUM(total_tokens),
                GROUP_CONCAT(DISTINCT accuracy)
             FROM usage_records
             WHERE substr(observed_at, 1, 10) = ?1",
                params![day],
                |row| {
                    Ok(TokenTotals {
                        input: row.get(0)?,
                        cached_input: row.get(1)?,
                        output: row.get(2)?,
                        reasoning_output: row.get(3)?,
                        total: row.get(4)?,
                        accuracy: parse_accuracy_list(row.get::<_, Option<String>>(5)?.as_deref())?,
                    })
                },
            )
            .map_err(DbError::from)
    }

    fn expensive_turns(&self) -> Result<Vec<ExpensiveTurn>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT
                t.id,
                th.id,
                p.normalized_root,
                COALESCE(m.display_name, m.id),
                rs.effort,
                SUM(ur.total_tokens) AS total_tokens,
                t.duration_ms,
                GROUP_CONCAT(DISTINCT ur.accuracy || '|' || t.accuracy)
             FROM turns t
             JOIN threads th ON th.id = t.thread_id
             JOIN usage_records ur ON ur.turn_id = t.id
             LEFT JOIN projects p ON p.id = th.project_id
             LEFT JOIN models m ON m.id = t.model_id
             LEFT JOIN reasoning_settings rs ON rs.id = t.reasoning_setting_id
             WHERE ur.total_tokens IS NOT NULL
             GROUP BY t.id, th.id, p.normalized_root, m.display_name, m.id, rs.effort, t.duration_ms
             ORDER BY total_tokens DESC, t.completed_at DESC
             LIMIT 5",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ExpensiveTurn {
                id: row.get(0)?,
                thread_id: row.get(1)?,
                project: row.get(2)?,
                model: row.get(3)?,
                reasoning_effort: row.get(4)?,
                tokens: row.get(5)?,
                duration_ms: row.get(6)?,
                accuracy: parse_compound_accuracy(row.get::<_, Option<String>>(7)?.as_deref())?,
            })
        })?;
        collect_rows(rows)
    }

    fn burn_analysis(&self, turns: &[ExpensiveTurn]) -> Result<BurnAnalysis, DbError> {
        if turns.is_empty() {
            return Ok(Dashboard::empty(self.codex_version()?).burn);
        }

        if let Some((evidence_json, accuracy)) = self
            .conn
            .query_row(
                "SELECT evidence_json, accuracy
                 FROM burn_analyses
                 ORDER BY created_at DESC
                 LIMIT 1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
        {
            let evidence: Value = serde_json::from_str(&evidence_json)?;
            return Ok(BurnAnalysis {
                headline: evidence
                    .get("headline")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                factors: string_array(&evidence, "factors"),
                missing_signals: string_array(&evidence, "missingSignals")
                    .into_iter()
                    .map(|name| missing_signal_from_name(&name))
                    .collect(),
                method: evidence
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or("Ranks observed signals; no causal claim is made.")
                    .to_string(),
                confidence: parse_confidence(
                    evidence
                        .get("confidence")
                        .and_then(Value::as_str)
                        .unwrap_or("medium"),
                )?,
                accuracy: parse_accuracy(accuracy.as_str())?,
            });
        }

        let context = self.burn_context(&turns[0])?;
        let evidence = derive_burn_evidence(context);
        Ok(BurnAnalysis {
            headline: evidence.headline,
            factors: evidence.factors,
            missing_signals: evidence
                .missing_signals
                .into_iter()
                .map(|name| missing_signal_from_name(&name))
                .collect(),
            method: evidence.method,
            confidence: evidence.confidence,
            accuracy: Accuracy::Estimated,
        })
    }

    fn burn_context(&self, turn: &ExpensiveTurn) -> Result<BurnContext, DbError> {
        self.conn
            .query_row(
                "SELECT
                COALESCE(SUM(cached_input_tokens), 0),
                COALESCE(SUM(input_tokens), 0),
                COALESCE(MAX(compaction_count), 0),
                COALESCE(MAX(subagent_count), 0)
             FROM usage_records ur
             JOIN turns t ON t.id = ur.turn_id
             WHERE ur.turn_id = ?1",
                params![turn.id],
                |row| {
                    Ok(BurnContext {
                        turn: turn.clone(),
                        cached_input: row.get(0)?,
                        input: row.get(1)?,
                        compaction_count: row.get(2)?,
                        subagent_count: row.get(3)?,
                    })
                },
            )
            .map_err(DbError::from)
    }

    fn channel_health(&self) -> Result<Vec<ChannelHealth>, DbError> {
        let sources = self.sources()?;
        sources
            .into_iter()
            .map(|source| {
                let latest_error = self
                    .conn
                    .query_row(
                        "SELECT occurred_at, redacted_message
                         FROM collection_errors
                         WHERE source_id = ?1
                         ORDER BY occurred_at DESC, id DESC
                         LIMIT 1",
                        params![source.id.as_str()],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                    )
                    .optional()?
                    .map(|(occurred_at, message)| SourceError {
                        historical: source
                            .last_successful_collection_at
                            .as_ref()
                            .is_some_and(|success| success > &occurred_at),
                        occurred_at,
                        message,
                    });
                let restart_count = if source.id == "app-server" {
                    self.conn
                        .query_row(
                            "SELECT COALESCE(MAX(restart_count), 0) FROM collector_sessions",
                            [],
                            |row| row.get::<_, i64>(0),
                        )
                        .unwrap_or_default()
                } else {
                    0
                };
                let telemetry_state = if !source.enabled {
                    TelemetryState::Disabled
                } else {
                    match source.health {
                        ChannelStatus::Healthy
                            if source.last_successful_collection_at.is_some() =>
                        {
                            TelemetryState::Live
                        }
                        ChannelStatus::Healthy | ChannelStatus::Inactive => TelemetryState::Waiting,
                        ChannelStatus::Degraded | ChannelStatus::Unavailable => {
                            TelemetryState::Error
                        }
                    }
                };
                Ok(ChannelHealth {
                    id: source.id.clone(),
                    name: source_display_name(source.id.as_str()).to_string(),
                    status: source.health,
                    enabled: source.enabled,
                    healthy: source.health == ChannelStatus::Healthy,
                    detail: source.detail,
                    last_event_at: source.last_event_at,
                    last_successful_collection_at: source.last_successful_collection_at,
                    latest_error,
                    capabilities: source_capabilities(source.id.as_str()),
                    telemetry_state,
                    configuration_status: if source.enabled {
                        "Configured".to_string()
                    } else {
                        "Not configured".to_string()
                    },
                    restart_count,
                })
            })
            .collect()
    }

    fn account_signal_status(
        &self,
        table: &str,
        metric: &str,
        source_label: &str,
    ) -> Result<TelemetryStatus, DbError> {
        let last_observed_at = self.scalar_optional_string_with_params(
            &format!("SELECT MAX(observed_at) FROM {table}"),
            [],
        )?;
        if last_observed_at.is_some() {
            return Ok(TelemetryStatus {
                state: TelemetryState::Live,
                accuracy: Accuracy::ReportedExact,
                source: source_label.to_string(),
                last_observed_at,
                reason: None,
                required_integration: Some("Read-only App Server collection".to_string()),
            });
        }

        let capability = if table == "quota_snapshots" {
            "quota.used_percent"
        } else {
            "account.lifetime_tokens"
        };
        if self.capability_is_unavailable(capability)? {
            return Ok(telemetry_status(
                TelemetryState::Unsupported,
                source_label,
                format!("The installed Codex version does not expose {metric}."),
                "Install a Codex version that exposes this capability",
            ));
        }

        let settings = self.settings()?;
        let app_server = self
            .sources()?
            .into_iter()
            .find(|source| source.id == "app-server");
        if !settings.collection_enabled || app_server.as_ref().is_some_and(|source| !source.enabled)
        {
            return Ok(telemetry_status(
                TelemetryState::Disabled,
                source_label,
                format!("{metric} collection is disabled."),
                "Enable read-only App Server collection",
            ));
        }
        if app_server.as_ref().is_some_and(|source| {
            matches!(
                source.health,
                ChannelStatus::Degraded | ChannelStatus::Unavailable
            )
        }) {
            return Ok(telemetry_status(
                TelemetryState::Error,
                source_label,
                format!("{metric} collection was attempted and failed."),
                "Review Source Health and Diagnostics",
            ));
        }
        Ok(telemetry_status(
            TelemetryState::Waiting,
            source_label,
            format!("Waiting for the first {metric} response."),
            "Read-only App Server collection",
        ))
    }

    fn detailed_signal_status(
        &self,
        table: &str,
        metric: &str,
        accuracy: Accuracy,
    ) -> Result<TelemetryStatus, DbError> {
        let last_observed_at = self.scalar_optional_string_with_params(
            &format!("SELECT MAX(observed_at) FROM {table}"),
            [],
        )?;
        if last_observed_at.is_some() {
            return Ok(TelemetryStatus {
                state: TelemetryState::Live,
                accuracy,
                source: "Detailed turn telemetry".to_string(),
                last_observed_at,
                reason: None,
                required_integration: Some(
                    "Lifecycle hooks, OpenTelemetry, or a compatible App Server event".to_string(),
                ),
            });
        }

        let capability = if metric.contains("Models") {
            "turn.reasoning_effort"
        } else {
            "turn.total_tokens"
        };
        if self.capability_is_unavailable(capability)? {
            return Ok(telemetry_status(
                TelemetryState::Unsupported,
                "Installed Codex version",
                format!("The installed Codex version does not expose {metric}."),
                "Install a Codex version that exposes this capability",
            ));
        }

        let sources = self.sources()?;
        let details = sources
            .iter()
            .filter(|source| matches!(source.id.as_str(), "lifecycle-hooks" | "opentelemetry"))
            .collect::<Vec<_>>();
        if details.iter().all(|source| !source.enabled) {
            return Ok(telemetry_status(
                TelemetryState::Disabled,
                "Lifecycle hooks or OpenTelemetry",
                format!(
                    "{metric} is unavailable because detailed turn telemetry is not configured."
                ),
                "Enable lifecycle hooks or the local OpenTelemetry receiver",
            ));
        }
        if details.iter().any(|source| {
            source.enabled
                && matches!(
                    source.health,
                    ChannelStatus::Degraded | ChannelStatus::Unavailable
                )
        }) {
            return Ok(telemetry_status(
                TelemetryState::Error,
                "Lifecycle hooks or OpenTelemetry",
                format!("{metric} collection was attempted and failed."),
                "Review Source Health and Diagnostics",
            ));
        }
        Ok(telemetry_status(
            TelemetryState::Waiting,
            "Lifecycle hooks or OpenTelemetry",
            format!("Waiting for the first completed turn that supplies {metric}."),
            "Complete a turn after detailed telemetry is enabled",
        ))
    }

    fn burn_signal_status(&self, burn: &BurnAnalysis) -> Result<TelemetryStatus, DbError> {
        if burn.accuracy != Accuracy::Unavailable {
            return Ok(TelemetryStatus {
                state: TelemetryState::Live,
                accuracy: burn.accuracy,
                source: "Local deterministic analysis".to_string(),
                last_observed_at: self.scalar_optional_string_with_params(
                    "SELECT MAX(observed_at) FROM usage_records",
                    [],
                )?,
                reason: None,
                required_integration: Some(
                    "Token usage, completed turns, and quota snapshots".to_string(),
                ),
            });
        }
        self.detailed_signal_status(
            "usage_records",
            "Usage Burn evidence",
            Accuracy::Unavailable,
        )
    }

    fn capability_is_unavailable(&self, metric: &str) -> Result<bool, DbError> {
        Ok(self
            .conn
            .query_row(
                "SELECT availability FROM capabilities WHERE metric = ?1",
                params![metric],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .is_some_and(|availability| availability == "unavailable"))
    }

    fn alert_center(&self) -> Result<AlertCenter, DbError> {
        Ok(AlertCenter {
            active: self.alerts_query(
                "WHERE ah.dismissed_at IS NULL
                   AND ah.resolved_at IS NULL
                   AND EXISTS (
                     SELECT 1
                     FROM quota_snapshots qs
                     WHERE qs.bucket_id = ah.bucket_id
                       AND ('raw:' || qs.resets_at_raw) = ah.reset_window_id
                       AND qs.id = (
                         SELECT latest.id
                         FROM quota_snapshots latest
                         WHERE latest.bucket_id = ah.bucket_id
                         ORDER BY latest.observed_at DESC
                         LIMIT 1
                       )
                   )",
            )?,
            dismissed: self.alerts_query("WHERE ah.dismissed_at IS NOT NULL")?,
            history: self.alerts_query("")?,
        })
    }

    fn alerts_query(&self, filter: &str) -> Result<Vec<QuotaAlert>, DbError> {
        let sql = format!(
            "SELECT
                ah.id, ah.created_at, ah.collection_timestamp, ah.bucket_id, ah.bucket_name,
                ah.reset_window_id, ah.resets_at, ah.threshold, ah.used_percent,
                ah.remaining_percent, ah.severity, ah.accuracy, ah.dismissed_at,
                ah.resolved_at, ah.unread, ah.alert_type, ah.alert_source,
                ah.predicted_exhaustion_at, ah.forecast_confidence
             FROM alert_history ah
             {filter}
             ORDER BY ah.created_at DESC, ah.threshold DESC
             LIMIT 100"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            Ok(QuotaAlert {
                id: row.get(0)?,
                created_at: row.get(1)?,
                collection_timestamp: row.get(2)?,
                bucket_id: row.get(3)?,
                bucket_name: row.get(4)?,
                reset_window_id: row.get(5)?,
                resets_at: row.get(6)?,
                threshold: row.get(7)?,
                used_percent: row.get(8)?,
                remaining_percent: row.get(9)?,
                severity: parse_severity(row.get::<_, String>(10)?.as_str())?,
                accuracy: parse_accuracy(row.get::<_, String>(11)?.as_str())?,
                dismissed_at: row.get(12)?,
                resolved_at: row.get(13)?,
                unread: row.get(14)?,
                alert_type: row.get(15)?,
                alert_source: row.get(16)?,
                predicted_exhaustion_at: row.get(17)?,
                forecast_confidence: parse_forecast_confidence(
                    row.get::<_, Option<String>>(18)?.as_deref(),
                )?,
            })
        })?;
        collect_rows(rows)
    }

    fn sources(&self) -> Result<Vec<SourceStatus>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, channel, enabled, health, last_event_at, last_success_at, detail
             FROM sources
             ORDER BY CASE id
                 WHEN 'app-server' THEN 0
                 WHEN 'lifecycle-hooks' THEN 1
                 ELSE 2
             END, id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SourceStatus {
                id: row.get(0)?,
                channel: row.get(1)?,
                enabled: row.get::<_, i64>(2)? != 0,
                health: parse_status(row.get::<_, String>(3)?.as_str())?,
                last_event_at: row.get(4)?,
                last_successful_collection_at: row.get(5)?,
                detail: row
                    .get::<_, Option<String>>(6)?
                    .unwrap_or_else(|| "No detail available".to_string()),
            })
        })?;
        collect_rows(rows)
    }

    fn capabilities(&self) -> Result<Vec<CapabilityStatus>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT metric, source_channel, source_method, source_field, availability, accuracy, limitation, updated_at
             FROM capabilities
             ORDER BY metric",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CapabilityStatus {
                metric: row.get(0)?,
                source_channel: row.get(1)?,
                source_method: row.get(2)?,
                source_field: row.get(3)?,
                availability: row.get(4)?,
                accuracy: parse_accuracy(row.get::<_, String>(5)?.as_str())?,
                limitation: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?;
        collect_rows(rows)
    }

    fn collector_diagnostics(&self) -> Result<CollectorDiagnostics, DbError> {
        let mut session_stmt = self.conn.prepare(
            "SELECT id, started_at, ended_at, health, restart_count
             FROM collector_sessions
             ORDER BY started_at DESC
             LIMIT 5",
        )?;
        let recent_sessions = collect_rows(session_stmt.query_map([], |row| {
            Ok(CollectorSessionDiagnostic {
                id: row.get(0)?,
                started_at: row.get(1)?,
                ended_at: row.get(2)?,
                health: row.get(3)?,
                restart_count: row.get(4)?,
            })
        })?)?;

        let mut error_stmt = self.conn.prepare(
            "SELECT occurred_at, source_id, category, redacted_message, retryable
             FROM collection_errors
             ORDER BY occurred_at DESC, id DESC
             LIMIT 5",
        )?;
        let recent_errors = collect_rows(error_stmt.query_map([], |row| {
            Ok(CollectorErrorDiagnostic {
                occurred_at: row.get(0)?,
                source_id: row.get(1)?,
                category: row.get(2)?,
                redacted_message: row.get(3)?,
                retryable: row.get::<_, i64>(4)? != 0,
            })
        })?)?;

        Ok(CollectorDiagnostics {
            latest_session: recent_sessions.first().cloned(),
            recent_sessions,
            recent_errors,
        })
    }

    fn diagnostics_report(&self) -> Result<DiagnosticsReport, DbError> {
        Ok(DiagnosticsReport {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            schema_version: MIGRATION_VERSION,
            codex_version: self.codex_version()?,
            demo_mode: self.demo_mode()?,
            collector_state: self.collector_state()?,
            settings: self.settings()?,
            sources: self.sources()?,
            capabilities: self.capabilities()?,
            counts: self.table_counts()?,
            latest_timestamps: BTreeMap::from([
                ("lastEventAt".to_string(), self.last_event_at()?),
                (
                    "latestUsageAt".to_string(),
                    self.scalar_optional_string_with_params(
                        "SELECT MAX(observed_at) FROM usage_records",
                        [],
                    )?,
                ),
                (
                    "latestQuotaAt".to_string(),
                    self.scalar_optional_string_with_params(
                        "SELECT MAX(observed_at) FROM quota_snapshots",
                        [],
                    )?,
                ),
                (
                    "latestAccountUsageAt".to_string(),
                    self.scalar_optional_string_with_params(
                        "SELECT MAX(observed_at) FROM account_usage_snapshots",
                        [],
                    )?,
                ),
                (
                    "latestDailyAccountUsageAt".to_string(),
                    self.scalar_optional_string_with_params(
                        "SELECT MAX(observed_at) FROM account_usage_daily",
                        [],
                    )?,
                ),
                (
                    "latestCollectorSessionAt".to_string(),
                    self.scalar_optional_string_with_params(
                        "SELECT MAX(started_at) FROM collector_sessions",
                        [],
                    )?,
                ),
                (
                    "latestCollectionErrorAt".to_string(),
                    self.scalar_optional_string_with_params(
                        "SELECT MAX(occurred_at) FROM collection_errors",
                        [],
                    )?,
                ),
            ]),
            collector_diagnostics: self.collector_diagnostics()?,
        })
    }

    fn distribution_by_model(&self) -> Result<Vec<DistributionPoint>, DbError> {
        self.distribution_query(
            "SELECT COALESCE(m.display_name, m.id, 'Unavailable'), COALESCE(SUM(ur.total_tokens), 0)
             FROM usage_records ur
             LEFT JOIN models m ON m.id = ur.model_id
             GROUP BY COALESCE(m.display_name, m.id, 'Unavailable')
             ORDER BY SUM(ur.total_tokens) DESC, 1 ASC",
        )
    }

    fn distribution_by_reasoning(&self) -> Result<Vec<DistributionPoint>, DbError> {
        self.distribution_query(
            "SELECT COALESCE(rs.effort, 'unavailable'), COALESCE(SUM(ur.total_tokens), 0)
             FROM usage_records ur
             LEFT JOIN turns t ON t.id = ur.turn_id
             LEFT JOIN reasoning_settings rs ON rs.id = t.reasoning_setting_id
             GROUP BY COALESCE(rs.effort, 'unavailable')
             ORDER BY SUM(ur.total_tokens) DESC, 1 ASC",
        )
    }

    fn distribution_query(&self, sql: &str) -> Result<Vec<DistributionPoint>, DbError> {
        let mut stmt = self.conn.prepare(sql)?;
        let mut raw = Vec::new();
        let mut total = 0i64;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (label, tokens) = row?;
            total += tokens;
            raw.push((label, tokens));
        }
        if total <= 0 {
            return Ok(Vec::new());
        }

        Ok(raw
            .into_iter()
            .map(|(label, tokens)| DistributionPoint {
                label,
                value: ((tokens as f64 / total as f64) * 100.0).round() as i64,
            })
            .collect())
    }

    fn dashboard_warnings(&self, dashboard: &Dashboard) -> Vec<String> {
        let mut warnings = Vec::new();
        if dashboard.demo_mode {
            warnings.push(
                "Fixture/demo mode: values below are synthetic and do not describe your account."
                    .to_string(),
            );
        }

        if dashboard.turns.is_empty() && dashboard.quota.is_empty() {
            warnings.push(
                "No telemetry is available. Load the synthetic fixture to explore the interface."
                    .to_string(),
            );
        } else {
            warnings.push(
                "Project quota attribution is estimated because quota snapshots are account-level."
                    .to_string(),
            );
        }

        warnings
    }

    fn prune_by_retention(&self) -> Result<(), DbError> {
        let settings = self.settings()?;
        let retention_days = settings.retention_days.max(1);
        let cutoff = (Utc::now() - ChronoDuration::days(retention_days)).to_rfc3339();
        let tx = self.conn.unchecked_transaction()?;

        for (table, column) in [
            ("hook_events", "occurred_at"),
            ("otel_events", "occurred_at"),
            ("usage_records", "observed_at"),
            ("quota_snapshots", "observed_at"),
            ("account_usage_snapshots", "observed_at"),
            ("account_usage_daily", "observed_at"),
            ("burn_analyses", "created_at"),
            ("quota_forecast_history", "generated_at"),
            ("collection_errors", "occurred_at"),
            ("alert_history", "created_at"),
            ("collector_sessions", "started_at"),
            ("exports", "created_at"),
            ("projects", "updated_at"),
            ("worktrees", "last_seen_at"),
            ("threads", "updated_at"),
        ] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE {column} < ?1"),
                params![cutoff],
            )?;
        }

        tx.execute(
            "DELETE FROM turns
             WHERE COALESCE(completed_at, started_at, '') < ?1",
            params![cutoff],
        )?;
        tx.execute(
            "DELETE FROM models
             WHERE last_seen_at < ?1",
            params![cutoff],
        )?;
        tx.execute(
            "DELETE FROM codex_installations
             WHERE detected_at < ?1",
            params![cutoff],
        )?;
        tx.commit()?;
        Ok(())
    }

    fn table_counts(&self) -> Result<BTreeMap<String, i64>, DbError> {
        let mut counts = BTreeMap::new();
        for table in [
            "sources",
            "capabilities",
            "collector_sessions",
            "projects",
            "threads",
            "turns",
            "usage_records",
            "quota_buckets",
            "quota_snapshots",
            "account_usage_snapshots",
            "account_usage_daily",
            "burn_analyses",
            "collection_errors",
            "alert_history",
            "exports",
        ] {
            let count =
                self.conn
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })?;
            counts.insert(table.to_string(), count);
        }
        Ok(counts)
    }

    fn ensure_source_tx(tx: &Transaction<'_>, source: &SourceIdentity) -> Result<(), DbError> {
        tx.execute(
            "INSERT INTO sources(id, channel, enabled, health, last_event_at, detail)
             VALUES(?1, ?2, 1, ?3, NULL, ?4)
             ON CONFLICT(id) DO UPDATE SET
               channel = excluded.channel,
               enabled = excluded.enabled",
            params![
                source.id,
                source.channel,
                status_str(ChannelStatus::Inactive),
                "Dynamically discovered source"
            ],
        )?;
        Ok(())
    }

    fn update_source_tx(
        tx: &Transaction<'_>,
        source_id: &str,
        status: ChannelStatus,
        observed_at: &str,
        detail: &str,
    ) -> Result<(), DbError> {
        tx.execute(
            "UPDATE sources
             SET health = ?2,
                 last_event_at = ?3,
                 detail = ?4,
                 last_success_at = CASE WHEN ?2 = 'healthy' THEN ?3 ELSE last_success_at END
             WHERE id = ?1",
            params![
                source_id,
                status_str(status),
                observed_at,
                redact_diagnostic(detail)
            ],
        )?;
        Ok(())
    }

    fn ensure_thread_turn_stub_tx(
        tx: &Transaction<'_>,
        source: &SourceIdentity,
        thread_id: &str,
        turn_id: &str,
        observed_at: &str,
    ) -> Result<(), DbError> {
        tx.execute(
            "INSERT INTO threads(
                id, source_id, project_id, worktree_id, title, created_at, updated_at, status, accuracy
             )
             VALUES(?1, ?2, NULL, NULL, NULL, NULL, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               source_id = COALESCE(threads.source_id, excluded.source_id),
               updated_at = excluded.updated_at,
               status = COALESCE(threads.status, excluded.status),
               accuracy = excluded.accuracy",
            params![
                thread_id,
                source.id,
                observed_at,
                "observed",
                accuracy_str(Accuracy::ReportedExact)
            ],
        )?;
        tx.execute(
            "INSERT INTO turns(
                id, thread_id, model_id, reasoning_setting_id, started_at, completed_at,
                duration_ms, status, tool_count, compaction_count, subagent_count, retry_count, accuracy
             )
             VALUES(?1, ?2, NULL, NULL, NULL, NULL, NULL, ?3, 0, 0, 0, 0, ?4)
             ON CONFLICT(id) DO UPDATE SET
               thread_id = excluded.thread_id,
               status = COALESCE(turns.status, excluded.status),
               accuracy = excluded.accuracy",
            params![
                turn_id,
                thread_id,
                "observed",
                accuracy_str(Accuracy::ReportedExact)
            ],
        )?;
        Ok(())
    }

    fn existing_bucket_tx(
        tx: &Transaction<'_>,
        bucket_id: &str,
    ) -> Result<Option<ExistingBucket>, DbError> {
        tx.query_row(
            "SELECT name, window_duration_minutes
             FROM quota_buckets
             WHERE id = ?1",
            params![bucket_id],
            |row| {
                Ok(ExistingBucket {
                    name: row.get(0)?,
                    duration: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(DbError::from)
    }

    fn quota_snapshot_is_duplicate_tx(
        tx: &Transaction<'_>,
        bucket_id: &str,
        candidate: &QuotaSnapshotCandidate<'_>,
    ) -> Result<bool, DbError> {
        tx.query_row(
            "SELECT
                used_percent,
                remaining_percent,
                resets_at,
                resets_at_raw,
                reset_interpretation,
                accuracy
             FROM quota_snapshots
             WHERE bucket_id = ?1
             ORDER BY observed_at DESC
             LIMIT 1",
            params![bucket_id],
            |row| {
                Ok((
                    row.get::<_, Option<f64>>(0)?,
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()
        .map_err(DbError::from)
        .map(|existing| {
            existing.is_some_and(
                |(
                    existing_used,
                    existing_remaining,
                    existing_resets_at,
                    existing_resets_at_raw,
                    existing_reset_interpretation,
                    existing_accuracy,
                )| {
                    existing_used == candidate.used_percent
                        && existing_remaining == candidate.remaining_percent
                        && existing_resets_at.as_deref() == candidate.resets_at
                        && existing_resets_at_raw == candidate.resets_at_raw
                        && existing_reset_interpretation == candidate.reset_interpretation
                        && existing_accuracy == accuracy_str(candidate.accuracy)
                },
            )
        })
    }

    fn metadata_value<T>(&self, key: &str, default: T) -> Result<T, DbError>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        match self.scalar_optional_string_with_params(
            "SELECT value FROM app_metadata WHERE key = ?1",
            params![key],
        )? {
            Some(raw) => Ok(serde_json::from_str(&raw)?),
            None => Ok(default),
        }
    }

    fn setting_value<T>(&self, key: &str, default: T) -> Result<T, DbError>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        match self.scalar_optional_string_with_params(
            "SELECT value_json FROM settings WHERE key = ?1",
            params![key],
        )? {
            Some(raw) => Ok(serde_json::from_str(&raw)?),
            None => Ok(default),
        }
    }

    fn scalar_optional_string_with_params<P>(
        &self,
        sql: &str,
        params: P,
    ) -> Result<Option<String>, DbError>
    where
        P: rusqlite::Params,
    {
        self.conn
            .query_row(sql, params, |row| row.get::<_, Option<String>>(0))
            .optional()
            .map(|value| value.flatten())
            .map_err(DbError::from)
    }
}

#[derive(Debug, Clone, Copy)]
struct CapabilitySeed {
    metric: &'static str,
    source_channel: &'static str,
    source_method: Option<&'static str>,
    source_field: Option<&'static str>,
    availability: &'static str,
    accuracy: Accuracy,
    limitation: Option<&'static str>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct SourceIdentity {
    id: String,
    channel: String,
}

#[allow(dead_code)]
impl SourceIdentity {
    fn from_input(input: &str) -> Self {
        let trimmed = input.trim();
        match trimmed {
            "app_server" | "app-server" => Self {
                id: "app-server".to_string(),
                channel: "app_server".to_string(),
            },
            "lifecycle_hooks" | "lifecycle-hooks" => Self {
                id: "lifecycle-hooks".to_string(),
                channel: "lifecycle_hooks".to_string(),
            },
            "opentelemetry" => Self {
                id: "opentelemetry".to_string(),
                channel: "opentelemetry".to_string(),
            },
            other => Self {
                id: other.replace('_', "-"),
                channel: other.replace('-', "_"),
            },
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct ExistingBucket {
    name: String,
    duration: Option<i64>,
}

struct QuotaSnapshotCandidate<'a> {
    used_percent: Option<f64>,
    remaining_percent: Option<f64>,
    resets_at: Option<&'a str>,
    resets_at_raw: Option<i64>,
    reset_interpretation: &'a str,
    accuracy: Accuracy,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct ParsedRateLimitSnapshot {
    storage_key: String,
    limit_id: Option<String>,
    limit_name: Option<String>,
    value: Value,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct NormalizedReset {
    normalized: Option<String>,
    interpretation: String,
}

#[allow(dead_code)]
fn rate_limit_snapshots(
    result_or_notification: &Value,
) -> Result<Vec<ParsedRateLimitSnapshot>, DbError> {
    let mut snapshots = Vec::new();
    if let Some(map) = result_or_notification
        .get("rateLimitsByLimitId")
        .and_then(Value::as_object)
    {
        for (key, value) in map {
            if !value.is_object() {
                continue;
            }
            snapshots.push(ParsedRateLimitSnapshot {
                storage_key: key.clone(),
                limit_id: optional_string(value.get("limitId")).or_else(|| Some(key.clone())),
                limit_name: optional_string(value.get("limitName")),
                value: value.clone(),
            });
        }
    }

    if !snapshots.is_empty() {
        return Ok(snapshots);
    }

    let Some(single) = result_or_notification.get("rateLimits") else {
        return Err(DbError::InvalidData(
            "rate limit payload missing rateLimitsByLimitId and rateLimits".to_string(),
        ));
    };
    if !single.is_object() {
        return Err(DbError::InvalidData(
            "rate limit payload rateLimits was not an object".to_string(),
        ));
    }

    snapshots.push(ParsedRateLimitSnapshot {
        storage_key: optional_string(single.get("limitId"))
            .unwrap_or_else(|| "default".to_string()),
        limit_id: optional_string(single.get("limitId")),
        limit_name: optional_string(single.get("limitName")),
        value: single.clone(),
    });
    Ok(snapshots)
}

#[allow(dead_code)]
fn optional_string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(ToString::to_string)
}

#[allow(dead_code)]
fn nullable_i64(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    if let Some(parsed) = value.as_i64() {
        return Some(parsed);
    }
    if let Some(parsed) = value.as_u64() {
        return i64::try_from(parsed).ok();
    }
    value.as_str().and_then(|raw| raw.parse::<i64>().ok())
}

#[allow(dead_code)]
fn required_i64(value: Option<&Value>, field: &str) -> Result<i64, DbError> {
    nullable_i64(value)
        .ok_or_else(|| DbError::InvalidData(format!("missing or invalid integer field: {field}")))
}

#[allow(dead_code)]
fn exact_percent(value: Option<&Value>) -> Option<f64> {
    let raw = nullable_i64(value)? as f64;
    if (0.0..=100.0).contains(&raw) {
        Some(raw)
    } else {
        None
    }
}

#[allow(dead_code)]
fn quota_bucket_id(source_id: &str, limit_key: &str, window_kind: &str) -> String {
    format!(
        "quota:{}:{}:{}",
        stable_identifier_fragment(source_id),
        stable_identifier_fragment(limit_key),
        stable_identifier_fragment(window_kind)
    )
}

#[allow(dead_code)]
fn stable_identifier_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

#[allow(dead_code)]
fn normalize_reset_timestamp(raw: Option<i64>) -> NormalizedReset {
    const PLAUSIBLE_SECONDS_MIN: i64 = 946_684_800;
    const PLAUSIBLE_SECONDS_MAX: i64 = 4_102_444_800;
    const PLAUSIBLE_MILLIS_MIN: i64 = PLAUSIBLE_SECONDS_MIN * 1_000;
    const PLAUSIBLE_MILLIS_MAX: i64 = PLAUSIBLE_SECONDS_MAX * 1_000;

    match raw {
        None => NormalizedReset {
            normalized: None,
            interpretation: "unavailable".to_string(),
        },
        Some(value) if (PLAUSIBLE_SECONDS_MIN..=PLAUSIBLE_SECONDS_MAX).contains(&value) => {
            match chrono::DateTime::<Utc>::from_timestamp(value, 0) {
                Some(date_time) => NormalizedReset {
                    normalized: Some(date_time.to_rfc3339()),
                    interpretation: "unix_seconds".to_string(),
                },
                None => NormalizedReset {
                    normalized: None,
                    interpretation: "ambiguous".to_string(),
                },
            }
        }
        Some(value) if (PLAUSIBLE_MILLIS_MIN..=PLAUSIBLE_MILLIS_MAX).contains(&value) => {
            let seconds = value.div_euclid(1_000);
            let nanos = (value.rem_euclid(1_000) as u32) * 1_000_000;
            match chrono::DateTime::<Utc>::from_timestamp(seconds, nanos) {
                Some(date_time) => NormalizedReset {
                    normalized: Some(date_time.to_rfc3339()),
                    interpretation: "unix_milliseconds".to_string(),
                },
                None => NormalizedReset {
                    normalized: None,
                    interpretation: "ambiguous".to_string(),
                },
            }
        }
        Some(_) => NormalizedReset {
            normalized: None,
            interpretation: "ambiguous".to_string(),
        },
    }
}

fn apply_migration(
    tx: &Transaction<'_>,
    version: i64,
    name: &str,
    sql: &str,
) -> Result<(), DbError> {
    let migration_table_exists = tx.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_master
            WHERE type = 'table' AND name = 'schema_migrations'
         )",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if migration_table_exists {
        let already_applied = tx.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM schema_migrations WHERE version = ?1
             )",
            params![version],
            |row| row.get::<_, bool>(0),
        )?;
        if already_applied {
            return Ok(());
        }
    }

    tx.execute_batch(sql)?;
    tx.execute(
        "INSERT INTO schema_migrations(version, name, applied_at)
         VALUES(?1, ?2, ?3)
         ON CONFLICT(version) DO NOTHING",
        params![version, name, now_rfc3339()],
    )?;
    Ok(())
}

fn default_capabilities() -> Vec<CapabilitySeed> {
    vec![
        CapabilitySeed {
            metric: "quota.used_percent",
            source_channel: "app_server",
            source_method: Some("account/rateLimits/read"),
            source_field: Some("usedPercent"),
            availability: "available",
            accuracy: Accuracy::ReportedExact,
            limitation: Some("Capacity units are not reported by the current Codex interface."),
        },
        CapabilitySeed {
            metric: "quota.remaining_percent",
            source_channel: "app_server",
            source_method: Some("account/rateLimits/read"),
            source_field: Some("usedPercent"),
            availability: "available",
            accuracy: Accuracy::DerivedExact,
            limitation: Some("Derived as 100 minus the reported used percentage."),
        },
        CapabilitySeed {
            metric: "turn.total_tokens",
            source_channel: "opentelemetry",
            source_method: Some("thread/tokenUsage/updated"),
            source_field: Some("total_tokens"),
            availability: "available",
            accuracy: Accuracy::ReportedExact,
            limitation: None,
        },
        CapabilitySeed {
            metric: "turn.reasoning_effort",
            source_channel: "lifecycle_hooks",
            source_method: Some("turn.completed"),
            source_field: Some("reasoning.effort"),
            availability: "conditional",
            accuracy: Accuracy::ReportedExact,
            limitation: Some(
                "Only present when the verified source includes model configuration metadata.",
            ),
        },
        CapabilitySeed {
            metric: "project.quota_attribution",
            source_channel: "derived",
            source_method: None,
            source_field: None,
            availability: "conditional",
            accuracy: Accuracy::Estimated,
            limitation: Some(
                "Quota snapshots are account-level and cannot be attributed exactly to a project.",
            ),
        },
        CapabilitySeed {
            metric: "account.cost",
            source_channel: "app_server",
            source_method: Some("account/usage/read"),
            source_field: None,
            availability: "unavailable",
            accuracy: Accuracy::Unavailable,
            limitation: Some(
                "The installed Codex interfaces do not expose exact account cost data.",
            ),
        },
        CapabilitySeed {
            metric: "account.lifetime_tokens",
            source_channel: "app_server",
            source_method: Some("account/usage/read"),
            source_field: Some("summary.lifetimeTokens"),
            availability: "available",
            accuracy: Accuracy::ReportedExact,
            limitation: Some("Account-level only; not attributable exactly to a project."),
        },
        CapabilitySeed {
            metric: "account.daily_tokens",
            source_channel: "app_server",
            source_method: Some("account/usage/read"),
            source_field: Some("dailyUsageBuckets[].tokens"),
            availability: "available",
            accuracy: Accuracy::ReportedExact,
            limitation: Some("Daily totals are account-level snapshots."),
        },
    ]
}

fn parse_accuracy(value: &str) -> rusqlite::Result<Accuracy> {
    match value {
        "reported_exact" => Ok(Accuracy::ReportedExact),
        "derived_exact" => Ok(Accuracy::DerivedExact),
        "estimated" => Ok(Accuracy::Estimated),
        "unavailable" => Ok(Accuracy::Unavailable),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            Type::Text,
            Box::new(DbError::InvalidData(format!("unknown accuracy: {other}"))),
        )),
    }
}

fn parse_confidence(value: &str) -> Result<BurnConfidence, DbError> {
    match value {
        "high" => Ok(BurnConfidence::High),
        "medium" => Ok(BurnConfidence::Medium),
        "low" => Ok(BurnConfidence::Low),
        other => Err(DbError::InvalidData(format!("unknown confidence: {other}"))),
    }
}

fn forecast_model_str(value: ForecastModel) -> &'static str {
    match value {
        ForecastModel::RecentRate => "recent_rate",
        ForecastModel::OrdinaryLeastSquares => "ordinary_least_squares",
        ForecastModel::ExponentiallyWeightedMovingAverage => {
            "exponentially_weighted_moving_average"
        }
        ForecastModel::Unavailable => "unavailable",
    }
}

fn forecast_confidence_str(value: ForecastConfidence) -> &'static str {
    match value {
        ForecastConfidence::Unavailable => "unavailable",
        ForecastConfidence::Preliminary => "preliminary",
        ForecastConfidence::Low => "low",
        ForecastConfidence::Medium => "medium",
        ForecastConfidence::High => "high",
    }
}

fn parse_forecast_confidence(value: Option<&str>) -> rusqlite::Result<Option<ForecastConfidence>> {
    value
        .map(|value| match value {
            "unavailable" => Ok(ForecastConfidence::Unavailable),
            "preliminary" => Ok(ForecastConfidence::Preliminary),
            "low" => Ok(ForecastConfidence::Low),
            "medium" => Ok(ForecastConfidence::Medium),
            "high" => Ok(ForecastConfidence::High),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                Type::Text,
                Box::new(DbError::InvalidData(format!(
                    "unknown forecast confidence: {other}"
                ))),
            )),
        })
        .transpose()
}

fn forecast_priority(forecast: &QuotaForecast) -> f64 {
    let risk = match forecast.risk {
        ForecastRisk::ExhaustionLikely => 4.0,
        ForecastRisk::AtRisk => 3.0,
        ForecastRisk::Watch => 2.0,
        ForecastRisk::Healthy => 1.0,
        ForecastRisk::Unavailable => 0.0,
    };
    risk * 1_000.0 + forecast.current_used_percent.unwrap_or_default()
}

fn forecast_risk_str(value: ForecastRisk) -> &'static str {
    match value {
        ForecastRisk::Healthy => "healthy",
        ForecastRisk::Watch => "watch",
        ForecastRisk::AtRisk => "at_risk",
        ForecastRisk::ExhaustionLikely => "exhaustion_likely",
        ForecastRisk::Unavailable => "unavailable",
    }
}

fn parse_status(value: &str) -> rusqlite::Result<ChannelStatus> {
    match value {
        "healthy" => Ok(ChannelStatus::Healthy),
        "inactive" => Ok(ChannelStatus::Inactive),
        "degraded" => Ok(ChannelStatus::Degraded),
        "unavailable" => Ok(ChannelStatus::Unavailable),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            Type::Text,
            Box::new(DbError::InvalidData(format!(
                "unknown source status: {other}"
            ))),
        )),
    }
}

fn parse_accuracy_list(value: Option<&str>) -> rusqlite::Result<Accuracy> {
    parse_compound_accuracy(value)
}

fn parse_compound_accuracy(value: Option<&str>) -> rusqlite::Result<Accuracy> {
    let Some(raw) = value else {
        return Ok(Accuracy::Unavailable);
    };
    let mut best = Accuracy::ReportedExact;
    let mut saw_value = false;
    for chunk in raw.split(',').flat_map(|part| part.split('|')) {
        let trimmed = chunk.trim();
        if trimmed.is_empty() {
            continue;
        }
        saw_value = true;
        let parsed = match trimmed {
            "reported_exact" => Accuracy::ReportedExact,
            "derived_exact" => Accuracy::DerivedExact,
            "estimated" => Accuracy::Estimated,
            "unavailable" => Accuracy::Unavailable,
            other => {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    0,
                    Type::Text,
                    Box::new(DbError::InvalidData(format!(
                        "unknown aggregate accuracy: {other}"
                    ))),
                ))
            }
        };
        if accuracy_rank(parsed) > accuracy_rank(best) {
            best = parsed;
        }
    }
    if saw_value {
        Ok(best)
    } else {
        Ok(Accuracy::Unavailable)
    }
}

fn accuracy_rank(value: Accuracy) -> i32 {
    match value {
        Accuracy::ReportedExact => 0,
        Accuracy::DerivedExact => 1,
        Accuracy::Estimated => 2,
        Accuracy::Unavailable => 3,
    }
}

fn accuracy_str(value: Accuracy) -> &'static str {
    match value {
        Accuracy::ReportedExact => "reported_exact",
        Accuracy::DerivedExact => "derived_exact",
        Accuracy::Estimated => "estimated",
        Accuracy::Unavailable => "unavailable",
    }
}

fn status_str(value: ChannelStatus) -> &'static str {
    match value {
        ChannelStatus::Healthy => "healthy",
        ChannelStatus::Inactive => "inactive",
        ChannelStatus::Degraded => "degraded",
        ChannelStatus::Unavailable => "unavailable",
    }
}

fn window_label(window_kind: &str, duration_minutes: Option<i64>) -> String {
    match (window_kind, duration_minutes) {
        ("rolling", Some(minutes)) if minutes % 60 == 0 => format!("{} hour window", minutes / 60),
        ("rolling", Some(minutes)) => format!("{minutes} minute window"),
        ("weekly", _) => "Weekly window".to_string(),
        ("daily", _) => "Daily window".to_string(),
        ("monthly", _) => "Monthly window".to_string(),
        (kind, _) => kind.replace('_', " "),
    }
}

fn source_display_name(id: &str) -> &'static str {
    match id {
        "app-server" => "App Server",
        "lifecycle-hooks" => "Lifecycle hooks",
        "opentelemetry" => "OpenTelemetry",
        _ => "Unknown source",
    }
}

fn source_capabilities(id: &str) -> Vec<String> {
    let values: &[&str] = match id {
        "app-server" => &["Quota windows", "Account activity"],
        "lifecycle-hooks" => &["Turn completion", "Model and reasoning", "Attribution"],
        "opentelemetry" => &["Token composition", "Usage Burn evidence"],
        _ => &[],
    };
    values.iter().map(|value| (*value).to_string()).collect()
}

fn telemetry_status(
    state: TelemetryState,
    source: &str,
    reason: String,
    required_integration: &str,
) -> TelemetryStatus {
    TelemetryStatus {
        state,
        accuracy: Accuracy::Unavailable,
        source: source.to_string(),
        last_observed_at: None,
        reason: Some(reason),
        required_integration: Some(required_integration.to_string()),
    }
}

fn missing_signal_from_name(name: &str) -> MissingSignal {
    let (source, setup_state, detail) = match name {
        "Token usage" | "Completed turns" => (
            "Lifecycle hooks or OpenTelemetry",
            TelemetryState::Disabled,
            "Detailed turn telemetry is not configured.",
        ),
        "Quota snapshots" => (
            "Codex App Server",
            TelemetryState::Unavailable,
            "No reliable quota snapshot is available.",
        ),
        "Exact quota units" => (
            "Codex App Server",
            TelemetryState::Unsupported,
            "The installed interface reports percentages, not capacity units.",
        ),
        "Direct project-level quota attribution" => (
            "Codex App Server",
            TelemetryState::Unsupported,
            "Quota snapshots are account-scoped.",
        ),
        _ => (
            "Verified telemetry source",
            TelemetryState::Unavailable,
            "This signal is not reliably available.",
        ),
    };
    MissingSignal {
        name: name.to_string(),
        source: source.to_string(),
        setup_state,
        detail: detail.to_string(),
    }
}

fn alert_id(bucket_id: &str, reset_window_id: &str, threshold: i64) -> String {
    format!("{bucket_id}|{reset_window_id}|{threshold}")
}

fn alert_severity(used_percent: f64) -> AlertSeverity {
    if used_percent >= 100.0 {
        AlertSeverity::Exhausted
    } else if used_percent >= 90.0 {
        AlertSeverity::Critical
    } else if used_percent >= 75.0 {
        AlertSeverity::Warning
    } else if used_percent >= 50.0 {
        AlertSeverity::Informational
    } else {
        AlertSeverity::Neutral
    }
}

fn severity_str(severity: AlertSeverity) -> &'static str {
    match severity {
        AlertSeverity::Neutral => "neutral",
        AlertSeverity::Informational => "informational",
        AlertSeverity::Warning => "warning",
        AlertSeverity::Critical => "critical",
        AlertSeverity::Exhausted => "exhausted",
    }
}

fn parse_severity(value: &str) -> rusqlite::Result<AlertSeverity> {
    match value {
        "neutral" => Ok(AlertSeverity::Neutral),
        "informational" => Ok(AlertSeverity::Informational),
        "warning" => Ok(AlertSeverity::Warning),
        "critical" => Ok(AlertSeverity::Critical),
        "exhausted" => Ok(AlertSeverity::Exhausted),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            Type::Text,
            Box::new(DbError::InvalidData(format!(
                "unknown alert severity: {other}"
            ))),
        )),
    }
}

fn sanitize_settings(settings: &AppSettings) -> AppSettings {
    let mut thresholds = settings
        .quota_thresholds
        .iter()
        .copied()
        .map(|value| value.clamp(1, 100))
        .collect::<Vec<_>>();
    thresholds.sort_unstable();
    thresholds.dedup();
    if thresholds.is_empty() {
        thresholds = AppSettings::default().quota_thresholds;
    }

    AppSettings {
        collection_enabled: settings.collection_enabled,
        minimize_to_tray: settings.minimize_to_tray,
        launch_at_login: settings.launch_at_login,
        retention_days: settings.retention_days.clamp(1, 3650),
        quota_thresholds: thresholds,
        raw_event_retention_enabled: settings.raw_event_retention_enabled,
    }
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn format_percent(value: f64) -> String {
    if (value.fract() - 0.0).abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

fn csv_row(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| {
            let escaped = value.replace('"', "\"\"");
            format!("\"{escaped}\"")
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn string_array(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn derive_burn_evidence(context: BurnContext) -> BurnEvidence {
    let cache_reuse = if context.input > 0 {
        ((context.cached_input as f64 / context.input as f64) * 100.0).round() as i64
    } else {
        0
    };
    let mut factors = vec![format!(
        "{} reported total tokens",
        format_large_number(context.turn.tokens)
    )];
    if context.input > 0 {
        factors.push(format!("{cache_reuse}% cache reuse"));
    }
    if let Some(model) = &context.turn.model {
        factors.push(format!("{model} model usage"));
    }
    if let Some(reasoning) = &context.turn.reasoning_effort {
        factors.push(format!("{reasoning} reasoning configured"));
    }
    if context.compaction_count > 0 || context.subagent_count > 0 {
        factors.push(format!(
            "{} compactions and {} subagent sessions",
            context.compaction_count, context.subagent_count
        ));
    }

    BurnEvidence {
        headline: "High usage correlates with a large context window and extended reasoning."
            .to_string(),
        factors,
        missing_signals: vec![
            "Exact quota units".to_string(),
            "Direct project-level quota attribution".to_string(),
        ],
        method: "Ranks observed signals; no causal claim is made.".to_string(),
        confidence: BurnConfidence::Medium,
    }
}

fn format_large_number(value: i64) -> String {
    if value >= 1_000_000 {
        format!("{:.2} million", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1} thousand", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

fn collect_rows<T, F>(rows: rusqlite::MappedRows<'_, F>) -> Result<Vec<T>, DbError>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

impl Database {
    fn set_setting_json<T: Serialize>(
        tx: &Transaction<'_>,
        key: &str,
        value: &T,
    ) -> Result<(), DbError> {
        tx.execute(
            "INSERT INTO settings(key, value_json, updated_at)
             VALUES(?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET
               value_json = excluded.value_json,
               updated_at = excluded.updated_at",
            params![key, serde_json::to_string(value)?, now_rfc3339()],
        )?;
        Ok(())
    }

    fn set_setting_if_missing<T: Serialize>(
        tx: &Transaction<'_>,
        key: &str,
        value: &T,
    ) -> Result<(), DbError> {
        tx.execute(
            "INSERT INTO settings(key, value_json, updated_at)
             VALUES(?1, ?2, ?3)
             ON CONFLICT(key) DO NOTHING",
            params![key, serde_json::to_string(value)?, now_rfc3339()],
        )?;
        Ok(())
    }

    fn set_metadata_json<T: Serialize>(
        tx: &Transaction<'_>,
        key: &str,
        value: &T,
    ) -> Result<(), DbError> {
        tx.execute(
            "INSERT INTO app_metadata(key, value, updated_at)
             VALUES(?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET
               value = excluded.value,
               updated_at = excluded.updated_at",
            params![key, serde_json::to_string(value)?, now_rfc3339()],
        )?;
        Ok(())
    }

    fn set_metadata_if_missing<T: Serialize>(
        tx: &Transaction<'_>,
        key: &str,
        value: &T,
    ) -> Result<(), DbError> {
        tx.execute(
            "INSERT INTO app_metadata(key, value, updated_at)
             VALUES(?1, ?2, ?3)
             ON CONFLICT(key) DO NOTHING",
            params![key, serde_json::to_string(value)?, now_rfc3339()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn open_temp_db() -> (tempfile::TempDir, std::path::PathBuf, Database) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("codex-meter.sqlite");
        let db = Database::open(&path).unwrap();
        (dir, path, db)
    }

    #[test]
    fn migrations_are_transactional_ordered_and_applied_once() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("codex-meter.sqlite");

        let first_applied_at = {
            let db = Database::open(&db_path).unwrap();
            let foreign_keys: i64 = db
                .conn
                .pragma_query_value(None, "foreign_keys", |row| row.get(0))
                .unwrap();
            assert_eq!(foreign_keys, 1);

            let migrations: Vec<(i64, String)> = {
                let mut statement = db
                    .conn
                    .prepare("SELECT version, name FROM schema_migrations ORDER BY version")
                    .unwrap();
                statement
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                    .unwrap()
                    .collect::<rusqlite::Result<_>>()
                    .unwrap()
            };
            assert_eq!(
                migrations,
                MIGRATIONS
                    .iter()
                    .map(|(version, name, _)| (*version, (*name).to_string()))
                    .collect::<Vec<_>>()
            );
            db.conn
                .query_row(
                    "SELECT applied_at FROM schema_migrations WHERE version = ?1",
                    params![MIGRATION_VERSION],
                    |row| row.get::<_, String>(0),
                )
                .unwrap()
        };

        let reopened = Database::open(&db_path).unwrap();
        let migration_count: i64 = reopened
            .conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        let second_applied_at: String = reopened
            .conn
            .query_row(
                "SELECT applied_at FROM schema_migrations WHERE version = ?1",
                params![MIGRATION_VERSION],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(migration_count, MIGRATIONS.len() as i64);
        assert_eq!(second_applied_at, first_applied_at);
    }

    #[test]
    fn failed_migration_rolls_back_all_schema_changes() {
        let mut connection = Connection::open_in_memory().unwrap();
        let result = {
            let tx = connection.transaction().unwrap();
            apply_migration(
                &tx,
                99,
                "broken",
                "CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    applied_at TEXT NOT NULL
                 );
                 CREATE TABLE rollback_probe(id INTEGER PRIMARY KEY);
                 THIS IS NOT VALID SQL;",
            )
        };
        assert!(result.is_err());

        let probe_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name IN ('schema_migrations', 'rollback_probe')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(probe_count, 0);
    }

    #[test]
    fn fixture_loading_is_idempotent_and_populates_dashboard() {
        let (_dir, _path, mut db) = open_temp_db();

        db.load_synthetic_fixture().unwrap();
        db.load_synthetic_fixture().unwrap();

        let dashboard = db.dashboard().unwrap();
        assert!(dashboard.demo_mode);
        assert_eq!(dashboard.collector_state, "fixture data loaded");
        assert_eq!(dashboard.codex_version, DEFAULT_CODEX_VERSION);
        assert_eq!(dashboard.quota.len(), 2);
        assert_eq!(
            dashboard
                .account_usage
                .as_ref()
                .and_then(|summary| summary.lifetime_tokens),
            Some(3_482_100)
        );
        assert_eq!(dashboard.today.total, Some(2_138_200));
        assert_eq!(dashboard.turns.len(), 2);
        assert_eq!(dashboard.turns[0].id, "turn-fixture-003");
        assert_eq!(dashboard.burn.accuracy, Accuracy::Estimated);
        assert_eq!(dashboard.channels[1].status, ChannelStatus::Healthy);
        assert_eq!(
            dashboard
                .collector_diagnostics
                .latest_session
                .as_ref()
                .map(|session| session.id.as_str()),
            Some("fixture-session")
        );

        let counts = db.table_counts().unwrap();
        assert_eq!(counts.get("usage_records"), Some(&2));
        assert_eq!(counts.get("quota_snapshots"), Some(&2));
        assert_eq!(counts.get("account_usage_snapshots"), Some(&1));
        assert_eq!(counts.get("turns"), Some(&2));
    }

    #[test]
    fn dynamic_rate_limits_support_multiple_buckets_and_dedupe_identical_snapshots() {
        let (_dir, _path, mut db) = open_temp_db();
        let payload = json!({
            "rateLimitsByLimitId": {
                "codex": {
                    "limitId": "codex",
                    "limitName": "Codex",
                    "primary": {
                        "usedPercent": 68,
                        "windowDurationMins": 300,
                        "resetsAt": 1753254000
                    },
                    "secondary": {
                        "usedPercent": 41,
                        "windowDurationMins": 10080,
                        "resetsAt": 1753660800000i64
                    }
                },
                "chatgpt": {
                    "limitId": "chatgpt",
                    "limitName": "ChatGPT",
                    "primary": {
                        "usedPercent": 12,
                        "windowDurationMins": 1440,
                        "resetsAt": 1753336800
                    },
                    "secondary": null
                }
            }
        });

        db.record_rate_limits(&payload, "2026-07-23T10:00:00Z", "app_server")
            .unwrap();
        db.record_rate_limits(&payload, "2026-07-23T10:05:00Z", "app_server")
            .unwrap();

        let quota = db.quota_windows().unwrap();
        assert_eq!(quota.len(), 3);
        assert_eq!(
            db.conn
                .query_row("SELECT COUNT(*) FROM quota_snapshots", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert!(quota.iter().any(|window| {
            window.source_limit_id.as_deref() == Some("codex")
                && window.window_kind == "primary"
                && window.remaining_percent == Some(32.0)
                && window.reset_interpretation == "unix_seconds"
        }));
        assert!(quota.iter().any(|window| {
            window.source_limit_id.as_deref() == Some("codex")
                && window.window_kind == "secondary"
                && window.reset_interpretation == "unix_milliseconds"
                && window.resets_at_raw == Some(1_753_660_800_000)
        }));
    }

    #[test]
    fn quota_forecast_is_calculated_and_persisted_from_compatible_history() {
        let (_dir, _path, mut db) = open_temp_db();
        let now = Utc::now();
        let reset_raw = (now + ChronoDuration::hours(2)).timestamp();
        for (minutes_ago, used_percent) in [(120, 50), (90, 55), (60, 60), (30, 65), (0, 70)] {
            let payload = json!({
                "rateLimitsByLimitId": {
                    "codex": {
                        "limitId": "codex",
                        "limitName": "Codex",
                        "primary": {
                            "usedPercent": used_percent,
                            "windowDurationMins": 300,
                            "resetsAt": reset_raw
                        },
                        "secondary": null
                    }
                }
            });
            db.record_rate_limits(
                &payload,
                &(now - ChronoDuration::minutes(minutes_ago)).to_rfc3339(),
                "app_server",
            )
            .unwrap();
        }

        let dashboard = db.dashboard().unwrap();
        let forecast = dashboard.forecasts.first().unwrap();
        assert_eq!(forecast.quality.observation_count, 5);
        assert!(forecast.selected_rate_pph.is_some());
        assert!(forecast.safe_rate_pph.is_some());
        assert_eq!(forecast.status.state, TelemetryState::Live);
        assert_eq!(
            db.conn
                .query_row("SELECT COUNT(*) FROM quota_forecast_history", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
        let csv = db.export_csv().unwrap();
        assert!(csv.contains(&format!("\"quota_forecast\",\"{}\"", forecast.bucket_id)));
        assert!(csv.contains(&format!("\"quota_safe_rate\",\"{}\"", forecast.bucket_id)));
    }

    #[test]
    fn forecast_alerts_are_deduplicated_readable_and_resolved_when_the_segment_changes() {
        let (_dir, _path, mut db) = open_temp_db();
        let now = Utc::now();
        let reset_raw = (now + ChronoDuration::hours(2)).timestamp();
        for (minutes_ago, used_percent) in [(120, 20), (90, 35), (60, 50), (30, 65), (0, 80)] {
            let payload = json!({
                "rateLimitsByLimitId": {
                    "codex": {
                        "limitId": "codex",
                        "limitName": "Codex",
                        "primary": {
                            "usedPercent": used_percent,
                            "windowDurationMins": 300,
                            "resetsAt": reset_raw
                        },
                        "secondary": null
                    }
                }
            });
            db.record_rate_limits(
                &payload,
                &(now - ChronoDuration::minutes(minutes_ago)).to_rfc3339(),
                "app_server",
            )
            .unwrap();
        }

        let first = db.dashboard().unwrap();
        let forecast_alerts = first
            .alerts
            .active
            .iter()
            .filter(|alert| alert.alert_source == "local_forecast")
            .collect::<Vec<_>>();
        assert!(forecast_alerts
            .iter()
            .any(|alert| alert.alert_type == "forecast_exhaustion_before_reset"));
        assert!(forecast_alerts.iter().all(|alert| {
            alert.accuracy == Accuracy::Estimated
                && alert.unread
                && alert.predicted_exhaustion_at.is_some()
                && alert.forecast_confidence.is_some()
        }));
        let history_count = first.alerts.history.len();
        assert_eq!(db.dashboard().unwrap().alerts.history.len(), history_count);

        db.mark_alerts_read().unwrap();
        assert!(db
            .alert_center()
            .unwrap()
            .active
            .iter()
            .all(|alert| !alert.unread));

        let reset_payload = json!({
            "rateLimitsByLimitId": {
                "codex": {
                    "limitId": "codex",
                    "limitName": "Codex",
                    "primary": {
                        "usedPercent": 10,
                        "windowDurationMins": 300,
                        "resetsAt": reset_raw
                    },
                    "secondary": null
                }
            }
        });
        db.record_rate_limits(
            &reset_payload,
            &(now + ChronoDuration::minutes(30)).to_rfc3339(),
            "app_server",
        )
        .unwrap();

        let resolved = db.dashboard().unwrap().alerts;
        assert!(resolved
            .active
            .iter()
            .all(|alert| alert.alert_source != "local_forecast"));
        assert!(resolved.history.iter().any(|alert| {
            alert.alert_source == "local_forecast" && alert.resolved_at.is_some()
        }));
    }

    #[test]
    fn quota_threshold_notifications_are_exact_deduplicated_and_suppressed_in_demo_mode() {
        let (_dir, _path, mut db) = open_temp_db();
        let first_window = json!({
            "rateLimitsByLimitId": {
                "codex": {
                    "limitId": "codex",
                    "limitName": "Codex",
                    "primary": {
                        "usedPercent": 91,
                        "windowDurationMins": 300,
                        "resetsAt": 1753254000
                    },
                    "secondary": null
                }
            }
        });

        db.record_rate_limits(&first_window, "2026-07-23T10:00:00Z", "app_server")
            .unwrap();
        let notifications = db.claim_quota_threshold_notifications().unwrap();
        assert_eq!(
            notifications
                .iter()
                .map(|notification| notification.threshold)
                .collect::<Vec<_>>(),
            vec![50, 75, 90]
        );
        assert!(notifications
            .iter()
            .all(|notification| notification.accuracy == Accuracy::ReportedExact));
        assert!(db.claim_quota_threshold_notifications().unwrap().is_empty());

        let next_window = json!({
            "rateLimitsByLimitId": {
                "codex": {
                    "limitId": "codex",
                    "limitName": "Codex",
                    "primary": {
                        "usedPercent": 91,
                        "windowDurationMins": 300,
                        "resetsAt": 1753257600
                    },
                    "secondary": null
                }
            }
        });
        db.record_rate_limits(&next_window, "2026-07-23T11:00:00Z", "app_server")
            .unwrap();
        assert_eq!(db.claim_quota_threshold_notifications().unwrap().len(), 3);

        db.load_synthetic_fixture().unwrap();
        assert!(db.claim_quota_threshold_notifications().unwrap().is_empty());
    }

    #[test]
    fn alert_severity_uses_fixed_accessible_bands() {
        assert_eq!(alert_severity(0.0), AlertSeverity::Neutral);
        assert_eq!(alert_severity(49.9), AlertSeverity::Neutral);
        assert_eq!(alert_severity(50.0), AlertSeverity::Informational);
        assert_eq!(alert_severity(74.9), AlertSeverity::Informational);
        assert_eq!(alert_severity(75.0), AlertSeverity::Warning);
        assert_eq!(alert_severity(89.9), AlertSeverity::Warning);
        assert_eq!(alert_severity(90.0), AlertSeverity::Critical);
        assert_eq!(alert_severity(99.9), AlertSeverity::Critical);
        assert_eq!(alert_severity(100.0), AlertSeverity::Exhausted);
    }

    #[test]
    fn alert_history_dismissal_and_deduplication_persist_across_restart() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("codex-meter.sqlite");
        let payload = json!({
            "rateLimitsByLimitId": {
                "codex": {
                    "limitId": "codex",
                    "limitName": "Codex",
                    "primary": {
                        "usedPercent": 80,
                        "windowDurationMins": 300,
                        "resetsAt": 1753254000
                    },
                    "secondary": null
                }
            }
        });

        {
            let mut db = Database::open(&db_path).unwrap();
            db.record_rate_limits(&payload, "2026-07-23T10:00:00Z", "app_server")
                .unwrap();
            assert_eq!(db.claim_quota_threshold_notifications().unwrap().len(), 2);
            assert!(db.claim_quota_threshold_notifications().unwrap().is_empty());

            let alerts = db.alert_center().unwrap();
            assert_eq!(alerts.active.len(), 2);
            assert_eq!(alerts.history.len(), 2);
            let warning = alerts
                .active
                .iter()
                .find(|alert| alert.threshold == 75)
                .unwrap();
            assert_eq!(warning.severity, AlertSeverity::Warning);
            db.dismiss_alert(
                &warning.bucket_id,
                &warning.reset_window_id,
                warning.threshold,
            )
            .unwrap();
        }

        let reopened = Database::open(&db_path).unwrap();
        let alerts = reopened.alert_center().unwrap();
        assert_eq!(alerts.active.len(), 1);
        assert_eq!(alerts.dismissed.len(), 1);
        assert_eq!(alerts.history.len(), 2);
        assert_eq!(alerts.dismissed[0].threshold, 75);
    }

    #[test]
    fn token_state_distinguishes_missing_zero_waiting_unsupported_error_and_recovery() {
        let (_dir, _path, mut db) = open_temp_db();
        let empty = db.dashboard().unwrap();
        assert_eq!(empty.today.total, None);
        assert_eq!(empty.token_status.state, TelemetryState::Disabled);

        db.conn
            .execute(
                "UPDATE sources
                 SET enabled = 1, health = 'healthy'
                 WHERE id = 'opentelemetry'",
                [],
            )
            .unwrap();
        assert_eq!(
            db.dashboard().unwrap().token_status.state,
            TelemetryState::Waiting
        );

        db.conn
            .execute(
                "UPDATE capabilities
                 SET availability = 'unavailable'
                 WHERE metric = 'turn.total_tokens'",
                [],
            )
            .unwrap();
        assert_eq!(
            db.dashboard().unwrap().token_status.state,
            TelemetryState::Unsupported
        );
        db.conn
            .execute(
                "UPDATE capabilities
                 SET availability = 'available'
                 WHERE metric = 'turn.total_tokens'",
                [],
            )
            .unwrap();

        db.record_collection_error(
            "opentelemetry",
            "2026-07-23T10:00:00Z",
            "receiver",
            "fixture receiver failure",
            true,
        )
        .unwrap();
        assert_eq!(
            db.dashboard().unwrap().token_status.state,
            TelemetryState::Error
        );
        db.record_collector_health(
            "opentelemetry",
            ChannelStatus::Healthy,
            "2026-07-23T11:00:00Z",
            "Recovered",
        )
        .unwrap();
        let recovered = db.dashboard().unwrap();
        let source = recovered
            .channels
            .iter()
            .find(|source| source.id == "opentelemetry")
            .unwrap();
        assert!(source.healthy);
        assert!(source
            .latest_error
            .as_ref()
            .is_some_and(|error| error.historical));

        let zero = json!({
            "threadId": "thread-zero",
            "turnId": "turn-zero",
            "tokenUsage": {
                "last": {
                    "inputTokens": 0,
                    "cachedInputTokens": 0,
                    "outputTokens": 0,
                    "reasoningOutputTokens": 0,
                    "totalTokens": 0
                }
            }
        });
        db.record_token_usage_notification(&zero, "2026-07-23T12:00:00Z", "opentelemetry")
            .unwrap();
        let dashboard = db.dashboard().unwrap();
        assert_eq!(dashboard.today.total, Some(0));
        assert_eq!(dashboard.token_status.state, TelemetryState::Live);
    }

    #[test]
    fn fallback_rate_limits_mark_ambiguous_reset_values() {
        let (_dir, _path, mut db) = open_temp_db();
        let payload = json!({
            "rateLimits": {
                "limitId": null,
                "limitName": "Fallback",
                "primary": {
                    "usedPercent": 55,
                    "windowDurationMins": 60,
                    "resetsAt": 12345
                },
                "secondary": null
            }
        });

        db.record_rate_limits(&payload, "2026-07-23T11:00:00Z", "app-server")
            .unwrap();

        let quota = db.quota_windows().unwrap();
        assert_eq!(quota.len(), 1);
        assert_eq!(quota[0].name, "Fallback");
        assert_eq!(quota[0].remaining_percent, Some(45.0));
        assert_eq!(quota[0].resets_at_raw, Some(12_345));
        assert!(quota[0].resets_at.is_none());
        assert_eq!(quota[0].reset_interpretation, "ambiguous");
    }

    #[test]
    fn account_usage_is_idempotent_and_persists_optional_fields_and_daily_buckets() {
        let (_dir, _path, mut db) = open_temp_db();
        let payload = json!({
            "summary": {
                "lifetimeTokens": 99_000,
                "peakDailyTokens": 22_000,
                "longestRunningTurnSec": null,
                "currentStreakDays": 3,
                "longestStreakDays": 7
            },
            "dailyUsageBuckets": [
                { "startDate": "2026-07-22", "tokens": 22_000 },
                { "startDate": "2026-07-23", "tokens": 11_000 }
            ]
        });

        db.record_account_usage(&payload, "2026-07-23T12:00:00Z", "app_server")
            .unwrap();
        db.record_account_usage(&payload, "2026-07-23T12:00:00Z", "app_server")
            .unwrap();

        let account_usage = db.account_usage_summary().unwrap().unwrap();
        assert_eq!(account_usage.lifetime_tokens, Some(99_000));
        assert_eq!(account_usage.longest_running_turn_sec, None);
        assert_eq!(account_usage.daily_usage_buckets.len(), 2);
        assert_eq!(
            db.conn
                .query_row("SELECT COUNT(*) FROM account_usage_snapshots", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
        assert_eq!(
            db.conn
                .query_row("SELECT COUNT(*) FROM account_usage_daily", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            2
        );
    }

    #[test]
    fn token_usage_notifications_create_stable_stubs_and_dedupe() {
        let (_dir, _path, mut db) = open_temp_db();
        let notification = json!({
            "threadId": "thread-live-1",
            "turnId": "turn-live-1",
            "tokenUsage": {
                "last": {
                    "inputTokens": 100,
                    "cachedInputTokens": 20,
                    "outputTokens": 10,
                    "reasoningOutputTokens": 5,
                    "totalTokens": 135
                },
                "total": {
                    "inputTokens": 100,
                    "cachedInputTokens": 20,
                    "outputTokens": 10,
                    "reasoningOutputTokens": 5,
                    "totalTokens": 135
                },
                "modelContextWindow": 200000
            }
        });

        db.record_token_usage_notification(&notification, "2026-07-23T12:30:00Z", "app_server")
            .unwrap();
        db.record_token_usage_notification(&notification, "2026-07-23T12:31:00Z", "app_server")
            .unwrap();

        assert_eq!(
            db.conn
                .query_row(
                    "SELECT COUNT(*) FROM threads WHERE id = 'thread-live-1'",
                    [],
                    |row| { row.get::<_, i64>(0) }
                )
                .unwrap(),
            1
        );
        assert_eq!(
            db.conn
                .query_row(
                    "SELECT COUNT(*) FROM turns WHERE id = 'turn-live-1'",
                    [],
                    |row| { row.get::<_, i64>(0) }
                )
                .unwrap(),
            1
        );
        assert_eq!(
            db.conn
                .query_row("SELECT COUNT(*) FROM usage_records", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        let total_tokens: i64 = db
            .conn
            .query_row(
                "SELECT total_tokens FROM usage_records WHERE turn_id = 'turn-live-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(total_tokens, 135);
    }

    #[test]
    fn collector_metadata_is_redacted_and_exposed_in_dashboard_and_diagnostics() {
        let (_dir, _path, mut db) = open_temp_db();
        db.record_collector_health(
            "app_server",
            ChannelStatus::Degraded,
            "2026-07-23T13:00:00Z",
            "authorization: Bearer abc C:\\Users\\person\\repo",
        )
        .unwrap();
        db.record_collector_session(
            "session-live",
            "2026-07-23T13:00:00Z",
            None,
            "token=abc C:\\Users\\person\\repo",
            2,
        )
        .unwrap();
        db.record_collection_error(
            "app_server",
            "2026-07-23T13:01:00Z",
            "request_timeout",
            "authorization: Bearer abc /home/person/repo",
            true,
        )
        .unwrap();

        let dashboard = db.dashboard().unwrap();
        assert_eq!(
            dashboard.collector_diagnostics.recent_sessions[0].health,
            "[REDACTED] [LOCAL_PATH]"
        );
        assert!(dashboard.collector_diagnostics.recent_errors[0]
            .redacted_message
            .contains("[REDACTED]"));
        assert!(!dashboard.collector_diagnostics.recent_errors[0]
            .redacted_message
            .contains("person"));
        assert!(dashboard.channels.iter().any(|channel| {
            channel.name == "App Server"
                && channel
                    .latest_error
                    .as_ref()
                    .is_some_and(|error| error.message.contains("[REDACTED]"))
        }));

        let diagnostics = db.diagnostics_json().unwrap();
        assert!(diagnostics.contains("\"collectorDiagnostics\""));
        assert!(diagnostics.contains("[REDACTED]"));
        assert!(!diagnostics.contains("person"));
    }

    #[test]
    fn settings_round_trip_and_retention_prunes_old_rows() {
        let (_dir, _path, mut db) = open_temp_db();
        db.load_synthetic_fixture().unwrap();

        let old_timestamp = (Utc::now() - ChronoDuration::days(120)).to_rfc3339();
        db.conn
            .execute(
                "INSERT INTO usage_records(
                    id, source_id, external_event_id, thread_id, turn_id, model_id, observed_at,
                    input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens, accuracy
                 )
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, 0, 0, 0, 1, ?8)",
                params![
                    "usage-old",
                    "opentelemetry",
                    "usage-old",
                    "thread-fixture-sol",
                    "turn-fixture-003",
                    "model-gpt-5.4-sol",
                    old_timestamp,
                    accuracy_str(Accuracy::ReportedExact)
                ],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO account_usage_snapshots(
                    id, source_id, observed_at, lifetime_tokens, peak_daily_tokens,
                    longest_running_turn_sec, current_streak_days, longest_streak_days, accuracy
                 )
                 VALUES(?1, ?2, ?3, 1, 1, 1, 1, 1, ?4)",
                params![
                    "account-old",
                    "app-server",
                    old_timestamp,
                    accuracy_str(Accuracy::ReportedExact)
                ],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO account_usage_daily(source_id, start_date, tokens, observed_at, accuracy)
                 VALUES(?1, ?2, ?3, ?4, ?5)",
                params![
                    "app-server",
                    "2026-03-01",
                    10,
                    old_timestamp,
                    accuracy_str(Accuracy::ReportedExact)
                ],
            )
            .unwrap();

        db.save_settings(&AppSettings {
            retention_days: 30,
            quota_thresholds: vec![90, 75, 101, 75],
            ..AppSettings::default()
        })
        .unwrap();

        let settings = db.settings().unwrap();
        assert_eq!(settings.retention_days, 30);
        assert_eq!(settings.quota_thresholds, vec![75, 90, 100]);

        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM usage_records WHERE id = 'usage-old'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        let old_account_snapshots: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM account_usage_snapshots WHERE id = 'account-old'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let old_daily: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM account_usage_daily WHERE start_date = '2026-03-01'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(old_account_snapshots, 0);
        assert_eq!(old_daily, 0);
    }

    #[test]
    fn legacy_persisted_fixture_is_removed_while_settings_survive_restart() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("codex-meter.sqlite");
        {
            let mut db = Database::open(&db_path).unwrap();
            db.load_synthetic_fixture().unwrap();
            db.record_collector_session(
                "session-restart",
                "2026-07-23T14:00:00Z",
                None,
                "collector healthy",
                1,
            )
            .unwrap();
            db.save_settings(&AppSettings {
                collection_enabled: false,
                retention_days: 180,
                ..AppSettings::default()
            })
            .unwrap();
        }

        let reopened = Database::open(&db_path).unwrap();
        let dashboard = reopened.dashboard().unwrap();
        let settings = reopened.settings().unwrap();
        assert!(!dashboard.demo_mode);
        assert!(dashboard.quota.is_empty());
        assert!(dashboard.account_usage.is_none());
        assert_eq!(dashboard.today.total, None);
        assert!(dashboard.collector_diagnostics.latest_session.is_none());
        assert!(!settings.collection_enabled);
        assert_eq!(settings.retention_days, 180);
    }

    #[test]
    fn exports_are_redacted_and_delete_analytics_resets_dashboard() {
        let (_dir, _path, mut db) = open_temp_db();
        db.load_synthetic_fixture().unwrap();

        let json_export = db.export_json().unwrap();
        assert!(json_export.contains("\"redacted\": true"));
        assert!(json_export.contains("\"demoMode\": true"));
        assert!(json_export.contains("\"accountUsage\""));

        let csv_export = db.export_csv().unwrap();
        assert!(csv_export.contains("\"section\",\"id\",\"label\",\"value\",\"accuracy\""));
        assert!(csv_export.contains("\"turn\",\"turn-fixture-003\""));
        assert!(csv_export.contains("\"account_usage_day\""));

        let diagnostics = db.diagnostics_json().unwrap();
        assert!(diagnostics.contains("\"schemaVersion\": 4"));
        assert!(diagnostics.contains("\"demoMode\": true"));
        assert!(diagnostics.contains("\"collectorDiagnostics\""));

        db.delete_analytics().unwrap();
        let dashboard = db.dashboard().unwrap();
        assert!(!dashboard.demo_mode);
        assert!(dashboard.quota.is_empty());
        assert!(dashboard.account_usage.is_none());
        assert!(dashboard.turns.is_empty());
        assert!(dashboard.collector_diagnostics.recent_sessions.is_empty());
        assert_eq!(dashboard.channels[1].status, ChannelStatus::Inactive);
        assert_eq!(
            db.conn
                .query_row("SELECT COUNT(*) FROM account_usage_snapshots", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
    }
}
