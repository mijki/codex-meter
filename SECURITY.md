# Security

Codex Meter is designed to keep telemetry local. It does not need a hosted backend for the current MVP scope, and it must not send user telemetry to third-party analytics services.

## What to protect

- authentication material;
- access tokens;
- complete prompts or responses;
- environment-variable values;
- repository contents that have not been deliberately redacted;
- runtime SQLite databases and local export bundles.

## What to report

Report any issue that could expose data, weaken local-only guarantees, or misclassify telemetry accuracy.

Examples:

- secrets appearing in logs or exports;
- unredacted prompt, transcript, or command content;
- accidental network transmission of local telemetry;
- incorrect claims that estimated values are exact;
- local file writes outside the intended per-user data location.

## How to report

- Prefer a private security contact if the repository host provides one.
- If no private channel exists, open a public issue with all sensitive details removed.
- Include the app version, Codex version, OS version, and a minimal reproduction.

## Safe reporting format

- redact secrets and tokens;
- replace absolute personal paths with generic placeholders;
- do not attach runtime databases unless a maintainer asks for a sanitized sample.
