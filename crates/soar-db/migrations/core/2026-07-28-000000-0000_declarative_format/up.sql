-- Installed rows record the family a package came from, so variants can be
-- told apart without the package id repositories no longer publish, and the
-- id itself stops being required. SQLite cannot relax NOT NULL in place, so
-- the table is rebuilt.
CREATE TABLE packages_new (
  id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  repo_name TEXT NOT NULL,
  pkg_id TEXT COLLATE NOCASE,
  pkg_name TEXT NOT NULL COLLATE NOCASE,
  pkg_family TEXT COLLATE NOCASE,
  pkg_type TEXT COLLATE NOCASE,
  version TEXT NOT NULL,
  size BIGINT NOT NULL,
  checksum TEXT,
  installed_path TEXT NOT NULL,
  installed_date TEXT NOT NULL,
  profile TEXT NOT NULL,
  pinned BOOLEAN NOT NULL DEFAULT false,
  is_installed BOOLEAN NOT NULL DEFAULT false,
  detached BOOLEAN NOT NULL DEFAULT false,
  unlinked BOOLEAN NOT NULL DEFAULT false,
  provides JSONB,
  install_patterns JSONB
);

INSERT INTO packages_new
SELECT id, repo_name, pkg_id, pkg_name, NULL, pkg_type, version, size, checksum,
       installed_path, installed_date, profile, pinned, is_installed, detached,
       unlinked, provides, install_patterns
FROM packages;

DROP TABLE packages;
ALTER TABLE packages_new RENAME TO packages;

-- Installs from a URL or a local file used to carry a synthesised id, since
-- that was the only field able to say where they came from. That is now the
-- family's job, so the value moves across and nothing derives an id again.
UPDATE packages
SET pkg_family = pkg_id
WHERE repo_name = 'local' AND pkg_family IS NULL AND pkg_id IS NOT NULL;
