PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_migrations (
  version INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  applied_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS app_metadata (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS codex_installations (
  id TEXT PRIMARY KEY,
  version TEXT NOT NULL,
  schema_version TEXT,
  executable_path TEXT,
  detected_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS capabilities (
  metric TEXT PRIMARY KEY,
  source_channel TEXT NOT NULL,
  source_method TEXT,
  source_field TEXT,
  availability TEXT NOT NULL,
  accuracy TEXT NOT NULL CHECK (accuracy IN ('reported_exact','derived_exact','estimated','unavailable')),
  limitation TEXT,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS collector_sessions (
  id TEXT PRIMARY KEY,
  started_at TEXT NOT NULL,
  ended_at TEXT,
  health TEXT NOT NULL,
  restart_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS sources (
  id TEXT PRIMARY KEY,
  channel TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  health TEXT NOT NULL DEFAULT 'inactive',
  last_event_at TEXT,
  detail TEXT
);

CREATE TABLE IF NOT EXISTS projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  normalized_root TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS repository_locations (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  root_path TEXT NOT NULL,
  remote_url_redacted TEXT,
  UNIQUE(project_id, root_path)
);

CREATE TABLE IF NOT EXISTS worktrees (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  repository_location_id TEXT REFERENCES repository_locations(id) ON DELETE SET NULL,
  path TEXT NOT NULL,
  branch TEXT,
  last_seen_at TEXT NOT NULL,
  UNIQUE(project_id, path)
);

CREATE TABLE IF NOT EXISTS threads (
  id TEXT PRIMARY KEY,
  source_id TEXT REFERENCES sources(id) ON DELETE SET NULL,
  project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
  worktree_id TEXT REFERENCES worktrees(id) ON DELETE SET NULL,
  title TEXT,
  created_at TEXT,
  updated_at TEXT NOT NULL,
  status TEXT,
  accuracy TEXT NOT NULL CHECK (accuracy IN ('reported_exact','derived_exact','estimated','unavailable'))
);

CREATE TABLE IF NOT EXISTS models (
  id TEXT PRIMARY KEY,
  display_name TEXT,
  provider TEXT,
  first_seen_at TEXT NOT NULL,
  last_seen_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS reasoning_settings (
  id TEXT PRIMARY KEY,
  effort TEXT NOT NULL,
  summary TEXT,
  UNIQUE(effort, summary)
);

CREATE TABLE IF NOT EXISTS turns (
  id TEXT PRIMARY KEY,
  thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
  model_id TEXT REFERENCES models(id) ON DELETE SET NULL,
  reasoning_setting_id TEXT REFERENCES reasoning_settings(id) ON DELETE SET NULL,
  started_at TEXT,
  completed_at TEXT,
  duration_ms INTEGER,
  status TEXT NOT NULL,
  tool_count INTEGER NOT NULL DEFAULT 0,
  compaction_count INTEGER NOT NULL DEFAULT 0,
  subagent_count INTEGER NOT NULL DEFAULT 0,
  retry_count INTEGER NOT NULL DEFAULT 0,
  accuracy TEXT NOT NULL CHECK (accuracy IN ('reported_exact','derived_exact','estimated','unavailable'))
);

CREATE TABLE IF NOT EXISTS hook_events (
  id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
  external_event_id TEXT NOT NULL,
  thread_id TEXT REFERENCES threads(id) ON DELETE SET NULL,
  turn_id TEXT REFERENCES turns(id) ON DELETE SET NULL,
  event_type TEXT NOT NULL,
  occurred_at TEXT NOT NULL,
  tool_category TEXT,
  succeeded INTEGER,
  duration_ms INTEGER,
  UNIQUE(source_id, external_event_id)
);

CREATE TABLE IF NOT EXISTS otel_events (
  id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
  external_event_id TEXT NOT NULL,
  thread_id TEXT REFERENCES threads(id) ON DELETE SET NULL,
  turn_id TEXT REFERENCES turns(id) ON DELETE SET NULL,
  event_name TEXT NOT NULL,
  occurred_at TEXT NOT NULL,
  request_duration_ms INTEGER,
  outcome TEXT,
  UNIQUE(source_id, external_event_id)
);

CREATE TABLE IF NOT EXISTS usage_records (
  id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
  external_event_id TEXT NOT NULL,
  thread_id TEXT REFERENCES threads(id) ON DELETE SET NULL,
  turn_id TEXT REFERENCES turns(id) ON DELETE SET NULL,
  model_id TEXT REFERENCES models(id) ON DELETE SET NULL,
  observed_at TEXT NOT NULL,
  input_tokens INTEGER,
  cached_input_tokens INTEGER,
  output_tokens INTEGER,
  reasoning_output_tokens INTEGER,
  total_tokens INTEGER,
  accuracy TEXT NOT NULL CHECK (accuracy IN ('reported_exact','derived_exact','estimated','unavailable')),
  UNIQUE(source_id, external_event_id)
);

CREATE TABLE IF NOT EXISTS quota_buckets (
  id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
  external_limit_id TEXT,
  name TEXT NOT NULL,
  window_kind TEXT NOT NULL,
  window_duration_minutes INTEGER,
  UNIQUE(source_id, external_limit_id, window_kind)
);

CREATE TABLE IF NOT EXISTS quota_snapshots (
  id TEXT PRIMARY KEY,
  bucket_id TEXT NOT NULL REFERENCES quota_buckets(id) ON DELETE CASCADE,
  observed_at TEXT NOT NULL,
  used_percent REAL,
  remaining_percent REAL,
  resets_at TEXT,
  accuracy TEXT NOT NULL CHECK (accuracy IN ('reported_exact','derived_exact','estimated','unavailable')),
  UNIQUE(bucket_id, observed_at)
);

CREATE TABLE IF NOT EXISTS burn_analyses (
  id TEXT PRIMARY KEY,
  scope_type TEXT NOT NULL,
  scope_id TEXT,
  period_start TEXT NOT NULL,
  period_end TEXT NOT NULL,
  method_version TEXT NOT NULL,
  evidence_json TEXT NOT NULL,
  explanation TEXT NOT NULL,
  accuracy TEXT NOT NULL CHECK (accuracy IN ('reported_exact','derived_exact','estimated','unavailable')),
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS collection_errors (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  source_id TEXT REFERENCES sources(id) ON DELETE SET NULL,
  occurred_at TEXT NOT NULL,
  category TEXT NOT NULL,
  redacted_message TEXT NOT NULL,
  retryable INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS notification_state (
  bucket_id TEXT NOT NULL REFERENCES quota_buckets(id) ON DELETE CASCADE,
  reset_key TEXT NOT NULL,
  threshold INTEGER NOT NULL,
  notified_at TEXT NOT NULL,
  PRIMARY KEY(bucket_id, reset_key, threshold)
);

CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS exports (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  format TEXT NOT NULL,
  path TEXT NOT NULL,
  created_at TEXT NOT NULL,
  redacted INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_threads_project_time ON threads(project_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_turns_thread_time ON turns(thread_id, completed_at DESC);
CREATE INDEX IF NOT EXISTS idx_turns_model_time ON turns(model_id, completed_at DESC);
CREATE INDEX IF NOT EXISTS idx_usage_time ON usage_records(observed_at DESC);
CREATE INDEX IF NOT EXISTS idx_usage_project_lookup ON usage_records(thread_id, turn_id, observed_at DESC);
CREATE INDEX IF NOT EXISTS idx_quota_time ON quota_snapshots(bucket_id, observed_at DESC);
CREATE INDEX IF NOT EXISTS idx_hook_turn ON hook_events(turn_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_otel_turn ON otel_events(turn_id, occurred_at DESC);
