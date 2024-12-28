CREATE TABLE repositories (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE,
  url TEXT NOT NULL,
  metadata TEXT,
  sandbox BOOLEAN,
  disabled BOOLEAN NOT NULL DEFAULT false
);

CREATE TABLE configs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  soar_root TEXT NOT NULL,
  soar_cache TEXT,
  soar_bin TEXT,
  soar_db TEXT,
  soar_packages TEXT
);

CREATE TABLE profiles (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE,
  config_id NOT NULL,
  sandbox BOOLEAN,
  is_default BOOLEAN NOT NULL DEFAULT false,
  FOREIGN KEY (config_id) REFERENCES configs (id)
);

CREATE TABLE global_config (
  parallel BOOLEAN,
  parallel_limit INTEGER,
  search_limit INTEGER,
  sandbox BOOLEAN
);

CREATE TABLE sandbox_rules (
  package_id INTEGER NOT NULL,
  fs_read TEXT,
  fs_write TEXT,
  net TEXT,
  FOREIGN KEY (package_id) REFERENCES packages (id)
);

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
  collection TEXT NOT NULL,
  family TEXT NOT NULL,
  pkg_name TEXT NOT NULL,
  pkg TEXT NOT NULL,
  pkg_id TEXT NOT NULL,
  app_id TEXT,
  description TEXT,
  version TEXT NOT NULL,
  size INTEGER NOT NULL,
  checksum TEXT NOT NULL,
  build_date TEXT NOT NULL,
  build_script TEXT NOT NULL,
  build_log TEXT NOT NULL,
  category TEXT,
  installed_path TEXT NOT NULL,
  bin_path TEXT,
  installed_date TEXT,
  disabled BOOLEAN NOT NULL DEFAULT false,
  pinned BOOLEAN NOT NULL DEFAULT false,
  is_installed BOOLEAN NOT NULL DEFAULT false,
  installed_with_family BOOLEAN NOT NULL DEFAULT false,
  UNIQUE (family, pkg_name, checksum)
);
