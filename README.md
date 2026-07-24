# Codex Meter

Codex Meter is an **experimental, local-first Windows desktop application** that records, preserves, explains, and visualizes telemetry exposed by Codex. It keeps history after transient quota indicators disappear and labels every metric by provenance.

> **Codex Meter is an independent open-source project and is not affiliated with, endorsed by, or supported by OpenAI. Codex interfaces and telemetry fields may change between versions.**

The native Windows slice uses Tauri 2, Svelte/TypeScript, Rust, and SQLite. It supervises a local Codex App Server, performs only verified read-only account calls, persists live quota and usage history, and keeps synthetic Demo data in memory and outside the live database. It also includes deterministic local quota forecasting, persistent threshold and forecast-risk alerts, Usage Burn analysis, local settings, JSON/CSV/redacted diagnostic exports, delete-local-data support, tray lifecycle behavior, version-specific App Server schemas, and optional uninstalled hook/OpenTelemetry integration assets.

## Why this project exists

Codex can expose quota percentages, token usage, model settings, and lifecycle signals through different interfaces. Those signals are incomplete, version-sensitive, and easy to misinterpret. Codex Meter creates a local historical record while keeping reported facts separate from calculations and estimates.

## Telemetry classifications

| Badge            | Meaning                                                     |
| ---------------- | ----------------------------------------------------------- |
| `reported_exact` | Directly emitted or returned by a verified Codex interface. |
| `derived_exact`  | Calculated only from reported exact values.                 |
| `estimated`      | Inferred by a documented correlation or method.             |
| `unavailable`    | Not exposed reliably by the installed Codex version.        |

Account-level quota movement is never presented as exact project, chat, or turn consumption. Configured reasoning effort, reasoning-output tokens, quota movement, and API-equivalent cost remain distinct concepts. Codex Meter does not implement a generic complexity score.

## Telemetry states

Every telemetry-dependent panel uses one shared state model:

| State         | Meaning                                                               |
| ------------- | --------------------------------------------------------------------- |
| `live`        | Verified data is currently being received.                            |
| `waiting`     | The source is enabled and healthy, but no relevant event has arrived. |
| `disabled`    | The required collection source is not enabled.                        |
| `unsupported` | The installed Codex version does not expose the capability.           |
| `unavailable` | No reliable value can currently be calculated.                        |
| `error`       | Collection was attempted and failed.                                  |
| `fixture`     | Clearly labeled, synthetic Demo data is displayed.                    |

Missing numeric telemetry is `null`, never zero. A displayed zero is valid only
when a verified source explicitly reports zero.

## Current functionality

- Risk-first Overview with prominent warning/critical alerts, quota windows,
  a highest-priority quota outlook, consumption trajectory, account activity,
  detailed source health, and compact turn analytics.
- Dedicated Forecast view with time-proportional quota and burn-rate charts,
  reset/exhaustion markers, 30-minute through 6-hour rolling rates, EWMA and
  ordinary-least-squares estimates, confidence ranges, and quality diagnostics.
- Persistent alert center with unread, active, dismissed, resolved, and retained
  history views; stable deduplication uses bucket, reset window, and alert type.
- Accessible quota severity bands: neutral below 50%, informational at 50–74%,
  warning at 75–89%, critical at 90–99%, and exhausted at 100%.
- Usage Burn view with ranked evidence, method, confidence, missing signals, and correlation-only language.
- Forecast, Usage Burn, Projects, Chats, Turns, Models, History, Alerts,
  Diagnostics, and Settings navigation with typed empty/partial states.
- Source Health reports enablement, current health, last event, last successful
  collection, timestamped current or historical errors, and supplied capabilities.
- Explicit Live/Demo selector. Demo data is visually marked on every view, never
  persisted or exported, and never emits native notifications.
- Versioned SQLite migrations covering normalized telemetry, forecast history
  and evaluation, alert lifecycle state, source recovery timestamps, and local
  controls.
