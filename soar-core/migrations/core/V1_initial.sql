CREATE TABLE portable_package (
  package_id INTEGER NOT NULL,
  portable_path TEXT,
  portable_home TEXT,
  portable_config TEXT,
  FOREIGN KEY (package_id) REFERENCES packages (id)
);

CREATE TABLE packages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  repo_name TEXT NOT NULL,
  pkg TEXT NOT NULL,
  pkg_id TEXT NOT NULL,
  pkg_name TEXT NOT NULL,
  pkg_type TEXT NOT NULL,
  version TEXT NOT NULL,
  size BIGINT NOT NULL,
  checksum TEXT NOT NULL,
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
