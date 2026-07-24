# ADR 0006: Supervised read-only App Server collection

Status: accepted

Codex Meter launches a dedicated Codex App Server over newline-delimited stdio JSON-RPC. The native supervisor performs initialization and the verified read-only `account/rateLimits/read` and `account/usage/read` calls. It validates responses and supported notifications, persists normalized metadata only, exposes explicit health states, and bounds timeouts and restart attempts. Pause, resume, and shutdown are native lifecycle operations.

The Windows runtime prefers the npm-installed `@openai/codex` JavaScript launcher through `node` because the Microsoft Store executable alias can reject child-process creation from the desktop app. A normal `codex` executable remains the fallback.

The collector does not create threads, start turns, send prompts, or assume that its separate App Server observes unrelated Codex desktop sessions. `thread/tokenUsage/updated` is stored only when naturally received; missed history is not fabricated.

Alternatives rejected: polling undocumented files, because authentication and protocol semantics would be unsafe; an unbounded restart loop, because persistent launch failures must not consume resources indefinitely; treating account quota movement as project usage, because the source is account-scoped.
