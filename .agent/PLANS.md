# Codex Meter execution plans

Use an ExecPlan for protocol work, migrations, desktop lifecycle changes, or any change spanning more than one architectural boundary.

An ExecPlan is a living document. Keep these sections current:

- objective and user-visible outcome;
- verified constraints and source-of-truth versions;
- milestones with checkboxes;
- discoveries and decisions, including accuracy/privacy consequences;
- validation commands and exact results;
- unresolved limitations and deferred work.

Plans must describe a smallest complete vertical slice before follow-on breadth. A milestone is complete only when its behavior is implemented, tested, and documented. Never silently convert an unavailable signal into an estimate, and never describe an estimate as reported telemetry.

The active plan is `docs/EXECUTION_PLAN.md`. Update it before and after substantial implementation work.
