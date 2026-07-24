# ADR 0001: Tauri 2 desktop stack

Status: accepted

Codex Meter uses Tauri 2, Svelte with strict TypeScript, Vite, and Rust. This keeps the Windows application local, gives the native layer ownership of processes and persistence, and avoids a hosted backend. Svelte only consumes normalized view models through Tauri commands and never parses Codex protocol messages.

Alternatives rejected: Electron, because the required baseline is Tauri and no environmental evidence justifies a stack change; a browser-only app, because tray lifecycle and local process supervision require a desktop host.
