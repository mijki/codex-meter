# Governance

Codex Meter follows a simple governance model:

- durable technical decisions are recorded as ADRs under `docs/adr/`;
- active implementation work is tracked in `docs/EXECUTION_PLAN.md`;
- changes that cross architectural boundaries should update both when needed;
- public-facing documentation must stay aligned with the current implementation.

## Decision rules

- Prefer local-only behavior over hosted infrastructure.
- Prefer explicit accuracy labels over implied certainty.
- Prefer small, reviewable changes over broad rewrites.
- Prefer reversible choices unless the repository has already proven a better long-term path.

## Release discipline

- Do not record a feature as complete until it is implemented, tested, and documented.
- Record any breaking change in `CHANGELOG.md`.
- Keep privacy and telemetry claims conservative until the implementation proves them.

## Contribution discipline

- Preserve user-authored changes.
- Avoid unrelated file churn.
- State assumptions and limitations directly.
