# ADR 0005: Metric accuracy classification

Status: accepted

Every displayed metric carries exactly one classification:

- `reported_exact`: directly present in a verified source payload;
- `derived_exact`: arithmetic using only reported exact values;
- `estimated`: a documented correlation or inference;
- `unavailable`: not reliably exposed by the installed version.

The classifier is stored alongside normalized records and preserved in exports. `100 - usedPercent` is derived exact relative to the reported percentage. Account-level quota changes correlated with a thread, turn, chat, or project are estimated. Cost and exact project quota consumption remain unavailable unless a future verified interface reports them directly.
