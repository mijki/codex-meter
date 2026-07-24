# Execution Plan: professional quota-risk dashboard

## Forecasting and operational-dashboard continuation (2026-07-24)

### Objective and user-visible outcome

Transform the verified native MVP into a risk-first operational dashboard that
answers whether the active account quota window is on a sustainable pace. The
interface must keep reported quota observations, exact arithmetic, and local
forecasts visually and semantically distinct.

The smallest complete vertical slice is:

`segmented quota history -> deterministic burn analytics -> persisted forecast ->
quota-risk hero and trajectory -> forecast-risk alerts -> dedicated Forecast view`

The slice preserves the existing App Server collector, SQLite history, tray
lifecycle, export controls, explicit Live/Demo mode, and detailed-turn
integration states.

### Verified constraints and current discoveries

- Git is clean on `feat/professional-dashboard-ux`; `main` and the remote
  feature branch still point to the baseline commit. Changes must remain
  unstaged and uncommitted.
- The repository already implements the seven-state telemetry contract,
  nullable token totals, exact quota-threshold alerts, alert dismissal/history,
  source recovery semantics, and in-memory Demo mode.
- Existing SQLite schema version 3 has quota snapshots and threshold alerts but
  no analytical window, forecast-history, forecast-evaluation, alert
  resolution/unread, or forecast-risk persistence.
- Quota snapshots contain reported used percentage and reset metadata. They do
  not expose quota units or token capacity, so all rates are percentage points
  per hour and the predicted event is quota exhaustion, never token exhaustion.
- Historical snapshots may be irregular and may span resets. Forecasting must
  segment on bucket/reset/window identity and defensive discontinuities before
  calculating a slope.
- The frontend has no chart dependency. A small uPlot adapter will be used for
  time-proportional charts instead of a bespoke rendering engine; chart data
  remains typed and accessible outside the canvas.
- Native Windows toast delivery remains deferred because it requires a separate
  notification/plugin and Windows identity review. Persistent in-app and tray
  risk communication remain in scope.

### Accuracy and privacy decisions

- Historical used percentages are `reported_exact`.
- Remaining percentage, interval/rolling rates, safe rate, and complete-window
  rate are `derived_exact` calculations whose interpretation is explicitly
  local.
- EWMA, regression, selected modeled rate, pace ratio based on that model,
  exhaustion time, projected usage at reset, confidence, and risk assessment
  are `estimated`.
- Forecasts with fewer than three valid observations, less than 15 minutes of
  coverage, non-positive/unstable selected rates, expired reset horizons, or
  ambiguous boundaries are unavailable rather than assigned an arbitrary date.
- Demo forecasts stay in frontend memory and cannot be inserted into live
  forecast or alert tables.
- User-facing diagnostics show environment notation. Full resolved paths remain
  available only through an explicit copy action and are excluded from redacted
  diagnostics.

### Milestones

- [x] Verify Git branch/status/log/diff and read the required repository,
      product, integration, implementation, migration, and test material.
- [x] Record forecasting, segmentation, chart, alert, and accuracy decisions in
      the living plan and ADR.
- [x] Add versioned forecast/alert persistence and deterministic native
      analytics with regular, irregular, reset, gap, confidence, and evaluation
      tests.
- [x] Extend the shared native/frontend contract with quota series, burn-rate
      windows, forecast quality, risk state, and alert lifecycle metadata.
- [x] Rebuild Overview around an operational header, quota-risk hero, active
      alerts, trajectory, compact KPI strip, secondary account activity, source
      health, and one detailed-integration card.
- [x] Add Forecast navigation, bucket/range controls, consumption and burn-rate
      charts, forecast-quality detail, and a separate token-activity chart.
- [x] Extend alert generation for pace/forecast risk, unread, dismissal,
      resolution, reset-window deduplication, and Demo suppression.
- [x] Redact displayed diagnostics paths, finish keyboard/focus/reduced-motion
      behavior, and verify supported desktop breakpoints.
- [x] Extend Vitest and Playwright coverage and pass frontend, Rust, UI,
      migration, privacy, and unsigned executable/NSIS gates. Native AppData
      runtime and MSI completion were externally blocked and are recorded below.
