CREATE TABLE package_maintainers_old (
  maintainer_id INTEGER NOT NULL,
  package_id INTEGER NOT NULL,
  FOREIGN KEY (maintainer_id) REFERENCES packages (id),
  FOREIGN KEY (package_id) REFERENCES packages (id),
  UNIQUE (maintainer_id, package_id)
);

INSERT INTO package_maintainers_old
SELECT maintainer_id, package_id FROM package_maintainers
WHERE maintainer_id IN (SELECT id FROM packages);

DROP TABLE package_maintainers;
ALTER TABLE package_maintainers_old RENAME TO package_maintainers;
