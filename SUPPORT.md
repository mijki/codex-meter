# Support

If you need help with Codex Meter, start with the repository docs:

- [README](README.md)
- [docs/TESTING.md](docs/TESTING.md)
- [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md)
- [docs/CODEX_INTEGRATION.md](docs/CODEX_INTEGRATION.md)

## What to include in a support request

- operating system and build;
- `codex --version`;
- the exact command or action you ran;
- whether you were in fixture/demo mode;
- the visible error message;
- sanitized logs, if relevant.

## What not to include

- tokens, credentials, or auth files;
- complete prompts or responses;
- environment-variable values;
- repository contents that are not already public;
- runtime databases or unredacted exports.

## Expected behavior today

- If the UI shows synthetic telemetry, that is expected when no live data source is connected.
- If source health is inactive, that reflects missing input rather than a crash.
- If a desktop build fails because Rust or Tauri tooling is missing, call that out in the request instead of treating it as an application bug.
