ALTER TABLE sources ADD COLUMN last_success_at TEXT;

CREATE TABLE IF NOT EXISTS alert_history (
  id TEXT PRIMARY KEY,
  bucket_id TEXT NOT NULL REFERENCES quota_buckets(id) ON DELETE CASCADE,
  bucket_name TEXT NOT NULL,
  reset_window_id TEXT NOT NULL,
  resets_at TEXT,
  threshold INTEGER NOT NULL,
  used_percent REAL NOT NULL,
  remaining_percent REAL NOT NULL,
  severity TEXT NOT NULL CHECK (
    severity IN ('neutral','informational','warning','critical','exhausted')
  ),
  accuracy TEXT NOT NULL CHECK (
    accuracy IN ('reported_exact','derived_exact','estimated','unavailable')
  ),
  collection_timestamp TEXT NOT NULL,
  created_at TEXT NOT NULL,
  dismissed_at TEXT,
  UNIQUE(bucket_id, reset_window_id, threshold)
);

CREATE INDEX IF NOT EXISTS idx_alert_history_created
  ON alert_history(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_alert_history_active
  ON alert_history(dismissed_at, bucket_id, reset_window_id);

UPDATE settings
SET value_json = '[50,75,90,100]'
WHERE key = 'quotaThresholds' AND value_json = '[75,90]';
