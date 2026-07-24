# Codex Meter optional integration

This uninstalled plugin contains fail-open Windows hooks that forward an allowlisted metadata subset to a loopback Codex Meter receiver. If the app is closed, normalized events spool below the user's local application-data directory for later replay.

The bridge deliberately discards prompts, responses, transcript paths, tool arguments, command text/output, patches, and repository contents. Successful hooks emit no output. Installation does not imply hook trust; review `hooks/hooks.json` and `scripts/emit-hook.ps1`, then explicitly trust the hook definition in Codex.

Do not edit Codex configuration or install this package automatically. See `docs/CODEX_INTEGRATION.md` for manual setup and removal.