- [x] Update README, changelog, product/design/integration/testing/export/
      troubleshooting/capability documentation with exact validation results
      and remaining limitations.

### Validation log

| Command / inspection                  | Result                                                                                                      |
| ------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `git status --short`                  | Clean; no staged or unstaged changes before this continuation.                                              |
| `git branch --show-current`           | `feat/professional-dashboard-ux`.                                                                           |
| `git log --oneline --decorate -n 5`   | Baseline `d5d59fb`; local feature, main, and remote refs aligned.                                           |
| `git diff --check`                    | Passed before implementation.                                                                               |
| Required docs/source/migration review | Completed; forecast analytics and persistence confirmed absent.                                             |
| Existing chart dependency inspection  | None present in the locked frontend dependency graph.                                                       |
| `pnpm install --frozen-lockfile`      | Passed; locked graph already current.                                                                       |
| `pnpm format:check` / `pnpm lint`     | Passed.                                                                                                     |
| `pnpm check`                          | Passed: 0 errors, 0 warnings.                                                                               |
| `pnpm test`                           | Passed: 24 tests.                                                                                           |
| `pnpm build`                          | Passed: 126 modules.                                                                                        |
| `pnpm test:ui`                        | Passed: 1 Chromium operational-dashboard smoke test.                                                        |
| Rust format/check/Clippy              | Passed; Clippy warnings denied.                                                                             |
| `cargo test ... --all-features`       | Passed: 50 tests.                                                                                           |
| Visual verdict iteration 2            | Passed heuristic category review at 93/100; no reference supplied.                                          |
| `pnpm tauri dev`                      | Compiled; sandbox denied AppData SQLite write. Elevated retry was rejected by the automatic usage reviewer. |
| `pnpm tauri build`                    | Fresh executable and NSIS built; WiX MSI bundling blocked by sandbox.                                       |
| Authenticode                          | Fresh executable and NSIS both `NotSigned`, as expected.                                                    |

### Deferred work and explicit non-goals

- No external forecasting service, language model, statistical claim of
  calibrated probability, exact quota units, or exact project/chat/turn quota
  attribution.
- Native Windows toast notifications remain deferred; this slice documents the
  dependency and identity limitation while preserving professional in-app and
  tray alerts.
- Forecast evaluation is recorded only for windows whose outcome was actually
  observed. Interrupted collection is not scored.
- Collector-health changes and material forecast-confidence changes are shown
  in their dedicated views but do not yet create persistent alert-history rows.

## Telemetry-state and alert continuation (2026-07-23)

### Objective and user-visible outcome

Make every telemetry-dependent surface distinguish verified values from waiting,
disabled, unsupported, unavailable, failed, and synthetic states. Missing values
must remain `null`; an exact zero is rendered only when a verified source reports
zero.

The smallest complete vertical slice is:

`shared metric state -> corrected Overview panels -> persisted quota alert history -> recoverable source health -> non-persistent Demo mode`

The Overview prioritizes quota risk, persistent active alerts, quota windows,
account activity, source health, and compact detailed analytics. No forecasting
or burn-rate prediction is introduced.

### Verified constraints and current discoveries

- The entire repository remains untracked on `main`; all files are treated as
  user-authored and nothing may be staged or committed.
- Live App Server account collection, local SQLite history, tray lifecycle, and
  unsigned Windows packaging passed in the preceding continuation.
- `TokenTotals` currently uses numeric zero even when accuracy is unavailable.
- The exposed synthetic-fixture command currently clears live analytics and
  persists fixture rows; the new Demo mode must instead be an in-memory
  presentation choice so Live mode can be restored immediately.
- `notification_state` deduplicates threshold events but has no user-visible
  alert history or dismissal state.
- `sources` lacks a last-success timestamp and startup seeding currently resets
  existing source status.
- Collection errors are preserved correctly for Diagnostics, but Source Health
  does not distinguish a recovered historical error from a current failure.
