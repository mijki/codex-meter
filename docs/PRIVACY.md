# Privacy

Codex Meter is built to keep telemetry local.

## Privacy commitments

- no hosted backend for the MVP;
- no third-party analytics service;
- no automatic upload of prompts, responses, or repository contents;
- no claim that estimated values are exact OpenAI-reported values;
- no raw runtime database committed to the repo.

## Data that may be stored locally

- normalized telemetry metadata;
- source health and collector state;
- settings and retention preferences;
- capability classifications;
- export metadata;
- redacted diagnostics.

## Data that should not be stored by default

- full prompts or responses;
- command text and command output;
- environment-variable values;
- authentication tokens or credentials;
- sensitive repository content;
- unredacted transcripts.

## Retention and deletion

- The app stores a configurable retention period and prunes normalized telemetry on database open and settings changes.
- The explicit delete action removes normalized analytics rows, generated export files, and normalized hook spool files.
- Deletion uses fixed application-data roots and canonical path containment checks so unrelated user files are not targets.
- Settings and the capability registry remain after analytics deletion; they are configuration, not usage history.

## Fixture mode

The synthetic fixture is local demo data only. It exists to show the UI shape and must never be confused with collected telemetry from a real account.
