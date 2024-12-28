use rusqlite::{params, Result, Transaction};

use super::{
    models::{RemotePackage, RemotePackageMetadata},
    statements::DbStatements,
};

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

    pub fn import_packages(&mut self, metadata: &RemotePackageMetadata) -> Result<()> {
        self.get_or_create_repo(self.repo_name)?;

        for (col_name, packages) in &metadata.collection {
            let collection_id = self.get_or_create_collection(col_name)?;

            for package in packages {
                self.insert_package(package, collection_id)?;
            }
        }
        Ok(())
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

    fn get_or_create_collection(&mut self, name: &str) -> Result<i64> {
        self.statements
            .collection_check
            .query_row(params![name], |row| row.get(0))
            .or_else(|_| {
                self.statements.collection_insert.execute(params![name])?;
                Ok(self.tx.last_insert_rowid())
            })
    }

    fn get_or_create_icon(&mut self, url: &str) -> Result<i64> {
        self.statements
            .icon_check
            .query_row(params![url], |row| row.get(0))
            .or_else(|_| {
                self.statements.icon_insert.execute(params![url])?;
                Ok(self.tx.last_insert_rowid())
            })
    }

    fn insert_package(&mut self, package: &RemotePackage, collection_id: i64) -> Result<()> {
        // FIXME: need to check provides, and deal with family appropriately
        // currently, it creates new family for each package
        self.statements
            .family_insert
            .execute(params![package.pkg_family.clone().unwrap_or_default()])?;
        let family_id = self.tx.last_insert_rowid();
        let icon_id = self.get_or_create_icon(&package.icon)?;

        self.statements.package_insert.execute(params![
            package.pkg,
            package.pkg_name,
            package.pkg_id,
            package.description,
            package.version,
            package.download_url,
            package.size,
            package.bsum,
            package.build_date,
            package.build_script,
            package.build_log,
            package.category,
            package.desktop,
            family_id,
            icon_id,
            collection_id
        ])?;

        let package_id = self.tx.last_insert_rowid();

        self.statements
            .homepage_insert
            .execute(params![package.homepage, package_id])?;
        self.statements
            .note_insert
            .execute(params![package.note, package_id])?;
        self.statements
            .source_url_insert
            .execute(params![package.src_url, package_id])?;
        self.statements
            .provides_insert
            .execute(params![family_id, package_id])?;

        Ok(())
    }
}
