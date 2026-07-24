ALTER TABLE quota_snapshots ADD COLUMN resets_at_raw INTEGER;
ALTER TABLE quota_snapshots
  ADD COLUMN reset_interpretation TEXT NOT NULL DEFAULT 'unavailable';

CREATE TABLE account_usage_snapshots (
  id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
  observed_at TEXT NOT NULL,
  lifetime_tokens INTEGER,
  peak_daily_tokens INTEGER,
  longest_running_turn_sec INTEGER,
  current_streak_days INTEGER,
  longest_streak_days INTEGER,
  accuracy TEXT NOT NULL CHECK (accuracy IN ('reported_exact','derived_exact','estimated','unavailable'))
);

CREATE TABLE account_usage_daily (
  source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
  start_date TEXT NOT NULL,
  tokens INTEGER NOT NULL,
  observed_at TEXT NOT NULL,
  accuracy TEXT NOT NULL CHECK (accuracy IN ('reported_exact','derived_exact','estimated','unavailable')),
  PRIMARY KEY(source_id, start_date)
);

CREATE INDEX idx_account_usage_snapshot_time
  ON account_usage_snapshots(observed_at DESC);
CREATE INDEX idx_account_usage_daily_date
  ON account_usage_daily(start_date DESC);
