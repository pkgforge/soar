-- Which project an installed package came from, so variants can be told apart
-- by family rather than by the package id repositories no longer publish.
ALTER TABLE packages ADD COLUMN pkg_family TEXT;
