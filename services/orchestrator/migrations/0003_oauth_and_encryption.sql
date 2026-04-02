ALTER TABLE connections
  ADD COLUMN IF NOT EXISTS encrypted_access_token TEXT,
  ADD COLUMN IF NOT EXISTS encrypted_refresh_token TEXT,
  ADD COLUMN IF NOT EXISTS external_account_id TEXT;

UPDATE connections
SET encrypted_access_token = token
WHERE encrypted_access_token IS NULL
  AND token IS NOT NULL;

ALTER TABLE connections
  ALTER COLUMN encrypted_access_token SET NOT NULL;

ALTER TABLE connections
  DROP COLUMN IF EXISTS token;

CREATE TABLE IF NOT EXISTS oauth_states (
  state TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  redirect_uri TEXT NOT NULL,
  code_verifier TEXT NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS oauth_states_expires_at_idx ON oauth_states (expires_at);
