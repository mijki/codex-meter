# Contributing

Codex Meter is intended to stay local-first, privacy-preserving, and honest about data accuracy.

## Before you change anything

- Read [docs/PRD.md](docs/PRD.md), [docs/HLD.md](docs/HLD.md), and [docs/LLD.md](docs/LLD.md) for the current product and architecture shape.
- Update [docs/EXECUTION_PLAN.md](docs/EXECUTION_PLAN.md) when the change crosses an architectural boundary or affects the collection pipeline.
- Add or update an ADR in `docs/adr/` when you change a durable technical decision.

## Working rules

- Keep diffs small and scoped.
- Preserve existing behavior unless a change is explicitly required.
- Do not add new dependencies without a clear reason.
- Do not store secrets, raw prompts, repository contents, or runtime databases in the repo.
- Never present estimated or inferred telemetry as exact OpenAI-reported data.

## Verification

Run the applicable checks before you hand work back:

- `pnpm test`
- `pnpm check`
- `pnpm build`
- any native or desktop checks that apply to the files you touched

If a check fails because the environment is missing a toolchain, say so explicitly in your handoff.

## Pull requests and patches

- Explain why the change is needed.
- Note any privacy, telemetry, or accuracy consequences.
- Call out any unverified behavior.
- Do not claim live collection, live supervision, or exact attribution unless the implementation proves it.
