# ADR 0007: Telemetry state, persistent alerts, and non-persistent Demo mode

Status: accepted

Codex Meter represents every telemetry-dependent metric with a shared state:
`live`, `waiting`, `disabled`, `unsupported`, `unavailable`, `error`, or
`fixture`. Metric values are nullable and travel with accuracy, source,
last-observed time, and an unavailability reason. Zero remains a value and is
shown only when a verified source explicitly reports it.

Quota alerts are persisted separately from notification delivery. Their stable
identity is the quota bucket, reset-window key, and crossed threshold. Alert
history records the collection timestamp, reset timestamp, severity, accuracy,
and optional dismissal timestamp. Dismissal changes presentation state without
removing history or deduplication evidence.

Demo mode is a frontend presentation mode backed by a deterministic in-memory
fixture. It does not pause live collection, clear SQLite, write fixture rows,
emit notification events, or change persisted settings. Leaving Demo mode
immediately reloads the live dashboard.

Successful source collection stores a last-success timestamp. Collection errors
remain in Diagnostics history, while Source Health labels an error older than
the most recent success as historical.

Rejected: persisting fixture rows in the live telemetry database, because it
temporarily destroys or contaminates live history and prevents immediate mode
restoration.

Rejected: adding a native notification dependency in this slice, because the
task prohibits dependency installation and Windows notification identity
behavior requires a separately reviewed integration.
