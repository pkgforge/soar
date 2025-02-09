PRAGMA foreign_keys=off;

CREATE TABLE portable_package_new (
  package_id INTEGER NOT NULL,
  portable_path TEXT,
  portable_home TEXT,
  portable_config TEXT,
  FOREIGN KEY (package_id) REFERENCES packages (id) ON DELETE CASCADE
);

INSERT INTO portable_package_new
SELECT * FROM portable_package;

DROP TABLE portable_package;
ALTER TABLE portable_package_new RENAME TO portable_package;

PRAGMA foreign_keys=on;
