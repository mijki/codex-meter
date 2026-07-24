use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Accuracy {
    ReportedExact,
    DerivedExact,
    Estimated,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BurnConfidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelStatus {
    Healthy,
    Inactive,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TelemetryState {
    Live,
    Waiting,
    Disabled,
    Unsupported,
    Unavailable,
    Error,
    Fixture,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryStatus {
    pub state: TelemetryState,
    pub accuracy: Accuracy,
    pub source: String,
    pub last_observed_at: Option<String>,
    pub reason: Option<String>,
    pub required_integration: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertSeverity {
    Neutral,
    Informational,
    Warning,
    Critical,
    Exhausted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub id: String,
    pub source_limit_id: Option<String>,
    pub name: String,
    pub window_kind: String,
    pub window_duration_minutes: Option<i64>,
    pub window_label: String,
    pub used_percent: Option<f64>,
    pub remaining_percent: Option<f64>,
    pub resets_at: Option<String>,
    pub resets_at_raw: Option<i64>,
    pub reset_interpretation: String,
    pub accuracy: Accuracy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForecastModel {
    RecentRate,
    OrdinaryLeastSquares,
    ExponentiallyWeightedMovingAverage,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForecastConfidence {
    Unavailable,
    Preliminary,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForecastRisk {
    Healthy,
    Watch,
    AtRisk,
    ExhaustionLikely,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaObservation {
    pub observed_at: String,
    pub used_percent: f64,
    pub accuracy: Accuracy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumptionTrajectoryPoint {
    pub observed_at: String,
    pub reported_used_percent: Option<f64>,
    pub rolling_trend_percent: Option<f64>,
    pub forecast_used_percent: Option<f64>,
    pub confidence_low_percent: Option<f64>,
    pub confidence_high_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BurnRatePoint {
    pub observed_at: String,
    pub interval_rate_pph: Option<f64>,
    pub rolling_30m_rate_pph: Option<f64>,
    pub rolling_1h_rate_pph: Option<f64>,
    pub rolling_3h_rate_pph: Option<f64>,
    pub rolling_6h_rate_pph: Option<f64>,
    pub ewma_rate_pph: Option<f64>,
    pub safe_rate_pph: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastQuality {
    pub selected_model: ForecastModel,
    pub confidence: ForecastConfidence,
    pub observation_count: usize,
    pub coverage_duration_minutes: f64,
    pub polling_regularity: f64,
    pub largest_gap_minutes: f64,
    pub model_agreement: Option<f64>,
    pub slope_stability: Option<f64>,
    pub invalidation_reason: Option<String>,
    pub missing_signals: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaForecast {
    pub bucket_id: String,
    pub bucket_name: String,
    pub window_label: String,
    pub generated_at: String,
    pub resets_at: Option<String>,
    pub current_used_percent: Option<f64>,
    pub remaining_percent: Option<f64>,
    pub latest_interval_rate_pph: Option<f64>,
    pub rolling_30m_rate_pph: Option<f64>,
    pub rolling_1h_rate_pph: Option<f64>,
    pub rolling_3h_rate_pph: Option<f64>,
    pub rolling_6h_rate_pph: Option<f64>,
    pub complete_window_rate_pph: Option<f64>,
    pub ewma_rate_pph: Option<f64>,
    pub regression_rate_pph: Option<f64>,
    pub selected_rate_pph: Option<f64>,
    pub safe_rate_pph: Option<f64>,
    pub pace_ratio: Option<f64>,
    pub risk: ForecastRisk,
    pub predicted_exhaustion_at: Option<String>,
    pub projected_usage_at_reset: Option<f64>,
    pub exhaustion_before_reset: Option<bool>,
    pub quality: ForecastQuality,
    pub trajectory: Vec<ConsumptionTrajectoryPoint>,
    pub burn_rates: Vec<BurnRatePoint>,
    pub status: TelemetryStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsageDailyBucket {
    pub start_date: String,
    pub tokens: i64,
    pub observed_at: String,
    pub accuracy: Accuracy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsageSummary {
    pub observed_at: String,
    pub lifetime_tokens: Option<i64>,
    pub peak_daily_tokens: Option<i64>,
    pub longest_running_turn_sec: Option<i64>,
    pub current_streak_days: Option<i64>,
    pub longest_streak_days: Option<i64>,
    pub daily_usage_buckets: Vec<AccountUsageDailyBucket>,
    pub accuracy: Accuracy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaThresholdEvent {
    pub bucket_id: String,
    pub bucket_name: String,
    pub threshold: i64,
    pub used_percent: f64,
    pub resets_at: Option<String>,
    pub remaining_percent: f64,
    pub collection_timestamp: String,
    pub severity: AlertSeverity,
    pub accuracy: Accuracy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenTotals {
    pub input: Option<i64>,
    pub cached_input: Option<i64>,
    pub output: Option<i64>,
    pub reasoning_output: Option<i64>,
    pub total: Option<i64>,
    pub accuracy: Accuracy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpensiveTurn {
    pub id: String,
    pub thread_id: String,
    pub project: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub tokens: i64,
    pub duration_ms: Option<i64>,
    pub accuracy: Accuracy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BurnAnalysis {
    pub headline: String,
    pub factors: Vec<String>,
    pub missing_signals: Vec<MissingSignal>,
    pub method: String,
    pub confidence: BurnConfidence,
    pub accuracy: Accuracy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingSignal {
    pub name: String,
    pub source: String,
    pub setup_state: TelemetryState,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceError {
    pub occurred_at: String,
    pub message: String,
    pub historical: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelHealth {
    pub id: String,
    pub name: String,
    pub status: ChannelStatus,
    pub enabled: bool,
    pub healthy: bool,
    pub detail: String,
    pub last_event_at: Option<String>,
    pub last_successful_collection_at: Option<String>,
    pub latest_error: Option<SourceError>,
    pub capabilities: Vec<String>,
    pub telemetry_state: TelemetryState,
    pub configuration_status: String,
    pub restart_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaAlert {
    pub id: String,
    pub created_at: String,
    pub collection_timestamp: String,
    pub bucket_id: String,
    pub bucket_name: String,
    pub reset_window_id: String,
    pub resets_at: Option<String>,
    pub threshold: i64,
    pub used_percent: f64,
    pub remaining_percent: f64,
    pub severity: AlertSeverity,
    pub accuracy: Accuracy,
    pub dismissed_at: Option<String>,
    pub resolved_at: Option<String>,
    pub unread: bool,
    pub alert_type: String,
    pub alert_source: String,
    pub predicted_exhaustion_at: Option<String>,
    pub forecast_confidence: Option<ForecastConfidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertCenter {
    pub active: Vec<QuotaAlert>,
    pub dismissed: Vec<QuotaAlert>,
    pub history: Vec<QuotaAlert>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorSessionDiagnostic {
    pub id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub health: String,
    pub restart_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorErrorDiagnostic {
    pub occurred_at: String,
    pub source_id: Option<String>,
    pub category: String,
    pub redacted_message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorDiagnostics {
    pub latest_session: Option<CollectorSessionDiagnostic>,
    pub recent_sessions: Vec<CollectorSessionDiagnostic>,
    pub recent_errors: Vec<CollectorErrorDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DistributionPoint {
    pub label: String,
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub demo_mode: bool,
    pub collector_state: String,
    pub codex_version: String,
    pub last_event_at: Option<String>,
    pub quota: Vec<QuotaWindow>,
    pub quota_status: TelemetryStatus,
    pub forecasts: Vec<QuotaForecast>,
    pub forecast_status: TelemetryStatus,
    pub account_usage: Option<AccountUsageSummary>,
    pub account_usage_status: TelemetryStatus,
    pub today: TokenTotals,
    pub token_status: TelemetryStatus,
    pub turns: Vec<ExpensiveTurn>,
    pub turns_status: TelemetryStatus,
    pub burn: BurnAnalysis,
    pub burn_status: TelemetryStatus,
    pub channels: Vec<ChannelHealth>,
    pub collector_diagnostics: CollectorDiagnostics,
    pub model_distribution: Vec<DistributionPoint>,
    pub reasoning_distribution: Vec<DistributionPoint>,
    pub model_status: TelemetryStatus,
    pub alerts: AlertCenter,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityStatus {
    pub metric: String,
    pub source_channel: String,
    pub source_method: Option<String>,
    pub source_field: Option<String>,
    pub availability: String,
    pub accuracy: Accuracy,
    pub limitation: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    pub id: String,
    pub channel: String,
    pub enabled: bool,
    pub health: ChannelStatus,
    pub last_event_at: Option<String>,
    pub last_successful_collection_at: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsReport {
    pub app_version: String,
    pub schema_version: i64,
    pub codex_version: String,
    pub demo_mode: bool,
    pub collector_state: String,
    pub settings: AppSettings,
    pub sources: Vec<SourceStatus>,
    pub capabilities: Vec<CapabilityStatus>,
    pub counts: BTreeMap<String, i64>,
    pub latest_timestamps: BTreeMap<String, Option<String>>,
    pub collector_diagnostics: CollectorDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub collection_enabled: bool,
    pub minimize_to_tray: bool,
    pub launch_at_login: bool,
    pub retention_days: i64,
    pub quota_thresholds: Vec<i64>,
    pub raw_event_retention_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            collection_enabled: true,
            minimize_to_tray: true,
            launch_at_login: false,
            retention_days: 90,
            quota_thresholds: vec![50, 75, 90, 100],
            raw_event_retention_enabled: false,
        }
    }
}

impl Dashboard {
    pub fn empty(codex_version: impl Into<String>) -> Self {
        Self {
            demo_mode: false,
            collector_state: "ready - no telemetry received".to_string(),
            codex_version: codex_version.into(),
            last_event_at: None,
            quota: Vec::new(),
            quota_status: unavailable_status(
                "Codex App Server",
                "No reliable quota snapshot is available.",
                "Read-only App Server collection",
            ),
            forecasts: Vec::new(),
            forecast_status: unavailable_status(
                "Local deterministic forecast",
                "A quota forecast needs at least three compatible observations.",
                "Read-only App Server quota history",
            ),
            account_usage: None,
            account_usage_status: unavailable_status(
                "Codex App Server",
                "No account usage response is available.",
                "Read-only App Server collection",
            ),
            today: TokenTotals {
                input: None,
                cached_input: None,
                output: None,
                reasoning_output: None,
                total: None,
                accuracy: Accuracy::Unavailable,
            },
            token_status: disabled_detail_status("Token composition"),
            turns: Vec::new(),
            turns_status: disabled_detail_status("Completed-turn ranking"),
            burn: BurnAnalysis {
                headline: "Usage burn analysis needs telemetry from at least one completed turn."
                    .to_string(),
                factors: Vec::new(),
                missing_signals: default_missing_signals(),
                method: "No calculation performed.".to_string(),
                confidence: BurnConfidence::Low,
                accuracy: Accuracy::Unavailable,
            },
            burn_status: disabled_detail_status("Usage Burn evidence"),
            channels: vec![
                ChannelHealth {
                    id: "app-server".to_string(),
                    name: "App Server".to_string(),
                    status: ChannelStatus::Inactive,
                    enabled: true,
                    healthy: false,
                    detail: "Waiting for collector startup".to_string(),
                    last_event_at: None,
                    last_successful_collection_at: None,
                    latest_error: None,
                    capabilities: vec!["Quota windows".to_string(), "Account activity".to_string()],
                    telemetry_state: TelemetryState::Waiting,
                    configuration_status: "Configured".to_string(),
                    restart_count: 0,
                },
                ChannelHealth {
                    id: "lifecycle-hooks".to_string(),
                    name: "Lifecycle hooks".to_string(),
                    status: ChannelStatus::Inactive,
                    enabled: false,
                    healthy: false,
                    detail: "Not configured".to_string(),
                    last_event_at: None,
                    last_successful_collection_at: None,
                    latest_error: None,
                    capabilities: vec![
                        "Turn completion".to_string(),
                        "Model and reasoning".to_string(),
                        "Attribution".to_string(),
                    ],
                    telemetry_state: TelemetryState::Disabled,
                    configuration_status: "Not configured".to_string(),
                    restart_count: 0,
                },
                ChannelHealth {
                    id: "opentelemetry".to_string(),
                    name: "OpenTelemetry".to_string(),
                    status: ChannelStatus::Inactive,
                    enabled: false,
                    healthy: false,
                    detail: "Not configured".to_string(),
                    last_event_at: None,
                    last_successful_collection_at: None,
                    latest_error: None,
                    capabilities: vec![
                        "Token composition".to_string(),
                        "Usage Burn evidence".to_string(),
                    ],
                    telemetry_state: TelemetryState::Disabled,
                    configuration_status: "Not configured".to_string(),
                    restart_count: 0,
                },
            ],
            collector_diagnostics: CollectorDiagnostics {
                latest_session: None,
                recent_sessions: Vec::new(),
                recent_errors: Vec::new(),
            },
            model_distribution: Vec::new(),
            reasoning_distribution: Vec::new(),
            model_status: disabled_detail_status("Model and reasoning analytics"),
            alerts: AlertCenter {
                active: Vec::new(),
                dismissed: Vec::new(),
                history: Vec::new(),
            },
            warnings: vec![
                "No telemetry is available. Load the synthetic fixture to explore the interface."
                    .to_string(),
            ],
        }
    }
}

fn unavailable_status(source: &str, reason: &str, integration: &str) -> TelemetryStatus {
    TelemetryStatus {
        state: TelemetryState::Unavailable,
        accuracy: Accuracy::Unavailable,
        source: source.to_string(),
        last_observed_at: None,
        reason: Some(reason.to_string()),
        required_integration: Some(integration.to_string()),
    }
}

fn disabled_detail_status(metric: &str) -> TelemetryStatus {
    TelemetryStatus {
        state: TelemetryState::Disabled,
        accuracy: Accuracy::Unavailable,
        source: "Lifecycle hooks or OpenTelemetry".to_string(),
        last_observed_at: None,
        reason: Some(format!(
            "{metric} is unavailable because detailed turn telemetry is not configured."
        )),
        required_integration: Some(
            "Enable lifecycle hooks or the local OpenTelemetry receiver".to_string(),
        ),
    }
}

fn default_missing_signals() -> Vec<MissingSignal> {
    vec![
        MissingSignal {
            name: "Token usage".to_string(),
            source: "Lifecycle hooks or OpenTelemetry".to_string(),
            setup_state: TelemetryState::Disabled,
            detail: "Detailed turn telemetry is not configured.".to_string(),
        },
        MissingSignal {
            name: "Quota snapshots".to_string(),
            source: "Codex App Server".to_string(),
            setup_state: TelemetryState::Unavailable,
            detail: "No reliable quota snapshot is available.".to_string(),
        },
        MissingSignal {
            name: "Completed turns".to_string(),
            source: "Lifecycle hooks or OpenTelemetry".to_string(),
            setup_state: TelemetryState::Disabled,
            detail: "No completed turn event is available.".to_string(),
        },
    ]
}
