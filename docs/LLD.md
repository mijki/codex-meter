# Low-Level Design

## Frontend model

The UI works against these types from `src/lib/types.ts`:

- `Accuracy`
- `QuotaWindow`
- `TokenTotals`
- `ExpensiveTurn`
- `BurnAnalysis`
- `ChannelHealth`
- `Dashboard`
- `AccountUsageSummary`
- `CollectorDiagnostics`
- `AppSettings`
- `TelemetryState` / `TelemetryStatus`
- `QuotaAlert` / `AlertCenter`
- `QuotaForecast` / `ForecastQuality` / `ConsumptionTrajectoryPoint`
- `BurnRatePoint` / `ForecastRisk` / `ForecastConfidence`

Accuracy is always one of:

- `reported_exact`
- `derived_exact`
- `estimated`
- `unavailable`

Telemetry state is always one of `live`, `waiting`, `disabled`, `unsupported`,
`unavailable`, `error`, or `fixture`. `TokenTotals` fields are nullable so an
absent value cannot be confused with an explicitly reported zero.

## Frontend behavior

- `src/App.svelte` drives navigation, Live/Demo presentation mode, alert
  dismissal, exports, settings persistence, and local deletion.
- `Overview.svelte`, `ForecastView.svelte`, `TimeSeriesChart.svelte`,
  `TelemetryState.svelte`, `QuotaAlertCard.svelte`, and `AlertCenter.svelte`
  render the shared operational, chart, state, and alert contracts.
- `QuotaCard.svelte` renders used/remaining quota, local reset time, and a one-second live countdown.
- `AccuracyBadge.svelte` displays the visible accuracy label.
- `src/lib/api.ts` uses browser fallbacks outside Tauri and invokes desktop commands inside Tauri.
- `src/lib/fixture.ts` supplies the synthetic dashboard state used by the demo mode.
- Tauri `collector-status` events trigger normalized dashboard/diagnostics refreshes. `quota-threshold` events display exact-data-only in-app notices.

## Native and storage contract

`src-tauri/migrations/0001_initial.sql` defines the normalized base model.
`0002_live_account_telemetry.sql` adds raw reset interpretation and account
usage. `0003_alerts_and_source_health.sql` adds alert history/dismissal and
source last-success timestamps. `0004_quota_forecasts.sql` adds forecast
history/evaluation and unread/resolved alert metadata. Embedded migrations
execute transactionally, in numeric order, and only once. The important tables
are:

| Table                                             | Purpose                                                        |
| ------------------------------------------------- | -------------------------------------------------------------- |
| `app_metadata`                                    | Store local key/value metadata.                                |
| `codex_installations`                             | Record detected Codex installations.                           |
| `capabilities`                                    | Track which metrics are available and how they are classified. |
| `collector_sessions`                              | Track collector runs and restarts.                             |
| `sources`                                         | Track source health and enablement.                            |
| `projects` / `repository_locations` / `worktrees` | Normalize project identity and workspace paths.                |
| `threads` / `turns`                               | Store conversation and turn identity plus accuracy labels.     |
| `models` / `reasoning_settings`                   | Normalize model and reasoning metadata.                        |
| `hook_events` / `otel_events` / `usage_records`   | Store normalized telemetry events.                             |
| `quota_buckets` / `quota_snapshots`               | Store quota state over time.                                   |
| `account_usage_snapshots` / `account_usage_daily` | Store exact account summaries and dated usage buckets.         |
| `burn_analyses`                                   | Store calculated burn summaries.                               |
| `collection_errors`                               | Store redacted collection failures.                            |
| `alert_history`                                   | Persist quota alerts and dismissal history.                    |
| `quota_forecast_history`                          | Persist model inputs/outcomes for later evaluation.            |
| `settings`                                        | Store user preferences.                                        |
| `exports`                                         | Track export runs.                                             |

## Command surface seen by the UI

The frontend currently expects these Tauri commands:

- `get_dashboard`
- `dismiss_alert`
- `mark_alerts_read`
- `get_resolved_database_path`
- `get_settings`
- `get_collector_diagnostics`
- `save_settings`
- `export_data`
- `delete_local_data`

All listed commands have native handlers. Demo mode needs no native command:
the deterministic fixture is held only in frontend memory. Live account
ingestion is performed by the collector event bridge rather than by exposing
raw protocol messages to Svelte.

## Collector runtime

`src-tauri/src/collector.rs` owns the child process, stdin/stdout threads, and typed runtime events. `src-tauri/src/app_server.rs` owns protocol parsing, monotonic request correlation, health transitions, deadlines, bounded exponential restart backoff with jitter, pause/resume, and graceful shutdown actions. `src-tauri/src/forecast.rs` remains protocol-independent and accepts only normalized quota observations.

The initialization sequence is:

1. spawn the npm Codex launcher through `node` on Windows, falling back to `codex`;
2. send `initialize`, wait for the correlated response, and send `initialized`;
3. issue `account/rateLimits/read` and `account/usage/read`;
4. normalize and persist responses/notifications in the collector event bridge;
5. emit typed Tauri status/threshold events.

The frontend never parses App Server JSON. No runtime path starts a thread or turn.

## Forecasting contract

- Observations are selected from the current bucket/reset identity and ordered by
  receipt time.
- An interval rate is `delta used percentage / elapsed hours`; rolling
  30-minute, 1-hour, 3-hour, and 6-hour rates use the same actual-time basis.
- The candidate set is the latest interval, complete-segment rate, EWMA
  (`alpha = 0.35`), and ordinary least squares. Quality considers sample count,
  coverage, polling regularity, largest gap, slope stability, and model
  agreement.
- At least three observations and 15 minutes of coverage are required. Usage
  drops greater than two percentage points or gaps over 120 minutes invalidate
  the earlier segment.
- Safe rate is remaining percentage divided by time until reset. Pace ratio is
  selected rate divided by safe rate. Forecast timestamps and confidence ranges
  are `estimated`; reported points and exact arithmetic retain their own labels.

## Runtime storage and exports

The database is created under Tauri's per-user app-data directory; on the validated Windows runtime it resolves to `%APPDATA%\dev.codexmeter.app\codex-meter.sqlite`. JSON, CSV, and redacted diagnostic files are written as UTF-8 under the adjacent `exports` directory. The current export command uses this fixed destination and returns filesystem errors; a destination picker is deferred.

## Demo contract

The synthetic fixture is intentionally explicit:

- every telemetry state is `fixture`;
- quota data is synthetic but keeps its demonstrated accuracy classification;
- token totals, turn identity, and source health are synthetic values;
- the burn analysis is marked estimated;
- the global `DEMO DATA` marker remains visible across every view;
- fixture values are not stored, exported, or sent to native notifications;
- selecting Live reloads the current native dashboard immediately.

## Schema evidence

The generated Codex App Server schema bundle under `schemas/` proves the following interfaces are present in Codex CLI 0.144.1:

- `account/rateLimits/read`
- `account/usage/read`
- `account/rateLimits/updated`
- `thread/tokenUsage/updated`

Do not extend the low-level design beyond that evidence without re-checking the generated bundle for the installed CLI version.