- Bounded Codex App Server supervision over newline-delimited stdio JSON-RPC with initialization, typed response/notification handling, timeouts, pause/resume, restart backoff, and clean shutdown.
- Live `account/rateLimits/read` and `account/usage/read` collection with dynamically discovered quota buckets, defensive reset-time normalization, and exact account usage summaries/daily buckets.
- Deterministic hook and OTLP JSON adapters that discard content fields and reject malformed/oversized input.
- Deterministic in-memory Demo data that restores the live dashboard immediately
  when Live mode is selected.
- Normalized JSON, CSV, and redacted diagnostic export; explicit local analytics deletion and retention settings.
- Tauri tray menu and verified hide-to-tray lifecycle; autostart is disabled by default.
- In-app quota-threshold events at 50%, 75%, 90%, and 100%, persisted and
  deduplicated by bucket, reset window, and threshold and emitted only for
  `reported_exact` data.
- Locally estimated forecast-risk alerts for unsafe pace and likely exhaustion,
  with confidence and predicted time shown explicitly and resolved when the
  condition clears.
- App Server JSON-RPC parsing, correlation, timeout/restart policy, and generated 0.144.1 schemas.

The authenticated loopback OpenTelemetry receiver remains unimplemented and disabled. The optional plugin is not installed or trusted.

## Screenshots

The browser smoke path exercises empty and synthetic states. The native Windows executable and both installer formats have been built and launched locally; no publication screenshot is included yet.

## Platform and architecture

- Primary platform: Windows 10 and Windows 11.
- Desktop: Tauri 2 with system tray and optional user-enabled autostart.
- Presentation: Svelte 5, strict TypeScript, and Vite.
- Native layer: Rust with embedded, versioned SQLite migrations.
- Integrations: capability-gated App Server, hook, and OpenTelemetry adapters.
- Backend: none. No hosted service or third-party analytics.

The Svelte layer receives normalized view models only. External protocol validation, redaction, normalization, persistence, analytics, and exports remain in separate native boundaries. See [HLD](docs/HLD.md), [LLD](docs/LLD.md), and the [ADRs](docs/adr/).

## Data collected

When a verified source supplies it, Codex Meter may store identifiers and metadata for source events, threads, turns, projects/worktrees, model and reasoning configuration, token breakdowns, quota snapshots, durations, tool categories/counts, compactions, subagents, retries, and collection errors. Runtime project paths stay only in the local database.

Codex Meter deliberately does **not** collect full prompts, responses, transcript content, tool arguments, command text/output, patches, repository source, credentials, authentication material, or environment-variable values. Raw payload retention is disabled.

## Local data paths

Tauri resolves the per-user application-data directory at runtime. With the current identifier on Windows, the SQLite database is `%APPDATA%\dev.codexmeter.app\codex-meter.sqlite` and generated exports are stored below its `exports` directory. The optional hook bridge would spool normalized events below `%LOCALAPPDATA%\CodexMeter\spool` while the app is unavailable. Runtime databases, spools, exports, logs, and secrets are ignored by Git.

## Development setup

Prerequisites:

- Node.js 24 or newer;
- pnpm 11.9.0;
- Rust stable with the MSVC target and Windows C++ build tools;
- WebView2 (normally present on supported Windows releases).

```powershell
pnpm install
pnpm dev
```

Run the native app when Rust/MSVC is available:

```powershell
pnpm tauri dev
```

Build production assets and Windows installers:

```powershell
pnpm build
pnpm tauri build
```

Expected installers are emitted below `src-tauri/target/release/bundle/` after a successful native build.

## Tests and quality gates

```powershell
pnpm format:check
pnpm lint
pnpm check
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo check --manifest-path src-tauri/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-features
pnpm test:ui
```

All tests are deterministic and must not start a model turn or consume real quota. See [Testing](docs/TESTING.md).

## Plugin and hooks setup

