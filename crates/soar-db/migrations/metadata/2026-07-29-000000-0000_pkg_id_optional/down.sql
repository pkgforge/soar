DROP INDEX IF EXISTS packages_identity;
DELETE FROM packages WHERE pkg_id IS NULL;
