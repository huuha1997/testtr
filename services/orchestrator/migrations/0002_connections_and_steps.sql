CREATE TABLE IF NOT EXISTS connections (
  provider TEXT PRIMARY KEY,
  token TEXT NOT NULL,
  scopes JSONB NOT NULL DEFAULT '[]'::jsonb,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS run_steps (
  id BIGSERIAL PRIMARY KEY,
  run_id UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
  step_key TEXT NOT NULL,
  status TEXT NOT NULL,
  detail TEXT,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS run_steps_run_step_key_idx ON run_steps (run_id, step_key);
CREATE INDEX IF NOT EXISTS run_steps_run_id_updated_at_idx ON run_steps (run_id, updated_at DESC);
