# ADR 0008: Local quota forecasting and uPlot charts

## Status

Accepted for the professional-dashboard continuation.

## Context

Codex App Server reports account quota used percentages and reset metadata but
does not report quota capacity units. Codex Meter needs actionable pace and
exhaustion guidance without presenting inference as reported telemetry. The
dashboard also needs time-proportional charts that handle irregular polling,
reported history, derived trends, estimated projections, thresholds, and reset
boundaries.

## Decision

Forecasting remains deterministic, local, and segmented by quota bucket and
reset-window identity. A segment is invalidated or restarted when reset
metadata changes, usage drops beyond a defensive tolerance, window duration
changes materially, or an observation gap makes continuity unreliable.

The analytics layer compares recent/rolling rate, ordinary least-squares slope,
and exponentially weighted moving average. It selects a primary positive rate
using observation count, coverage, regularity, volatility, model agreement,
reset proximity, and discontinuities. Forecast confidence is a documented
quality grade (`unavailable`, `preliminary`, `low`, `medium`, or `high`), not a
statistically calibrated probability.

Rates use percentage points per hour. Safe rate is remaining reported
percentage divided by hours until reset. Exhaustion time and projected usage at
reset are estimates, and no date is fabricated when the rate or horizon is
invalid.

The frontend uses `uplot` through one focused Svelte adapter. uPlot is
framework-agnostic, small, maintained, time-axis aware, and supports irregular
timestamps and multiple styled series. Accessible textual summaries and typed
tables remain the semantic source of truth because the chart canvas is
supplemental.

## Consequences

- Forecast and quality records require a versioned SQLite migration and indexed
  bucket/time lookups.
- Historical reported points, derived trend/rates, and estimated forecast
  series use distinct line styles and visible accuracy labels.
- Demo charts use in-memory fixtures only and cannot write forecast or alert
  history.
- Native Windows toast delivery is not introduced by this decision; in-app and
  tray alerts remain the verified notification surfaces.
- Completed-window forecast evaluation is possible only when Codex Meter
  observed the relevant outcome through the reset boundary.

## Rejected alternatives

- A hosted forecast service would violate the local-only product boundary.
- A language-model forecast would be nondeterministic and methodologically
  inappropriate for numeric time-series extrapolation.
- A large custom SVG/canvas chart engine would add maintenance and
  accessibility risk.
- Treating token totals as quota capacity would fabricate a relationship that
  the verified interface does not expose.
