-- A package without an id cannot be represented once the column is required
-- again. Refuse the downgrade rather than deleting the rows: the CHECK fails
-- when any such row exists, and the table name is what the error reports.
CREATE TEMP TABLE cannot_downgrade_packages_without_pkg_id (
  ok INTEGER NOT NULL CHECK (ok = 1)
);
INSERT INTO cannot_downgrade_packages_without_pkg_id (ok)
  SELECT CASE WHEN EXISTS (SELECT 1 FROM packages WHERE pkg_id IS NULL) THEN 0 ELSE 1 END;
DROP TABLE cannot_downgrade_packages_without_pkg_id;

CREATE TABLE packages_old (
  id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  repo_name TEXT NOT NULL,
  pkg_id TEXT NOT NULL COLLATE NOCASE,
  pkg_name TEXT NOT NULL COLLATE NOCASE,
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

INSERT INTO packages_old
SELECT id, repo_name, pkg_id, pkg_name, pkg_type, version, size, checksum,
       installed_path, installed_date, profile, pinned, is_installed, detached,
       unlinked, provides, install_patterns
FROM packages;

DROP TABLE packages;
ALTER TABLE packages_old RENAME TO packages;
