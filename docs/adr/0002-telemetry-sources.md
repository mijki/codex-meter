# ADR 0002: Hybrid, capability-gated telemetry sources

Status: accepted

Codex Meter treats App Server, lifecycle hooks, and OpenTelemetry as independent, disableable channels. Version-specific generated App Server schemas are authoritative for the installed CLI. Each adapter validates and normalizes only allowlisted metadata; unknown fields are ignored without crashing. Duplicate source event identifiers are idempotent.

No channel is assumed complete. A separately launched App Server may not observe unrelated desktop sessions. Hooks and OpenTelemetry therefore remain complementary, and missing channels produce capability warnings rather than fabricated values.
