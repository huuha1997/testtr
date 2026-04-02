CREATE TABLE IF NOT EXISTS runs (
  id UUID PRIMARY KEY,
  status TEXT NOT NULL,
  prompt TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS runs_created_at_idx ON runs (created_at DESC);