- Native Windows toast delivery would require adding and configuring a Tauri
  notification dependency. This task prohibits installing dependencies, so the
  complete slice retains accessible in-app alerts and documents the exact
  blocker.

### Milestones

- [x] Read repository guidance, product/design documents, relevant ADRs, source,
      migrations, and existing tests.
- [x] Record the state-model, alert-persistence, source-recovery, and Demo-mode
      decisions.
- [x] Add shared native/frontend telemetry-state, metric metadata, alert, and
      source-health contracts.
- [x] Add migration-backed alert history/dismissal and source last-success data.
- [x] Correct Overview hierarchy and all detailed-telemetry empty states.
- [x] Replace fixture import with explicit, non-persistent Live/Demo switching.
- [x] Add deterministic Rust, frontend, and Playwright coverage.
- [x] Update README, changelog, product/design/testing/troubleshooting/capability
      documentation.
- [x] Pass all frontend, Rust, UI, native runtime, and unsigned packaging gates.

### Validation log

| Command / inspection                                     | Result                                                                                     |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `git status --short --branch`                            | Entire repository untracked on `main`; nothing staged.                                     |
| Required document/source inspection                      | Completed before implementation.                                                           |
| `pnpm format:check`                                      | Passed.                                                                                    |
| `pnpm lint`                                              | Passed.                                                                                    |
| `pnpm check`                                             | Passed with 0 errors and 0 warnings.                                                       |
| `pnpm test`                                              | Passed: 20 tests.                                                                          |
| `pnpm build`                                             | Passed.                                                                                    |
| `pnpm test:ui`                                           | Passed: 1 Chromium smoke test.                                                             |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | Passed with the installed per-user Cargo executable.                                       |
| `cargo check --manifest-path src-tauri/Cargo.toml ...`   | Passed for all targets and features.                                                       |
| `cargo clippy --manifest-path src-tauri/Cargo.toml ...`  | Passed for all targets/features with warnings denied.                                      |
| `cargo test --manifest-path src-tauri/Cargo.toml ...`    | Passed: 43 tests.                                                                          |
| Heuristic browser visual inspection                      | Live unavailable and Demo warning/critical layouts inspected; no reference image supplied. |
| `pnpm tauri dev`                                         | Native window launched; v3 database and read-only live collector reached healthy.          |
| Read-only live database inspection                       | App Server source healthy; fresh rate-limit/account-usage snapshots present.               |
| Isolated `pnpm tauri build`                              | Passed; optimized executable plus unsigned NSIS and MSI bundles produced.                  |
| `Get-AuthenticodeSignature`                              | Executable, NSIS installer, and MSI all reported `NotSigned`.                              |

### Deferred work and explicit non-goals

- Forecasting, quota-unit inference, and burn-rate prediction are out of scope.
- Native Windows toast delivery is deferred until a notification dependency and
  Windows identity behavior can be reviewed and added in a dependency-authorized
  change.
- Demo telemetry is never exported or written into the live database.

---

# Historical plan: first functional Codex Meter vertical slice

## Native continuation (2026-07-23)

### Objective and user-visible outcome

Convert the fixture-first slice into the strongest environment-supported native
Windows build without changing the product scope: execute the Rust and SQLite
paths, supervise a read-only Codex App Server process, persist normalized account
telemetry, connect typed native state to the existing UI, validate desktop
lifecycle and browser smoke behavior, and produce unsigned local bundle artifacts
where the installed Windows toolchain permits it.

The smallest complete vertical slice for this continuation is:

`spawn and initialize App Server -> read account rate limits and account usage -> normalize and persist snapshots -> expose typed dashboard/diagnostics state -> stop the child cleanly`

This slice must never create a Codex thread or turn, send a prompt, edit global
Codex configuration, install/trust the optional plugin, or consume quota
intentionally.

### Verified continuation constraints and source versions

- The complete working tree is currently untracked on branch `main`, so existing files must be treated as
  user-authored and nothing may be staged or committed.
- Node.js: `v26.4.0`.
- pnpm: `11.9.0`.
- Codex CLI: `codex-cli 0.144.1`.
- `codex app-server --help` confirms stdio is the default transport and exposes
  `generate-ts` and `generate-json-schema`.
