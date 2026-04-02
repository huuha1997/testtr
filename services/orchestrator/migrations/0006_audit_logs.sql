CREATE TABLE IF NOT EXISTS audit_logs (
  id BIGSERIAL PRIMARY KEY,
  provider TEXT,
  action TEXT NOT NULL,
  status TEXT NOT NULL,
  detail JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS audit_logs_provider_created_at_idx
  ON audit_logs (provider, created_at DESC);

CREATE INDEX IF NOT EXISTS audit_logs_action_created_at_idx
  ON audit_logs (action, created_at DESC);
