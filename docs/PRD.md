# Product Requirements Document

## Problem

Users need a local way to understand Codex usage without depending on a hosted analytics service or guessing about data provenance.

## Product goal

Codex Meter should collect, store, and explain locally available Codex telemetry on Windows while clearly distinguishing:

- values reported directly by Codex;
- values derived from reported data;
- values estimated from correlations;
- values that the installed Codex version does not expose.

## Current scope

The current repository is a native local-first vertical slice with live,
read-only App Server account collection, SQLite history, shared telemetry
states, deterministic quota outlooks, persistent quota and forecast-risk alerts,
source recovery details, and an in-memory Demo mode. Generated App Server schema
artifacts target Codex CLI 0.144.1.

## User goals

- see quota windows and recent usage quickly;
- understand whether a metric is exact, derived, estimated, or unavailable;
- keep data local to the machine;
- export or delete local analytics data;
- avoid losing historical context when Codex is unavailable.
- understand whether a panel is live, waiting, disabled, unsupported,
  unavailable, failed, or showing Demo data;
- distinguish an exact reported zero from a missing value;
- review, dismiss, and audit quota alerts by reset window.
- see whether the current quota pace is sustainable and when a locally estimated
  exhaustion would occur;
- inspect the observations, model, coverage, gaps, and agreement behind an
  estimate rather than receiving an unexplained prediction.

## Non-goals

- hosted backend services;
- third-party analytics;
- exact project-level quota attribution when the source cannot prove it;
- storing raw prompts or full responses by default;
- claiming live supervision or collection that has not been verified.
- treating percentage burn as token consumption or claiming that a forecast was
  reported by Codex.

## Success criteria

- the app opens locally on Windows;
- the UI makes data provenance obvious;
- data stays on the device;
- the app degrades gracefully when sources are missing;
- exports and local deletion are explicit;
- tests cover the current visible behavior.
- warning-level quota risk remains prominent until dismissed;
- Demo data never enters the live telemetry database or native notification path.
- the highest-priority quota risk is visible without opening a secondary view;
- forecasts degrade to a reasoned unavailable state when the evidence is
  insufficient and preserve reported/derived/estimated provenance throughout.
