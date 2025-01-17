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

    pub fn import_packages(&mut self, metadata: &[RemotePackage]) -> Result<()> {
        self.get_or_create_repo(self.repo_name)?;

        for package in metadata {
            self.insert_package(package)?;
        }
        Ok(())
    }

    fn get_or_create_family(&mut self, value: &str) -> Result<i64> {
        self.statements
            .family_check
            .query_row(params![value], |row| row.get(0))
            .or_else(|_| {
                self.statements.family_insert.execute(params![value])?;
                Ok(self.tx.last_insert_rowid())
            })
    }

    fn get_or_create_repo(&mut self, name: &str) -> Result<()> {
        self.statements
            .repo_check
            .query_row([], |_| Ok(()))
            .or_else(|_| {
                self.statements.repo_insert.execute(params![name])?;
                Ok(())
            })
    }

    fn insert_package(&mut self, package: &RemotePackage) -> Result<()> {
        let family_id = self.get_or_create_family(&package.pkg_id)?;
        let homepages = serde_json::to_string(&package.homepages).unwrap();
        let notes = serde_json::to_string(&package.notes).unwrap();
        let source_urls = serde_json::to_string(&package.src_urls).unwrap();
        let categories = serde_json::to_string(&package.categories).unwrap();
        self.statements.package_insert.execute(params![
            package.pkg,
            package.pkg_name,
            package.pkg_id,
            package.pkg_type,
            package.description,
            package.version,
            package.download_url,
            package.size_raw,
            package.bsum,
            package.build_date,
            package.build_script,
            package.build_log,
            package.desktop,
            package.icon,
            family_id,
            homepages,
            notes,
            source_urls,
            categories
        ])?;

        let package_id = self.tx.last_insert_rowid();

        self.statements
            .provides_insert
            .execute(params![family_id, package_id])?;

        Ok(())
    }
}
