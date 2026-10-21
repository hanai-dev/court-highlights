-- Fleet console (FR-47…FR-53): Admin and Installer accounts with password + authenticator code.

CREATE TABLE fleet_user (
  id BIGSERIAL PRIMARY KEY,
  username TEXT NOT NULL UNIQUE,
  role TEXT NOT NULL CHECK (role IN ('admin', 'installer')),
  password_hash TEXT NOT NULL,
  must_change BOOLEAN NOT NULL DEFAULT TRUE,
  -- The authenticator secret, encrypted with the service key (CH_SECRET_KEY).
  totp_secret_enc BYTEA NOT NULL,
  -- Each authenticator code is accepted once (RFC 6238 §5.2): the last accepted time step.
  totp_last_step BIGINT NOT NULL DEFAULT 0,
  active BOOLEAN NOT NULL DEFAULT TRUE,
  failures INT NOT NULL DEFAULT 0,
  locked_until TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE fleet_session (
  token_hash TEXT PRIMARY KEY,
  user_id BIGINT NOT NULL REFERENCES fleet_user(id) ON DELETE CASCADE,
  signed_in_at TIMESTAMPTZ NOT NULL,
  last_seen_at TIMESTAMPTZ NOT NULL
);

-- Registrations, revocations, switches, updates and sign-ins, with who and when (FR-52).
CREATE TABLE fleet_log (
  id BIGSERIAL PRIMARY KEY,
  at TIMESTAMPTZ NOT NULL DEFAULT now(),
  actor TEXT NOT NULL,
  action TEXT NOT NULL,
  item TEXT NOT NULL DEFAULT '',
  detail TEXT NOT NULL DEFAULT ''
);
