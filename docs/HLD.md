# High-Level Design

## Overview

Codex Meter is structured as a desktop shell around a local telemetry pipeline:

`Codex sources -> normalization -> SQLite -> dashboard view models -> Svelte UI`

The repository currently shows the UI, fixture data, the SQLite schema, generated schema evidence, and the desktop shell configuration. Live ingestion is still under validation and should not be advertised as verified in this snapshot.

## Major layers

### Frontend

- Svelte 5 renders the dashboard and settings experience.
- `src/lib/types.ts` defines the view models and accuracy classes.
- `src/lib/api.ts` isolates the UI from the Tauri command surface.
- `src/lib/fixture.ts` provides synthetic demo data.

### Native shell

- Tauri 2 owns the desktop window and the local application bundle.
- `src-tauri/tauri.conf.json` defines the window, security policy, and installer targets.
- `src-tauri/migrations/0001_initial.sql` defines the SQLite schema.

### Integration evidence

- `schemas/` contains version-specific Codex App Server schemas generated for CLI 0.144.1.
- The verified schema surface includes account rate limits and token usage notifications.

## Trust boundaries

- Raw prompts, responses, and repository contents must not be stored by default.
- Accuracy labels must travel with the metric.
- Unknown or missing source fields should be preserved or ignored defensively, not guessed.
- Local exports and diagnostics must remain redacted.

## Telemetry presentation contract

Normalized dashboard responses carry a shared telemetry state plus accuracy,
source, last observation, integration requirement, and unavailability reason.
The Svelte layer renders this contract and does not reinterpret missing values.
Numeric absence is `null`; zero remains an observed value.

Alert delivery and alert persistence are separate. SQLite retains alert history
and dismissal state, while Tauri events only notify the current UI of newly
crossed exact thresholds.

Demo mode is an in-memory frontend presentation path. It does not pause the
collector, replace SQLite data, or emit notifications.

## Current implementation boundary

Live read-only App Server collection is verified for account quota and usage.
Detailed per-turn events remain capability- and integration-dependent. The UI
shows explicit disabled, waiting, unsupported, unavailable, and error states
rather than presenting these gaps as broken cards.
