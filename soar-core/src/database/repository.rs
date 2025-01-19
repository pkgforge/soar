use rusqlite::{params, Result, Transaction};

use super::{models::RemotePackage, statements::DbStatements};

pub struct PackageRepository<'a> {
    tx: &'a Transaction<'a>,
    statements: DbStatements<'a>,
    repo_name: &'a str,
}

impl<'a> PackageRepository<'a> {
    pub fn new(tx: &'a Transaction<'a>, statements: DbStatements<'a>, repo_name: &'a str) -> Self {
        Self {
            tx,
            statements,
            repo_name,
        }
    }

    pub fn import_packages(&mut self, metadata: &[RemotePackage], etag: &str) -> Result<()> {
        self.get_or_create_repo(self.repo_name, etag)?;

        for package in metadata {
            self.insert_package(package)?;
        }
        Ok(())
    }

    fn get_or_create_repo(&mut self, name: &str, etag: &str) -> Result<()> {
        self.statements
            .repo_check
            .query_row([], |_| Ok(()))
            .or_else(|_| {
                self.statements.repo_insert.execute(params![name, etag])?;
                Ok(())
            })
    }

    fn insert_package(&mut self, package: &RemotePackage) -> Result<()> {
        let disabled_reason = serde_json::to_string(&package.disabled_reason).unwrap();
        let homepages = serde_json::to_string(&package.homepages).unwrap();
        let notes = serde_json::to_string(&package.notes).unwrap();
        let source_urls = serde_json::to_string(&package.src_urls).unwrap();
        let tags = serde_json::to_string(&package.tags).unwrap();
        let categories = serde_json::to_string(&package.categories).unwrap();
        self.statements.package_insert.execute(params![
            package.disabled == "true",
            disabled_reason,
            package.pkg,
            package.pkg_id,
            package.pkg_name,
            package.pkg_type,
            package.pkg_webpage,
            package.app_id,
            package.description,
            package.version,
            package.download_url,
            package.size_raw,
            package.ghcr_pkg,
            package.ghcr_size_raw,
            package.bsum,
            homepages,
            notes,
            source_urls,
            tags,
            categories,
            package.icon,
            package.desktop,
            package.build_id,
            package.build_date,
            package.build_script,
            package.build_log,
        ])?;

        Ok(())
    }
}
