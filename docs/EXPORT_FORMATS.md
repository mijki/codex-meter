# Export Formats

The current UI and native command layer implement JSON, CSV, and diagnostics export. Files are written below the application's local export directory and are removed by the delete-local-data operation.

## Shared rules

- preserve accuracy classification on every exported metric;
- keep identifiers stable where possible;
- use UTC timestamps;
- redact secrets, prompts, responses, and repository contents by default;
- do not export values that the source did not expose.

## JSON export

Use JSON for machine-readable archival. Import is not implemented in v0.1.

The `ExportEnvelope` includes:

- application metadata;
- detected Codex version;
- settings and retention preferences;
- source health;
- quota windows and token totals;
- turn summaries;
- burn analysis;
- accuracy labels.

## CSV export

Use CSV for spreadsheet workflows. The first row is:

```csv
section,id,label,value,accuracy,detail,secondary,tertiary,quaternary
```

Rows are emitted for quota windows, turns, channel health, and warnings. Each metric row preserves the available stable identifier and classification.

- stable IDs;
- timestamp fields;
- source channel;
- metric name;
- metric value;
- accuracy classification;
- any available attribution fields.

## Diagnostics export

Use diagnostics for support and troubleshooting.

The redacted diagnostics envelope includes:

- app version;
- Codex version;
- schema version;
- source health;
- collector state;
- environment limitations.

It contains aggregate table counts and latest timestamps, not raw events or local paths.

## Do not export

- raw prompt or response bodies;
- complete tool arguments;
- credentials or tokens;
- environment-variable values;
- unredacted repository content.
