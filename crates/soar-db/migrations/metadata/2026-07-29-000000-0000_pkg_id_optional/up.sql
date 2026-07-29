-- A repository publishing the declarative format has no package id to give,
-- so the column stops being required. SQLite cannot relax NOT NULL in place.
--
-- The uniqueness key keeps the id, but NULLs are collapsed first: SQLite
-- treats every NULL as distinct, so an id-less package would otherwise insert
-- a fresh duplicate on every sync instead of conflicting with itself.
CREATE TABLE packages_new (
  id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  pkg_id TEXT COLLATE NOCASE,
  pkg_family TEXT COLLATE NOCASE,
  pkg_name TEXT NOT NULL COLLATE NOCASE,
  pkg_type TEXT COLLATE NOCASE,
  app_id TEXT COLLATE NOCASE,
  description TEXT,
  version TEXT NOT NULL,
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
  binaries JSONB,
  extra JSONB
);

INSERT INTO packages_new SELECT
  id, pkg_id, pkg_family, pkg_name, pkg_type, app_id, description, version,
  licenses, download_url, size, ghcr_pkg, ghcr_size, ghcr_blob, ghcr_url,
  bsum, icon, desktop, appstream, homepages, notes, source_urls, categories,
  build_id, build_date, build_action, build_script, build_log, provides,
  snapshots, replaces, soar_syms, desktop_integration, portable, binaries, extra
FROM packages;

DROP TABLE packages;
ALTER TABLE packages_new RENAME TO packages;

CREATE UNIQUE INDEX packages_identity
  ON packages (COALESCE(pkg_id, ''), pkg_name, version);
