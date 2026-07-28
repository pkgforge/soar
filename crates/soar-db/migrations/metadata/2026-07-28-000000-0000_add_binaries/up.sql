-- Where each executable lives inside the artifact, and what to call it.
-- Without this a package whose binary is named differently from the package
-- (gdu_linux_amd64 -> gdu) cannot be installed from a repository index.
ALTER TABLE packages ADD COLUMN binaries JSONB;
