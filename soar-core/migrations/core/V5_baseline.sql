CREATE TABLE portable_package (
  package_id INTEGER NOT NULL,
  portable_path TEXT,
  portable_home TEXT,
  portable_config TEXT,
  portable_share TEXT,
  FOREIGN KEY (package_id) REFERENCES packages (id) ON DELETE CASCADE
);

CREATE TABLE packages (
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
  profile TEXT NOT NULL,
  pinned BOOLEAN NOT NULL DEFAULT false,
  is_installed BOOLEAN NOT NULL DEFAULT false,
  with_pkg_id BOOLEAN NOT NULL DEFAULT false,
  detached BOOLEAN NOT NULL DEFAULT false,
  unlinked BOOLEAN NOT NULL DEFAULT false,
  provides JSONB
  install_patterns JSONB
);
