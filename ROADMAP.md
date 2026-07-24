# Roadmap

Codex Meter is being built in small verified slices. The current snapshot is fixture-first, local-first, and intentionally conservative about what it claims.

## Current phase

- stabilize the local dashboard and normalized data model;
- keep explicit accuracy labels on all visible metrics;
- preserve the fixture-driven exploration path while live ingestion is still being validated.

## Next

- finish the live collection path for verified Codex sources;
- connect source health, deduplication, and persistence more completely;
- verify tray behavior, launch-at-login control, export, and deletion end to end.

## Later

- package and sign Windows installers;
- harden migration and recovery paths;
- expand automated coverage for native lifecycle behavior;
- improve support tooling and redacted diagnostics.

## Out of scope for the MVP

- hosted backend services;
- third-party analytics;
- exact project-level quota attribution when the source cannot prove it;
- storing raw prompts or full responses by default.
