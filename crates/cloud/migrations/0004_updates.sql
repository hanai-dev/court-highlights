-- Fleet updates (FR-51): releases an Admin publishes, and each box's progress through them.
CREATE TABLE release (
  id BIGSERIAL PRIMARY KEY,
  version TEXT NOT NULL UNIQUE,
  sha256 TEXT NOT NULL,
  object_key TEXT NOT NULL,
  bytes BIGINT NOT NULL,
  published_by TEXT NOT NULL,
  published_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE box_update (
  box_id BIGINT NOT NULL REFERENCES box(id) ON DELETE CASCADE,
  version TEXT NOT NULL,
  offered_at TIMESTAMPTZ NOT NULL,
  -- NULL while the box installs; then what it reported.
  outcome TEXT CHECK (outcome IN ('updated', 'rolled back', 'refused')),
  finished_at TIMESTAMPTZ,
  PRIMARY KEY (box_id, version)
);
