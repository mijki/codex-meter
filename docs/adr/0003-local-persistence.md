# ADR 0003: Local SQLite persistence

Status: accepted

Normalized telemetry is stored in a foreign-key-enforced SQLite database below the operating system's per-user application-data directory. Schema changes use ordered embedded migrations. Writes that span entities run in transactions, and source plus external event ID provide duplicate protection.

Runtime databases, spools, logs, and diagnostics are ignored by Git. Raw prompts, responses, tool arguments, command text/output, transcripts, credentials, and repository contents are not stored.
