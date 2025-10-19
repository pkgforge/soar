CREATE TABLE portable_package (
  package_id INTEGER NOT NULL,
  portable_path TEXT,
  portable_home TEXT,
  portable_config TEXT,
  portable_share TEXT,
  portable_cache TEXT,
  FOREIGN KEY (package_id) REFERENCES packages (id) ON DELETE CASCADE
);
