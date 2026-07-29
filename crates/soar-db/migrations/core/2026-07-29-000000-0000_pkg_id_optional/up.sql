-- Repositories publishing the declarative format do not produce a package id,
-- so installed rows must be able to omit it. SQLite cannot relax NOT NULL in
-- place, so the table is rebuilt.
CREATE TABLE packages_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_name TEXT NOT NULL,
    pkg_id TEXT,
    pkg_name TEXT NOT NULL,
    pkg_family TEXT,
    pkg_type TEXT,
    version TEXT NOT NULL,
    size BIGINT NOT NULL,
    checksum TEXT,
    installed_path TEXT NOT NULL,
    installed_date TEXT NOT NULL,
    profile TEXT NOT NULL,
    pinned BOOLEAN NOT NULL DEFAULT 0,
    is_installed BOOLEAN NOT NULL DEFAULT 0,
    detached BOOLEAN NOT NULL DEFAULT 0,
    unlinked BOOLEAN NOT NULL DEFAULT 0,
    provides BLOB,
    install_patterns BLOB
);

INSERT INTO packages_new
SELECT id, repo_name, pkg_id, pkg_name, pkg_family, pkg_type, version, size,
       checksum, installed_path, installed_date, profile, pinned, is_installed,
       detached, unlinked, provides, install_patterns
FROM packages;

DROP TABLE packages;
ALTER TABLE packages_new RENAME TO packages;
