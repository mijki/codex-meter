# Codex Meter Repository Instructions

## Mission

Build and maintain Codex Meter: a local-first Windows desktop application for collecting, storing, analyzing, and displaying Codex usage and quota telemetry.

The application must distinguish clearly between:

- telemetry reported directly by Codex;
- telemetry derived from reported values;
- locally estimated metrics;
- telemetry that the installed Codex version does not expose.

Never present estimated or inferred values as exact OpenAI-reported values.

## Working Method

For significant features, architectural changes, protocol integrations, database migrations, or multi-step refactors, create or update an ExecPlan according to `.agent/PLANS.md`.

Before implementing a significant change:

1. Read the relevant files under `docs/`.
2. Inspect the existing implementation and tests.
3. Update `docs/EXECUTION_PLAN.md`.
4. Record material architectural decisions under `docs/adr/`.
5. Implement the smallest complete vertical slice.
6. Run all applicable quality checks.
7. Update documentation to match the implemented behavior.

Execution plans are living documents. Keep progress, discoveries, decisions, validation results, and unresolved limitations current while working.

## Product Constraints

- Primary platform: Windows 10 and Windows 11.
- The application must operate locally.
- No hosted backend is permitted for the MVP.
- No user telemetry may be sent to a third-party analytics service.
- Persistent data must be stored locally in SQLite.
- The application must support a visible desktop window and Windows system-tray operation.
- Closing the main window should minimize to the system tray unless the user explicitly exits.
- Windows autostart must be configurable and disabled by default until verified.
- The application must continue to show historical data when Codex is unavailable.
- Missing Codex capabilities must degrade gracefully rather than crashing the application.
- Logs must not contain authentication tokens, secrets, complete prompts, environment-variable values, or sensitive repository content.

## Preferred Technical Baseline

Use this baseline unless repository research demonstrates a materially better choice:

- Tauri 2 desktop application
- Svelte with TypeScript and Vite
- Rust-based native process and persistence layer
- SQLite with versioned migrations
- Codex App Server integration
- Structured JSON-RPC message handling
- Version-specific generated Codex App Server schemas
- Local structured diagnostic logging
- Unit, integration, and UI smoke tests

Do not change the primary technology stack without recording the reason in an architecture decision record.

## Codex Integration Rules

Do not hard-code assumptions based on remembered or undocumented Codex protocols.

Before implementing protocol-dependent behavior:

1. Inspect the installed Codex CLI version.
2. Inspect relevant `codex --help` and `codex app-server --help` output.
3. Generate schemas from the installed Codex version where supported.
4. Store the detected Codex version.
5. Validate incoming messages defensively.
6. preserve unknown fields where practical;
7. handle missing methods and fields as capability limitations.

Prefer local standard-input/standard-output communication with a supervised Codex App Server child process unless there is a documented reason to use another transport.

The application must maintain a telemetry capability registry. Each metric must be classified as one of:

- `reported_exact`
- `derived_exact`
- `estimated`
- `unavailable`

Never fabricate quota windows, reset times, token counts, model identifiers, reasoning effort, thread attribution, project attribution, or cost values.

## Attribution Rules

Use stable identifiers whenever available.

- Project attribution should use normalized Git repository roots.
- Preserve worktree paths and Git branches separately.
- Chat attribution should use Codex thread identifiers.
- Prompt attribution should use Codex turn identifiers.
- Model and reasoning settings must be stored at turn level because they may change during a thread.
- Account-level quota changes must not automatically be described as exact project-level consumption.
- Derived quota attribution must be labeled as estimated.

## Data and Privacy

- Store application data under an appropriate per-user application-data directory.
- Never commit runtime SQLite databases.
- Never commit Codex authentication material.
- Never read arbitrary repository files for analytics.
- Do not store full prompt or response bodies by default.
- Store only metadata required for usage analytics unless the user explicitly enables additional local collection.
- Provide a data-retention setting.
- Provide JSON and CSV export.
- Make local data deletion explicit and testable.

## Engineering Standards

- Use strict TypeScript.
- Avoid unchecked `any` in production code.
- Use explicit Rust error types or well-structured error contexts.
- Validate all external process and JSON-RPC input.
- Use SQLite migrations rather than ad hoc schema creation.
- Keep frontend components focused and testable.
- Keep protocol, persistence, analytics, and presentation layers separate.
- Avoid unnecessary production dependencies.
- Pin or lock dependencies.
- Do not suppress compiler, linter, or test failures without documenting the reason.
- Do not leave placeholder implementations in the core collection path.

## Commands

Use pnpm 11.9.0 and the locked dependency graph.

- `pnpm format:check` - frontend and documentation formatting
- `pnpm lint` - ESLint
- `pnpm check` - strict TypeScript/Svelte validation
- `pnpm test` - deterministic frontend unit tests
- `pnpm build` - production frontend build
- `pnpm test:ui` - Overview browser smoke test
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `pnpm tauri dev` / `pnpm tauri build` - native development and production builds

Never use a real Codex turn as a telemetry test.

## Testing

Tests must not depend on consuming the developer’s real Codex quota.

Create deterministic fixtures or a fake App Server process for:

- initialization;
- successful requests and responses;
- notifications;
- malformed messages;
- unsupported methods;
- process termination;
- restart and reconnection;
- missing fields;
- version differences;
- database persistence;
- duplicate-event handling;
- migration behavior.

Where feasible, test Windows-specific lifecycle behavior separately from protocol logic.

## Quality Gates

Before declaring a task complete, run every applicable check exposed by the repository, including:

- frontend formatting;
- frontend linting;
- TypeScript validation;
- frontend unit tests;
- Rust formatting;
- Rust linting with warnings treated seriously;
- Rust unit and integration tests;
- production frontend build;
- Tauri development compilation;
- Tauri production build where the environment supports it;
- database migration tests;
- UI smoke tests where available.

Record the exact commands and results in the final report.

## Definition of Done

A feature is complete only when:

- the implementation is functional;
- failure states are handled;
- tests cover the important behavior;
- relevant checks pass;
- documentation matches the implementation;
- telemetry accuracy is classified;
- no secrets or runtime data are committed;
- the execution plan is updated;
- remaining limitations are stated explicitly.

## Change Discipline

- Do not rewrite unrelated files.
- Do not remove working behavior merely to simplify implementation.
- Keep commits and changes logically scoped.
- Preserve user-authored changes.
- State assumptions explicitly.
- Prefer an honest partial capability over a misleading complete-looking dashboard.

## Open-Source Project Requirements

Codex Meter is intended to be publicly developed and distributed as an
independent open-source project.

All implementation and documentation must be suitable for publication.

- Never commit credentials, authentication files, tokens, runtime databases,
  private prompts, responses, repository source content, absolute personal
  paths, usernames, or machine-specific logs.
- Use generic fixture data in tests and documentation.
- Clearly state that Codex Meter is an independent project and is not affiliated
  with or endorsed by OpenAI.
- Public APIs, database migrations, configuration formats, and exported data
  formats must be documented.
- Breaking changes must be recorded in `CHANGELOG.md`.
- User-visible telemetry must indicate whether it is reported, derived,
  estimated, or unavailable.
- New dependencies must be justified and compatible with the repository license.
- Contributions must pass automated quality and security checks before merge.
