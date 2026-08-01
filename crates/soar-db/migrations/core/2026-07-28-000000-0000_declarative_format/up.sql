PRAGMA foreign_keys = OFF;

-- Rebuilt rather than altered: SQLite cannot relax `pkg_id NOT NULL` in place.
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

-- A synthesised id was how a URL or local install recorded its source; that is
-- the family's job now.
UPDATE packages
SET pkg_family = pkg_id
WHERE repo_name = 'local' AND pkg_family IS NULL AND pkg_id IS NOT NULL;

PRAGMA foreign_keys = ON;
