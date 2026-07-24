CREATE TABLE IF NOT EXISTS quota_forecast_history (
  id TEXT PRIMARY KEY,
  bucket_id TEXT NOT NULL REFERENCES quota_buckets(id) ON DELETE CASCADE,
  reset_window_id TEXT NOT NULL,
  generated_at TEXT NOT NULL,
  forecast_model TEXT NOT NULL,
  predicted_exhaustion_at TEXT,
  projected_usage_at_reset REAL,
  safe_rate_pph REAL,
  selected_burn_rate_pph REAL,
  pace_ratio REAL,
  confidence TEXT NOT NULL,
  sample_count INTEGER NOT NULL,
  coverage_duration_sec INTEGER NOT NULL,
  largest_gap_sec INTEGER NOT NULL,
  invalidation_reason TEXT,
  exhaustion_before_reset INTEGER,
  observed_final_usage REAL,
  evaluated_at TEXT,
  forecast_error_minutes REAL,
  forecast_lead_time_minutes REAL,
  projected_usage_error REAL,
  classification_correct INTEGER
);

CREATE INDEX IF NOT EXISTS idx_quota_forecast_bucket_time
  ON quota_forecast_history(bucket_id, generated_at DESC);

CREATE INDEX IF NOT EXISTS idx_quota_forecast_window_time
  ON quota_forecast_history(reset_window_id, generated_at DESC);

ALTER TABLE alert_history
  ADD COLUMN alert_type TEXT NOT NULL DEFAULT 'quota_threshold';
ALTER TABLE alert_history
  ADD COLUMN alert_source TEXT NOT NULL DEFAULT 'reported_quota';
ALTER TABLE alert_history ADD COLUMN resolved_at TEXT;
ALTER TABLE alert_history ADD COLUMN unread INTEGER NOT NULL DEFAULT 1;
ALTER TABLE alert_history ADD COLUMN predicted_exhaustion_at TEXT;
ALTER TABLE alert_history ADD COLUMN forecast_confidence TEXT;

CREATE INDEX IF NOT EXISTS idx_alert_history_unread
  ON alert_history(unread, resolved_at, dismissed_at, created_at DESC);
