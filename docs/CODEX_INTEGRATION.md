# Codex Integration

## Verified source facts

The repository documents Codex CLI version 0.144.1 and stores generated App Server schemas under `schemas/`.

The verified schema surface includes:

- `account/rateLimits/read`
- `account/usage/read`
- `account/rateLimits/updated`
- `thread/tokenUsage/updated`

## Rate limits

`GetAccountRateLimitsResponse` and `AccountRateLimitsUpdatedNotification` show a sparse rate-limit model:

- `rateLimits` is the backward-compatible single-bucket view;
- `rateLimitsByLimitId` is the multi-bucket view keyed by `limit_id`;
- `usedPercent` is required on each rate-limit window;
- `resetsAt` is an integer timestamp when present and may be null in some snapshots;
- `credits` and `individualLimit` are optional and may be unavailable.

The generated schema also makes it clear that the notification is sparse and should be merged into the most recent snapshot instead of replacing previously observed values blindly.

## Token usage

`ThreadTokenUsageUpdatedNotification` carries:

- `threadId`;
- `turnId`;
- `tokenUsage.last`;
- `tokenUsage.total`.

The breakdown includes:

- `inputTokens`;
- `cachedInputTokens`;
- `outputTokens`;
- `reasoningOutputTokens`;
- `totalTokens`.

`GetAccountTokenUsageResponse` adds account-level summary data such as `lifetimeTokens`, `peakDailyTokens`, and streak counters.

## Accuracy policy

Every metric must be classified as one of:

- `reported_exact`
- `derived_exact`
- `estimated`
- `unavailable`

Use these rules:

- values read directly from a verified schema are `reported_exact`;
- arithmetic over exact values is `derived_exact`;
- attribution or inference is `estimated`;
- unsupported or missing fields are `unavailable`.

## Current integration boundary

The native application now supervises a separate `codex app-server --stdio` process and performs only initialization, `account/rateLimits/read`, and `account/usage/read`. These account reads were observed live with CLI 0.144.1 and persisted to SQLite. The collector also validates and persists `account/rateLimits/updated` and `thread/tokenUsage/updated` when naturally received.

Validation deliberately started no thread or model turn. No token notification was observed live, and the separate App Server is not assumed to receive events from unrelated Codex desktop tasks. Missed token notifications are not backfilled or invented.

On Windows, the Microsoft Store executable alias returned access denied when spawned from the desktop app. The runtime therefore prefers the npm-installed `@openai/codex/bin/codex.js` launcher through `node`, while retaining a `codex` fallback. The supervised native child remains the same installed 0.144.1 CLI.

The authenticated loopback OpenTelemetry receiver is still not implemented or enabled.

## Reset timestamp interpretation

The schema does not declare the epoch unit of `resetsAt`. Codex Meter stores the raw integer and applies a range check:

- plausible Unix seconds are labeled `unix_seconds`;
- plausible Unix milliseconds are labeled `unix_milliseconds`;
- other values are labeled `ambiguous` and are not presented as exact reset times.

The live value `1785357505` resolved plausibly to `2026-07-29T20:38:25Z` as Unix seconds. This observation supports the interpretation for that payload, not a permanent undocumented protocol guarantee.

## Optional plugin and hooks

The package under `integrations/codex-plugin/` uses the verified default `hooks/hooks.json` discovery path. It observes `SessionStart`, `UserPromptSubmit`, `PostToolUse`, `PreCompact`, `PostCompact`, `SubagentStart`, `SubagentStop`, and `Stop`. Its PowerShell bridge allowlists identifiers and metadata, creates a bounded local request, then falls back to `%LOCALAPPDATA%\CodexMeter\spool` without blocking Codex.

Installation and trust are deliberately manual:

1. Review the manifest, hook JSON, and script.
2. Place the package in a local marketplace controlled by the user.
3. Run `codex plugin add codex-plugin@<local-marketplace-name>`.
4. Review and trust the hook definition separately; Codex does not trust plugin hooks merely because the plugin is installed.
5. Start a new task after installation or update.

Remove it with `codex plugin remove codex-plugin` using the marketplace/plugin state shown by `codex plugin list`. Delete normalized spool files only after Codex Meter has replayed them or the user intentionally discards that local history.

## OpenTelemetry configuration

Official configuration for the installed generation supports user-level `[otel]` with OTLP/HTTP or OTLP/gRPC exporters. Project-local `.codex/config.toml` cannot set `otel`. Codex Meter therefore provides `integrations/otel/codex-config.example.toml` and a Settings copy action, but does not edit user configuration.

Keep prompt logging disabled. Do not enable the example until the live loopback receiver reports healthy and supplies a local secret. The current slice implements fixture normalization and configuration generation, not the listener. Disable export by removing the `[otel]` section from user configuration.

## Practical implications

- merge sparse rate-limit updates into existing state;
- discover every bucket in `rateLimitsByLimitId` instead of assuming fixed window names;
- never fabricate quota windows, reset times, or project attribution;
- treat project, chat, and turn correlation as estimated unless a verified source proves otherwise;
- preserve unknown fields where practical;
- handle missing methods and fields as capability limitations.
