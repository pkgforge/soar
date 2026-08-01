PRAGMA foreign_keys = OFF;

-- A package without an id cannot be represented once the column is required
-- again. This table is a cache of the published index, so the rows are simply
-- dropped and the next sync puts them back.
DELETE FROM packages WHERE pkg_id IS NULL;

DROP INDEX IF EXISTS packages_identity;

CREATE TABLE packages_old (
  id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  pkg_id TEXT NOT NULL COLLATE NOCASE,
  pkg_family TEXT COLLATE NOCASE,
  pkg_name TEXT NOT NULL COLLATE NOCASE,
  pkg_type TEXT COLLATE NOCASE,
  pkg_webpage TEXT,
  app_id TEXT COLLATE NOCASE,
  description TEXT,
  version TEXT NOT NULL,
  version_upstream TEXT,
  licenses JSONB,
  download_url TEXT NOT NULL,
  size BIGINT,
  ghcr_pkg TEXT,
  ghcr_size BIGINT,
  ghcr_blob TEXT,
  ghcr_url TEXT,
  bsum TEXT,
  icon TEXT,
  desktop TEXT,
  appstream TEXT,
  homepages JSONB,
  notes JSONB,
  source_urls JSONB,
  tags JSONB,
  categories JSONB,
  build_id TEXT,
  build_date TEXT,
  build_action TEXT,
  build_script TEXT,
  build_log TEXT,
  provides JSONB,
  snapshots JSONB,
  replaces JSONB,
  soar_syms BOOLEAN NOT NULL DEFAULT false,
  desktop_integration BOOLEAN,
  portable BOOLEAN,
  UNIQUE (pkg_id, pkg_name, version)
);

INSERT INTO packages_old SELECT
  id, pkg_id, pkg_family, pkg_name, pkg_type, NULL, app_id, description,
  version, NULL, licenses, download_url, size, ghcr_pkg, ghcr_size, ghcr_blob,
  ghcr_url, bsum, icon, desktop, appstream, homepages, notes, source_urls,
  NULL, categories, build_id, build_date, build_action, build_script,
  build_log, provides, snapshots, replaces, soar_syms, desktop_integration,
  portable
FROM packages;

DROP TABLE packages;
ALTER TABLE packages_old RENAME TO packages;

PRAGMA foreign_keys = ON;
