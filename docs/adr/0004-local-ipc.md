# ADR 0004: Loopback OTLP/HTTP with local authorization

Status: accepted for the planned live receiver

The live OpenTelemetry receiver will bind only to `127.0.0.1` and require a randomly generated per-installation bearer secret. OTLP/HTTP JSON is preferred for the first receiver because it is straightforward to configure, validate, size-limit, and test on Windows. The secret is stored locally and never included in diagnostics.

The first vertical slice ships the OTLP normalization adapter and configuration generator even if a build environment cannot validate the live listener. It must report that limitation as unavailable rather than imply collection is active.
