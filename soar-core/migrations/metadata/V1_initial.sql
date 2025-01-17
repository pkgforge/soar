CREATE TABLE repository (
  name TEXT NOT NULL UNIQUE
);

CREATE TABLE families (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  value TEXT NOT NULL
);

CREATE TABLE provides (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  family_id INTEGER NOT NULL,
  package_id INTEGER NOT NULL,
  FOREIGN KEY (package_id) REFERENCES packages (id),
  FOREIGN KEY (family_id) REFERENCES families (id),
  UNIQUE (family_id, package_id)
);

CREATE TABLE maintainers (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  contact TEXT NOT NULL,
  name TEXT NOT NULL
);

CREATE TABLE package_maintainers (
  maintainer_id INTEGER NOT NULL,
  package_id INTEGER NOT NULL,
  FOREIGN KEY (maintainer_id) REFERENCES packages (id),
  FOREIGN KEY (package_id) REFERENCES packages (id),
  UNIQUE (maintainer_id, package_id)
);

CREATE TABLE packages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  disabled BOOLEAN NOT NULL DEFAULT false,
  disabled_reason JSONB,
  pkg TEXT NOT NULL,
  pkg_id TEXT,
  pkg_name TEXT NOT NULL,
  pkg_type TEXT NOT NULL,
  app_id TEXT,
  description TEXT,
  version TEXT NOT NULL,
  download_url TEXT NOT NULL,
  size BIGINT NOT NULL,
  ghcr_pkg TEXT,
  ghcr_url TEXT,
  checksum TEXT NOT NULL,
  icon TEXT,
  desktop TEXT,
  homepages JSONB,
  notes JSONB,
  source_urls JSONB,
  tags JSONB,
  categories JSONB,
  build_date TEXT NOT NULL,
  build_script TEXT NOT NULL,
  build_log TEXT NOT NULL,
  family_id INTEGER NOT NULL,
  FOREIGN KEY (family_id) REFERENCES families (id)
);
