# Testing

## Current automated coverage

The 20-test frontend suite covers:

- all seven shared telemetry states;
- missing token values versus an explicitly reported zero;
- waiting token and expensive-turn states;
- consolidated disabled integration UX;
- Usage Burn missing-signal source/setup explanations;
- informational, warning, critical, and exhausted alert labels;
- alert dismissal and retained history;
- explicit Live/Demo switching;
- source timestamps, capabilities, and historical-error wording.

Native unit tests live beside their modules under `src-tauri/src/`. The 43-test
suite additionally covers migration v3, alert persistence/deduplication and
dismissal across restart, fixed severity bands, legacy fixture cleanup, source
recovery, nullable/zero token persistence, and unsupported/waiting/error state
transitions.

## Verified commands

The repository exposes these commands in `package.json`:

| Command             | Purpose                       | 2026-07-23 result                |
| ------------------- | ----------------------------- | -------------------------------- |
| `pnpm test`         | Run the Vitest suite.         | Passed, 20 tests.                |
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

The production build generated an executable plus unsigned NSIS and MSI
bundles. The first sandboxed attempt produced the executable and NSIS installer
but WiX `light.exe` could not complete the MSI step. Repeating the cached
bundling command at the OS level completed both formats. Authenticode inspection
reported `NotSigned` for all three artifacts.

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