- Rust toolchain: `stable-x86_64-pc-windows-msvc` with target
  `x86_64-pc-windows-msvc`.
- rustc: `1.97.1 (8bab26f4f 2026-07-14)`, LLVM `22.1.6`.
- cargo: `1.97.1 (c980f4866 2026-06-30)`.
- Rust is installed under the per-user Cargo directory but that directory is
  not present on the current process `PATH`; native commands must use the
  absolute binaries or a command-local `PATH` addition.
- Windows SDK versions `10.0.19041.0`, `10.0.22621.0`, and `10.0.26100.0` are
  present; the 10.0.26100.0 x64 `kernel32.lib` and `ucrt.lib` were verified.
- Visual Studio Installer `vswhere.exe`, `cl.exe`, and `link.exe` are not on the
  interactive shell path. With the Rust directory added to the command-local
  path, Tauri detects Visual Studio Build Tools 2019 and Cargo compiles the full
  Tauri/WebView dependency graph. Debug and optimized linking both passed.
- The locked pnpm graph is restored. Tauri CLI `2.11.4` and Playwright `1.61.1`
  are available locally.

### Continuation milestones

- [x] Read all required repository guidance and product/integration documents.
- [x] Inspect Git, Node, pnpm, Rust, Codex, Windows SDK, and native compiler state.
- [x] Restore the locked JavaScript dependency graph.
- [x] Pass frontend format, lint, type, unit-test, build, and Playwright gates.
- [x] Pass Rust format, check, Clippy, and test gates.
- [x] Execute SQLite migrations and persistence tests against temporary databases.
- [x] Implement and test bounded App Server supervision and read-only account collection.
- [x] Connect collector, diagnostics, settings, notifications, and exports to typed Tauri commands/events.
- [x] Validate native window/tray lifecycle and clean child-process shutdown to the environment-supported extent.
- [x] Produce and launch unsigned executable/MSI/NSIS outputs.
- [x] Update public documentation from observed runtime behavior and complete a privacy/security scan.

### Continuation validation log

| Command / inspection                       | Observed result                                                                     |
| ------------------------------------------ | ----------------------------------------------------------------------------------- |
| `git status --short`                       | Entire repository is untracked; nothing staged.                                     |
| `git diff --check`                         | Passed on the current tracked diff (no output).                                     |
| `node --version`                           | `v26.4.0`.                                                                          |
| `pnpm --version`                           | `11.9.0`.                                                                           |
| `rustup show` via installed absolute path  | Active/default `stable-x86_64-pc-windows-msvc`; installed target matches.           |
| `rustc --version --verbose`                | `1.97.1`, host `x86_64-pc-windows-msvc`, LLVM `22.1.6`.                             |
| `cargo --version`                          | `1.97.1`.                                                                           |
| `codex --version`                          | `codex-cli 0.144.1`.                                                                |
| `codex app-server --help`                  | Passed; stdio default verified.                                                     |
| `pnpm install --frozen-lockfile`           | Passed from the locked graph; no dependency changes.                                |
| local `tauri info` with Rust on `PATH`     | Passed; Tauri 2.11.4, WebView2 150.0.4078.83, Build Tools 2019.                     |
| local `playwright --version`               | `1.61.1`.                                                                           |
| Visual Studio / MSVC inspection            | SDK libraries present; Tauri detects Build Tools 2019.                              |
| `pnpm lint`                                | Passed.                                                                             |
| `pnpm check`                               | Passed with 0 errors and 0 warnings.                                                |
| `pnpm test`                                | Passed: 5 tests.                                                                    |
| `pnpm build`                               | Passed.                                                                             |
| `cargo check --all-targets --all-features` | Passed after fixing native type/error/lifetime defects.                             |
| `cargo fmt --all -- --check`               | Passed.                                                                             |
| `cargo clippy ... -- -D warnings`          | Passed with no warnings.                                                            |
| `cargo test --all-features`                | Passed: 40 tests after live persistence/notification/export additions.              |
| Temporary SQLite migration tests           | Passed: ordered once-only application, foreign keys, rollback, idempotency.         |
| Live App Server initialization             | Passed over stdio; Windows platform and CLI user agent 0.144.1.                     |
| Live `account/rateLimits/read`             | Passed; dynamic `codex` bucket persisted (latest release observation 71% used).     |
| Live `account/usage/read`                  | Passed; lifetime, daily, peak, duration, and streak values present.                 |
| Reset timestamp probe                      | Raw 1785357505 = 2026-07-29T20:38:25Z as plausible Unix seconds.                    |
| Token notification observation             | None observed; no turn was started to force one.                                    |
| `pnpm test:ui`                             | Passed: Chromium smoke covers Overview, fixture, Usage Burn, Diagnostics, Settings. |
| `pnpm tauri dev`                           | Passed; native window opened and collector reached healthy.                         |
| `pnpm tauri build`                         | Passed after approved NSIS/WiX extraction; release EXE, NSIS, and MSI produced.     |
| Release executable launch                  | Passed; window opened, live collector healthy, existing history preserved.          |
| Native close-to-tray check                 | Close hid the window while process stayed alive; native restoration showed it.      |
| Release process shutdown check             | App and known Node/Codex child processes absent after the release test stopped.     |
| Final frontend quality gates               | Format, lint, check, 5 unit tests, and production build passed.                     |
| Final Rust quality gates                   | Format, check, Clippy with warnings denied, and 40 tests passed.                    |
| Final privacy/repository scan              | No runtime DB/log/spool or personal path; only the documented placeholder secret.   |

