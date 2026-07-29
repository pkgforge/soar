-- Rows without an id cannot be represented once the column is required again.
DELETE FROM packages WHERE pkg_id IS NULL;
