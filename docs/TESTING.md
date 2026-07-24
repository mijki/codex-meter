# Testing

## Current automated coverage

The 24-test frontend suite covers:

- all seven shared telemetry states;
- missing token values versus an explicitly reported zero;
- waiting token and expensive-turn states;
- consolidated disabled integration UX;
- Usage Burn missing-signal source/setup explanations;
- informational, warning, critical, and exhausted alert labels;
- alert dismissal and retained history;
- explicit Live/Demo switching;
- source timestamps, capabilities, and historical-error wording.
- quota-risk hero priority, Forecast navigation, trajectory/quality rendering,
  typed forecast empty states, and explicit database-path disclosure.

Native unit tests live beside their modules under `src-tauri/src/`. The 50-test
suite additionally covers migration v4; regular and irregular elapsed-time
rates; reset/drop/gap segmentation; insufficient, zero, and negative rate
handling; risk boundaries; forecast persistence and CSV export; alert
deduplication/read/resolution; legacy fixture cleanup; source recovery; and
nullable/zero token state transitions.

## Verified commands

The repository exposes these commands in `package.json`:

| Command             | Purpose                       | 2026-07-24 result                |
| ------------------- | ----------------------------- | -------------------------------- |
| `pnpm test`         | Run the Vitest suite.         | Passed, 24 tests.                |
| `pnpm check`        | Run Svelte type checking.     | Passed, 0 errors and 0 warnings. |
| `pnpm build`        | Build the frontend with Vite. | Passed.                          |
| `pnpm format:check` | Check Prettier formatting.    | Passed.                          |
| `pnpm lint`         | Run ESLint.                   | Passed.                          |
| `pnpm test:ui`      | Run the Overview smoke test.  | Passed in installed Chromium.    |

## Desktop and native checks

The following checks passed with Rust 1.97.1 on `x86_64-pc-windows-msvc`:

- `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`
- `cargo check --manifest-path src-tauri/Cargo.toml --all-targets --all-features`
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`
- `cargo test --manifest-path src-tauri/Cargo.toml --all-features`
- `pnpm tauri dev`
- `pnpm tauri build`

The 2026-07-24 production build generated a fresh executable and NSIS installer;
Authenticode reported `NotSigned` for both. The sandbox denied WiX `light.exe`,
so the existing MSI from the previous validated baseline was not counted as a
fresh artifact. The automatic approval reviewer also rejected the AppData write
needed for the fresh native runtime check after the environment's usage
allowance was exhausted. Deterministic migration/database tests and the release
compile still passed; no real Codex turn was started.

## What the tests should prove

- the UI stays stable in loading and empty states;
- Demo telemetry is explicit, non-persistent, and reversible;
- accuracy labels remain visible;
- the burn panel keeps calling out missing signals;
- Demo mode is clearly marked across navigation and content;
- a fake process proves startup, initialization, read-only request selection, notifications, timeout/restart bounds, pause/resume, and shutdown without consuming quota;
- migrations execute in order, exactly once, with rollback and foreign keys;
- threshold notifications require `reported_exact` data, use fixed severity
  bands, persist dismissal/history, and deduplicate per bucket/reset/threshold;
- forecast models use actual elapsed time, never cross reset/gap segments, and
  withhold invalid dates rather than fabricating them;
- forecast alerts are estimated, deduplicate by bucket/reset/type, retain
  unread/resolved history, and do not enter the exact native threshold path;
- a successful source event demotes an older error to historical without
  removing it from Diagnostics.

## Runtime validation and gaps

- A live 0.144.1 App Server probe sent only initialize, `account/rateLimits/read`, and `account/usage/read`; it started no thread or turn.
- The current native development executable opened, migrated/reused the per-user
  database, and reached healthy App Server collection with fresh rate-limit and
  account-usage snapshots. A pre-existing release instance was left untouched.
- Close-to-tray hiding and native restoration were exercised. Tray-menu Open/Settings/Exit clicks and autostart toggling were not UI-automated.
- `thread/tokenUsage/updated` was not observed live because no model turn was started; deterministic parser/persistence tests cover it.
- The Playwright test passes when it reuses an explicitly started Vite server. On this Node 26 environment, Playwright's managed Vite child can linger during teardown; this is a harness cleanup limitation, not an assertion failure.
- The live loopback OpenTelemetry receiver remains deferred.
- Collector-health and forecast-confidence-change events are visible in Source
  Health/forecast quality, but do not yet create alert-history records.
