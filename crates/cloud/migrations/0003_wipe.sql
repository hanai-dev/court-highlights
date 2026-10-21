-- Wipe box (FR-55): a wiped box leaves the fleet console; its key no longer works.
ALTER TABLE box ADD COLUMN wiped_at TIMESTAMPTZ;