### Remaining continuation limitations

- No model turn was started, so live `thread/tokenUsage/updated` delivery was not observed.
- Tray-menu Open/Settings/Exit clicks, explicit autostart enable/disable, and Windows focus ownership were not UI-automated.
- Quota notifications are in-app events rather than native Windows toasts.
- Exports use a fixed per-user destination and Diagnostics has no file-log path.
- The authenticated loopback OpenTelemetry receiver and hook spool replay remain deferred.

## Historical initial slice (superseded by the native continuation above)

The following record describes the earlier fixture-first environment and is retained as plan history. Statements about unavailable Rust, Chromium, live supervision, and installer output are historical and no longer describe the current verified state.

## Objective

Deliver an experimental Windows-first Tauri 2 application that accepts deterministic Codex hook and OpenTelemetry fixtures, stores normalized telemetry in local SQLite, calculates explicitly classified analytics, and renders a useful Overview and Usage Burn experience after restart.

## Verified constraints

- Repository began with only `AGENTS.md` and no Git metadata.
- Installed Codex CLI: `codex-cli 0.144.1`.
- Generated App Server TypeScript and JSON Schemas are stored under `schemas/`.
- Verified App Server methods include `account/rateLimits/read`, `account/usage/read`, and notifications including `account/rateLimits/updated` and `thread/tokenUsage/updated`.
- Hooks, plugins, multi-agent support, and Apps are enabled in the installed CLI; `runtime_metrics` is under development and disabled.
- Node.js 26.4.0 and pnpm 11.9.0 are available. The npm shim is broken. Rust/Cargo are not installed or on `PATH`.
- Tests must use synthetic data and must never initiate a model turn.

## Milestones

- [x] Inspect repository and local CLI/toolchain.
- [x] Generate version-specific App Server schemas.
- [x] Establish accuracy, privacy, persistence, and IPC decisions.
- [x] Create Tauri/Svelte application scaffold and local SQLite migration.
- [x] Implement normalized hook and OTLP JSON fixture adapters with deduplication/redaction.
- [x] Implement dashboard queries, deterministic burn analysis, JSON/CSV export, retention, and delete-local-data.
- [x] Implement tray lifecycle and settings/autostart surface.
- [x] Package an uninstalled optional plugin/hook integration and configuration generator.
- [x] Add deterministic Rust/frontend tests and an Overview smoke test.
- [x] Complete public documentation and GitHub workflow files.
- [x] Run every quality gate supported by the installed environment and record results.
- [x] Perform final privacy/security inspection without staging, committing, or pushing.

