# Troubleshooting

## The app opens, but the dashboard is empty

Read the state shown in the affected panel. `waiting` means an enabled healthy
source has not produced the relevant event; `disabled` points to the integration
that must be enabled; `unsupported` means the installed Codex version lacks the
capability; `unavailable` means no reliable value can be calculated; and `error`
points to Source Health and Diagnostics. Demo can be selected explicitly for UI
evaluation, but it is never account telemetry.

## The UI says source health is inactive

Source Health separately shows enabled/disabled, current health, last event,
last successful collection, and supplied capabilities. A failure older than the
last successful collection is labeled `Last historical error`; current failures
remain `Latest error`. Diagnostics retains both.

## The burn analysis says signals are missing

Each missing signal lists its source and setup state. Exact quota units and
project-level attribution are unsupported by the verified interface; completed
turn evidence may instead be waiting, disabled, unavailable, or failed.

## Forecast says unavailable or preliminary

Codex Meter needs at least three compatible quota observations covering 15
minutes in the current bucket/reset segment. A reset, a usage drop greater than
two percentage points, a gap over two hours, missing reset time, or a
non-positive/invalid slope can withhold the estimate. Keep Live collection
running; the UI shows the exact invalidation reason and never substitutes an
arbitrary exhaustion time.

Forecasts use percentage points per hour because the verified interface does
not expose quota capacity units. They are local estimates, not values reported
by Codex and not token forecasts.

## Export buttons do nothing outside the desktop app

The export actions are desktop-only. Browser mode uses local fallbacks and does not write export files.

## `pnpm build` or `pnpm tauri build` fails

The environment is probably missing the required desktop toolchain.

Common causes:

- Rust is not installed;
- the Tauri CLI is not available;
- Windows native build prerequisites are missing;
- the environment can build the frontend but not the desktop shell.

On Windows, ensure Rust targets `x86_64-pc-windows-msvc`, Visual Studio C++ Build Tools and a Windows SDK are installed, and the Cargo binary directory is on the command's `PATH`. Installer creation may also require Tauri to download and extract verified NSIS/WiX tooling.

## Diagnostics reports an App Server access-denied error

Microsoft Store app execution aliases can be visible but not spawnable by a packaged desktop process. Codex Meter first resolves the npm-installed `@openai/codex` JavaScript launcher on Windows and uses `node` to start it. Ensure the npm package is installed and `node` is available. The supervisor records bounded failures instead of terminating the UI.

## Reset time is unavailable or marked ambiguous

The 0.144.1 schema exposes an integer `resetsAt` without documenting its unit. Codex Meter preserves the raw value. Plausible Unix seconds or milliseconds are normalized; values outside those ranges stay ambiguous and are not displayed as false exact timestamps.

## The Playwright command finishes assertions but does not exit

On Node 26, Playwright's managed Vite child may remain during teardown. Start `pnpm dev --host 127.0.0.1` separately, run `pnpm test:ui` so Playwright reuses it, then stop Vite explicitly.

## Where are local data and exports?

The database is `%APPDATA%\dev.codexmeter.app\codex-meter.sqlite`. Exports are
written below `%APPDATA%\dev.codexmeter.app\exports`. Diagnostics shows this
redacted environment-relative form by default; use **Copy full resolved
database path** when the absolute path is explicitly needed. A destination
picker and file logger are not currently implemented.

## `pnpm test` or `pnpm check` fails on the fixture UI

Check for local edits in `src/` first. The current test suite is focused on the visible UI contract, not on live telemetry.

## The app shows `DEMO DATA`

Demo was explicitly selected. The marker appears across every screen. Demo
values live only in frontend memory, cannot trigger native notifications, and
are not written to SQLite or exports. Select **Live** to reload current live
history immediately.

## Why there is no Windows toast

The current build keeps alerts persistent and accessible in-app and updates the
tray tooltip for warning/critical quota states. Native Windows toasts require a
new Tauri notification dependency plus reviewed Windows application-identity
configuration; that dependency was not authorized for this slice.
