# High-Level Design

## Overview

Codex Meter is structured as a desktop shell around a local telemetry pipeline:

`Codex sources -> normalization -> SQLite -> deterministic analytics -> dashboard view models -> Svelte UI`

Live read-only account ingestion is verified for Codex CLI 0.144.1. Forecasting
is a separate local analytics layer and is never presented as an App Server
capability.

## Major layers

### Frontend

- Svelte 5 renders the dashboard and settings experience.
- `src/lib/types.ts` defines the view models and accuracy classes.
- `src/lib/api.ts` isolates the UI from the Tauri command surface.
- `src/lib/fixture.ts` provides synthetic demo data.
- `uPlot` renders time-proportional charts without owning analytics or changing
  telemetry classifications.

### Native shell

- Tauri 2 owns the desktop window and the local application bundle.
- `src-tauri/tauri.conf.json` defines the window, security policy, and installer targets.
- `src-tauri/migrations/` contains the ordered schema through version 4.
- `src-tauri/src/forecast.rs` computes deterministic rates, projections, model
  quality, and trajectory points from compatible quota observations.

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
plus unread/dismissed/resolved state, while Tauri events only notify the current
UI of newly crossed exact thresholds. Locally estimated risk alerts remain
in-app and identify their forecast source.

Forecast input is segmented by quota bucket and reset identity. Usage drops and
long observation gaps start a new analytical segment. Models use actual elapsed
time, require minimum evidence, and return `unavailable` instead of inventing an
exhaustion time when the slope is invalid or non-positive.

Demo mode is an in-memory frontend presentation path. It does not pause the
collector, replace SQLite data, or emit notifications.

## Current implementation boundary

Live read-only App Server collection is verified for account quota and usage.
Detailed per-turn events remain capability- and integration-dependent. The UI
shows explicit disabled, waiting, unsupported, unavailable, and error states
rather than presenting these gaps as broken cards.
