-- pkg_webpage was a repository-specific URL derivable from the package's own
-- fields, and tags were stored and displayed but never searched or queried.
ALTER TABLE packages DROP COLUMN pkg_webpage;
ALTER TABLE packages DROP COLUMN tags;
