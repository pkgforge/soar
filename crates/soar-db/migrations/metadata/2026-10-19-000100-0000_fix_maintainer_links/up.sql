-- Rebuilt rather than altered: SQLite cannot change a foreign key in place.
-- `maintainer_id` referenced `packages`, so a link failed whenever no
-- package happened to share the maintainer's id.
CREATE TABLE package_maintainers_new (
  maintainer_id INTEGER NOT NULL,
  package_id INTEGER NOT NULL,
  FOREIGN KEY (maintainer_id) REFERENCES maintainers (id) ON DELETE CASCADE,
  FOREIGN KEY (package_id) REFERENCES packages (id) ON DELETE CASCADE,
  UNIQUE (maintainer_id, package_id)
);

INSERT INTO package_maintainers_new
SELECT maintainer_id, package_id FROM package_maintainers
WHERE maintainer_id IN (SELECT id FROM maintainers)
  AND package_id IN (SELECT id FROM packages);

DROP TABLE package_maintainers;
ALTER TABLE package_maintainers_new RENAME TO package_maintainers;
