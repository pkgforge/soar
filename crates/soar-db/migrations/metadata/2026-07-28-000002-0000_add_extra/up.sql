-- Pinned side files installed alongside the artifact, typically a licence the
-- artifact itself does not carry.
ALTER TABLE packages ADD COLUMN extra JSONB;