## Discoveries

- A quota snapshot can contain dynamically keyed buckets and primary/secondary windows. `usedPercent` is reported, but capacity and consumed units are not exposed.
- Remaining percentage can be derived from a reported percentage, but quota movement cannot be attributed exactly to a project, thread, or turn.
- Detailed token breakdown is available on `thread/tokenUsage/updated`; historical thread reads do not backfill missed token notifications.
- Hook payloads and thread items may contain prompt, command, output, repository, or transcript content. Adapters must use allowlists rather than persist raw payloads.
- Codex user-level OpenTelemetry configuration supports OTLP/HTTP and OTLP/gRPC. Project-local configuration cannot set `otel`.
- Plugin hooks require separate review and trust even after plugin installation.

## Decisions

- Use Tauri 2 + Svelte/TypeScript + Rust + bundled SQLite, with migrations embedded in the native binary.
- Use loopback HTTP with a per-installation local secret as the planned live OTLP transport; the first slice implements and tests the adapter plus configuration generation, and exposes live-receiver status honestly.
- Keep external payload retention off. Normalize with field allowlists and store only metadata.
- Treat quota-to-project/chat/turn correlation as `estimated`; token totals emitted directly by verified interfaces are `reported_exact`; arithmetic over exact fields is `derived_exact`.
- Do not automatically launch App Server, edit Codex configuration, install/trust a plugin, or enable autostart.

## Validation log

| Command                                    | Result                                                   |
| ------------------------------------------ | -------------------------------------------------------- |
| `git --version`                            | 2.55.0.windows.2                                         |
| `node --version`                           | 26.4.0                                                   |
| `npm --version`                            | failed: broken global npm shim                           |
| `pnpm --version`                           | 11.9.0                                                   |
| `rustc --version`                          | unavailable                                              |
| `cargo --version`                          | unavailable                                              |
| `codex --version`                          | 0.144.1                                                  |
| App Server schema generation               | passed for TypeScript and JSON Schema                    |
| `pnpm format:check`                        | passed                                                   |
| `pnpm lint`                                | passed                                                   |
| `pnpm check`                               | passed with 0 errors and 0 warnings                      |
| `pnpm test`                                | passed: 5 frontend tests                                 |
| `pnpm build`                               | passed: production web bundle                            |
| `pnpm audit --audit-level high`            | passed: no known vulnerabilities                         |
| `pnpm tauri info`                          | passed; confirmed WebView2 and missing Rust/MSVC tooling |
| `pnpm tauri icon public/meter.svg`         | passed                                                   |
| Plugin JSON and PowerShell parser checks   | passed                                                   |
| Manual in-app browser smoke                | passed: empty state, fixture load, Usage Burn navigation |
| Playwright browser smoke                   | blocked: Chromium binary is not installed                |
| Rust format, lint, tests, and Tauri builds | blocked: Rust/Cargo/MSVC toolchain is not installed      |
| Official plugin scaffold validator         | blocked: its optional PyYAML dependency is not installed |
| Final code and security review             | passed after fixing local-data deletion and UI wording   |
| Visual verdict                             | passed, score 92; no external reference was supplied     |

## Unresolved limitations

- Rust/Tauri compilation requires installing a Rust MSVC toolchain, which is not currently present.
- Because native compilation is blocked, no MSI, NSIS installer, or executable was produced in this environment.
- The Playwright test is authored but its managed Chromium binary is not installed; the same flow was exercised manually in the in-app browser.
- Live visibility into events from separate Codex desktop sessions is not established by the generated schema.
- Reset timestamp units are not documented in the generated rate-limit schema; values require defensive interpretation.
- Live OTLP listener, real App Server supervision, notification wiring, spool replay, and end-to-end collection remain after the fixture-driven persistent slice.
- GitHub Actions use mutable major-version action tags. Pinning verified commit SHAs remains a supply-chain hardening task.

## Deferred work

Cloud sync, user accounts, hosted services, non-Windows packaging, automatic updates, code signing, content analytics, AI-generated explanations, cost predictions, and exact per-project quota attribution.
