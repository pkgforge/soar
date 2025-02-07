CREATE TABLE packages_new (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  repo_name TEXT NOT NULL,
  pkg TEXT COLLATE NOCASE,
  pkg_id TEXT NOT NULL COLLATE NOCASE,
  pkg_name TEXT NOT NULL COLLATE NOCASE,
  pkg_type TEXT COLLATE NOCASE,
  version TEXT NOT NULL,
  size BIGINT NOT NULL,
  checksum TEXT,
  installed_path TEXT NOT NULL,
  installed_date TEXT NOT NULL,
  bin_path TEXT,
  icon_path TEXT,
  desktop_path TEXT,
  appstream_path TEXT,
  profile TEXT NOT NULL,
  pinned BOOLEAN NOT NULL DEFAULT false,
  is_installed BOOLEAN NOT NULL DEFAULT false,
  with_pkg_id BOOLEAN NOT NULL DEFAULT false,
  detached BOOLEAN NOT NULL DEFAULT false,
  unlinked BOOLEAN NOT NULL DEFAULT false,
  provides JSONB
);

INSERT INTO packages_new
SELECT * FROM packages;

DROP TABLE packages;

ALTER TABLE packages_new RENAME TO packages;

CREATE TABLE portable_package_new (
  package_id INTEGER NOT NULL,
  portable_path TEXT,
  portable_home TEXT,
  portable_config TEXT,
  FOREIGN KEY (package_id) REFERENCES packages (id)
);

INSERT INTO portable_package_new 
SELECT * FROM portable_package;

DROP TABLE portable_package;
ALTER TABLE portable_package_new RENAME TO portable_package;
