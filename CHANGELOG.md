# Changelog

All notable changes to Codex Meter will be recorded here.

## Unreleased

### Added

- Professional operational dashboard with a priority quota-risk hero, KPI strip,
  time-proportional uPlot charts, compact source-health grid, responsive states,
  keyboard focus treatment, and explicit Live/Demo presentation.
- Deterministic quota analytics over reset-segmented history: interval and
  rolling burn rates, safe pace, pace ratio, EWMA, ordinary least squares,
  exhaustion estimate, projected reset usage, confidence bounds, and forecast
  quality metadata.
- SQLite migration v4 for forecast history/evaluation and unread/resolved
  forecast-alert lifecycle fields.
- Dedicated Forecast view and separate account token-activity chart; quota
  percentages and token totals remain separate units.
- Local forecast-risk alerts for unsafe pace and predicted exhaustion, including
  deduplication, read state, automatic resolution, and retained history.
- Forecast and alert records in normalized CSV export; the JSON dashboard
  envelope includes the full current forecast view model.
- Shared `live`, `waiting`, `disabled`, `unsupported`, `unavailable`, `error`,
  and `fixture` telemetry-state contract across Rust and TypeScript.
- Migration-backed quota alert center with active, dismissed, and history views,
  fixed severity bands, timestamps, accuracy, reset identity, and persistent
  bucket/reset/threshold deduplication.
- Source last-success tracking, capability summaries, and historical-error
  demotion after collector recovery.
- Navigation notification indicator, prominent 75%+ Overview alerts, and
  critical quota text in the tray tooltip.
- Tauri 2/Svelte desktop scaffold with tray and user-controlled autostart surfaces.
- Versioned normalized SQLite schema, deterministic fixture ingestion, retention, deletion, and persistence tests.
- Overview, Usage Burn, data, diagnostics, and settings views with accuracy badges and honest partial states.
- JSON, CSV, and redacted diagnostic exports.
- App Server JSON-RPC primitives and 0.144.1 generated schema bundles.
- A bounded native App Server supervisor with initialization, read-only account polling, typed notifications, timeout handling, pause/resume, restart backoff, and shutdown.
- Live quota and account-usage persistence with a second ordered SQLite migration, dynamic buckets, reset-unit interpretation, duplicate protection, and restart-safe history.
- Native collector diagnostics, automatic UI refresh, live countdowns, and exact-data-only quota-threshold events with persistent deduplication.
- Verified unsigned Windows executable, NSIS installer, and MSI installer output.
- Allowlist-based hook and OTLP JSON adapters with privacy and malformed-input tests.
- Optional uninstalled Codex plugin/hook package and OpenTelemetry configuration preview.
- Frontend unit coverage, Overview browser smoke, CI/security/release workflows, and public project documentation.

### Changed

- Navigation now follows the operational order Overview, Forecast, Usage Burn,
  Projects, Chats, Turns, Models, History, Alerts, Diagnostics, Settings.
- Diagnostics redacts the application-data root by default and reveals the full
  resolved database path only through an explicit copy action.
- Missing token telemetry is now nullable; exact zero is rendered only when a
  verified record explicitly reports zero.
- Overview is ordered by quota risk, active alerts, quota windows, account
  activity, source health, then detailed turn analytics.
- Disabled detailed telemetry panels collapse into one concise integration card.
- Fixture import was replaced by explicit, non-persistent Live/Demo switching;
  legacy persisted fixture state is removed during migration startup.
- Overview and Diagnostics now consume normalized native collector state instead of remaining fixture-only.
- Windows Codex discovery uses the npm-installed JavaScript launcher when the packaged Store executable cannot be spawned directly.

### Notes

- The loopback OpenTelemetry receiver, hook spool replay, Windows toast
  notifications, and native export destination picker remain incomplete.
- Windows toasts were not added because this dependency-constrained slice would
  require a new Tauri notification plugin and reviewed Windows identity setup.
- Token notifications are supported and tested with deterministic payloads but were not forced during live validation.
