-- Online service schema (HLD §3). Player email addresses are never stored: only a peppered hash of addresses that
-- asked never to be emailed again (FR-25).

CREATE TABLE venue (
  id BIGSERIAL PRIMARY KEY,
  name TEXT NOT NULL,
  online_on BOOLEAN NOT NULL DEFAULT TRUE,
  switch_reason TEXT NOT NULL DEFAULT '',
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE box (
  id BIGSERIAL PRIMARY KEY,
  venue_id BIGINT NOT NULL REFERENCES venue(id),
  name TEXT NOT NULL,
  key_hash TEXT NOT NULL UNIQUE,
  registered_by TEXT NOT NULL,
  registered_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  revoked_at TIMESTAMPTZ,
  revoked_by TEXT
);

-- Fleet health, kept 90 days (FR-49).
CREATE TABLE checkin (
  id BIGSERIAL PRIMARY KEY,
  box_id BIGINT NOT NULL REFERENCES box(id) ON DELETE CASCADE,
  at TIMESTAMPTZ NOT NULL DEFAULT now(),
  version TEXT NOT NULL,
  disk_pct INT NOT NULL,
  temperature_c REAL,
  courts_recording INT NOT NULL,
  courts_total INT NOT NULL
);
CREATE INDEX checkin_box_at ON checkin (box_id, at DESC);

-- One uploaded clip, highlight, reel or 720p game video.
CREATE TABLE item (
  id BIGSERIAL PRIMARY KEY,
  box_id BIGINT NOT NULL REFERENCES box(id),
  venue_id BIGINT NOT NULL REFERENCES venue(id),
  box_ref TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('tap', 'highlight', 'reel', 'game')),
  court TEXT NOT NULL,
  made_at TIMESTAMPTZ NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  object_key TEXT NOT NULL UNIQUE,
  bytes BIGINT,
  uploaded_at TIMESTAMPTZ,
  withdrawn_at TIMESTAMPTZ,
  UNIQUE (box_id, box_ref)
);
CREATE INDEX item_expires ON item (expires_at);

-- One take-home email per game (or per Email-me-this clip): the link page lists its items (FR-23).
CREATE TABLE delivery (
  id BIGSERIAL PRIMARY KEY,
  box_id BIGINT NOT NULL REFERENCES box(id),
  venue_id BIGINT NOT NULL REFERENCES venue(id),
  box_ref TEXT NOT NULL,
  token TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL,
  sent_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (box_id, box_ref)
);

CREATE TABLE delivery_item (
  delivery_id BIGINT NOT NULL REFERENCES delivery(id) ON DELETE CASCADE,
  item_id BIGINT NOT NULL REFERENCES item(id) ON DELETE CASCADE,
  position INT NOT NULL,
  PRIMARY KEY (delivery_id, item_id)
);

-- "This isn't me — don't email me again": each email carries its own token, pointing at the address's hash.
CREATE TABLE never_link (
  token TEXT PRIMARY KEY,
  email_hash TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE blocked (
  email_hash TEXT PRIMARY KEY,
  at TIMESTAMPTZ NOT NULL DEFAULT now()
);