The optional package at `integrations/codex-plugin/` follows the installed plugin layout and uses stable 0.144.1 hook events. It is intentionally not installed or trusted automatically.

1. Review `.codex-plugin/plugin.json`, `hooks/hooks.json`, and `scripts/emit-hook.ps1`.
2. Copy the package into a local marketplace or plugin development location using the installed Codex plugin workflow.
3. Add it from that local marketplace with `codex plugin add codex-plugin@<local-marketplace-name>`.
4. Explicitly review and trust the hook definition in Codex; installation alone does not trust hooks.
5. Start a new Codex task so newly installed plugin components are discovered.

The bridge is fail-open, emits no successful-path output, and spools only normalized allowlisted metadata when Codex Meter is closed. See [Codex integration](docs/CODEX_INTEGRATION.md) for update, verification, and removal guidance.

## OpenTelemetry setup

The example at `integrations/otel/codex-config.example.toml` is for the **user-level** `~/.codex/config.toml`; Codex ignores `otel` in project-local configuration. Keep `log_user_prompt = false`. Do not enable the snippet until Diagnostics reports a running `127.0.0.1` receiver and Codex Meter has generated its local secret. The receiver is not implemented in this first slice, so the shipped snippet is documentation/configuration generation only.

Codex Meter never modifies the user's Codex configuration automatically. Remove the `[otel]` section to disable export.

## Export

Settings can generate normalized JSON, CSV, or redacted diagnostics under the local application-data export directory. Exports omit content and credentials. Schemas are documented in [Export formats](docs/EXPORT_FORMATS.md).

## Tested Codex version and capabilities

The local investigation and live read-only probe used `codex-cli 0.144.1`. Generated bindings and JSON Schemas are stored under `schemas/`. `account/rateLimits/read` and `account/usage/read` were observed live. The collector accepts `account/rateLimits/updated` and `thread/tokenUsage/updated`, but no token notification was forced or observed because validation did not start a model turn. Exact per-project quota consumption and monetary cost are unavailable. See the [capability matrix](docs/TELEMETRY_CAPABILITY_MATRIX.md).

## Known limitations

- Forecasts are local deterministic estimates over reported percentage history,
  not Codex predictions. They are withheld when sample count, elapsed coverage,
  reset identity, or timestamp quality is insufficient.
- Forecast segmentation uses bucket/reset identity plus detected drops and long
  gaps. Account identity is not segmented because the verified rate-limit
  response does not expose a stable account identifier.
- A separately launched App Server was verified for account reads, but it may not observe token events produced by unrelated Codex desktop sessions.
- The authenticated loopback OpenTelemetry listener, hook spool replay, and plugin validator remain deferred.
- Quota reset units are not documented in the schema. Values in a plausible seconds or milliseconds range are normalized and labeled; ambiguous values remain raw and are not shown as exact times.
- Quota-threshold notifications are currently accessible in-app alerts, not
  Windows toast notifications. Native toast delivery requires adding and
  reviewing the Tauri notification plugin plus Windows application-identity
  behavior; this change added no dependency.
- Export destination is the fixed per-user `exports` directory; a native destination picker is not implemented.
- Diagnostics intentionally reports no log path because file logging is not configured.
- Detailed historical token usage cannot be backfilled if live notifications were missed.
- The tray menu's Open, Settings, and Exit clicks and autostart enable/disable toggles were not UI-automated; hide-on-close and restoration were exercised at the native-window level.

See [Troubleshooting](docs/TROUBLESHOOTING.md) before reporting a compatibility problem.

## Contributing and security

Read [CONTRIBUTING.md](CONTRIBUTING.md), use an ExecPlan for substantial changes, update the capability matrix for telemetry changes, and include exact validation results. Use synthetic fixtures only.

Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md). Never include credentials, prompts, responses, repository contents, or unredacted diagnostics in a public issue.

Licensed under the [Apache License 2.0](LICENSE).
