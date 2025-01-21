use regex::Regex;
use rusqlite::{params, Result, Transaction};

use super::{models::RemotePackage, packages::PackageProvide, statements::DbStatements};

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

    fn get_or_create_maintainer(&mut self, name: &str, contact: &str) -> Result<i64> {
        self.statements
            .maintainer_check
            .query_row(params![contact], |row| row.get(0))
            .or_else(|_| {
                self.statements
                    .maintainer_insert
                    .execute(params![name, contact])?;
                Ok(self.tx.last_insert_rowid())
            })
    }

    fn extract_name_and_contact(&self, input: &str) -> Option<(String, String)> {
        let re = Regex::new(r"^([^()]+) \(([^)]+)\)$").unwrap();

        if let Some(captures) = re.captures(input) {
            let name = captures.get(1).map_or("", |m| m.as_str()).to_string();
            let contact = captures.get(2).map_or("", |m| m.as_str()).to_string();
            Some((name, contact))
        } else {
            None
        }
    }

    fn insert_package(&mut self, package: &RemotePackage) -> Result<()> {
        let disabled_reason = serde_json::to_string(&package.disabled_reason).unwrap();
        let licenses = serde_json::to_string(&package.licenses).unwrap();
        let ghcr_files = serde_json::to_string(&package.ghcr_files).unwrap();
        let homepages = serde_json::to_string(&package.homepages).unwrap();
        let notes = serde_json::to_string(&package.notes).unwrap();
        let source_urls = serde_json::to_string(&package.src_urls).unwrap();
        let tags = serde_json::to_string(&package.tags).unwrap();
        let categories = serde_json::to_string(&package.categories).unwrap();
        let snapshots = serde_json::to_string(&package.snapshots).unwrap();
        let repology = serde_json::to_string(&package.repology).unwrap();

        let provides = package.provides.clone().map(|vec| {
            vec.iter()
                .filter_map(|p| {
                    if p.split_once("==").is_some()
                        || p.split_once("=>").is_some()
                        || p.split_once(":").is_some()
                        || *p == package.pkg_name
                    {
                        Some(PackageProvide::from_string(p))
                    } else {
                        None
                    }
                })
                .collect::<Vec<PackageProvide>>()
        });
        let provides = serde_json::to_string(&provides).unwrap();
        self.statements.package_insert.execute(params![
            package.disabled,
            disabled_reason,
            package.rank,
            package.pkg,
            package.pkg_id,
            package.pkg_name,
            package.pkg_family,
            package.pkg_type,
            package.pkg_webpage,
            package.app_id,
            package.description,
            package.version,
            package.version_upstream,
            licenses,
            package.download_url,
            package.size_raw,
            package.ghcr_pkg,
            package.ghcr_size_raw,
            ghcr_files,
            package.ghcr_blob,
            package.ghcr_url,
            package.bsum,
            package.shasum,
            package.icon,
            package.desktop,
            package.appstream,
            homepages,
            notes,
            source_urls,
            tags,
            categories,
            package.build_id,
            package.build_date,
            package.build_action,
            package.build_script,
            package.build_log,
            provides,
            snapshots,
            repology,
            package.download_count,
            package.download_count_week,
            package.download_count_month
        ])?;
        let package_id = self.tx.last_insert_rowid();
        for maintainer in &package.maintainers {
            let typed = self.extract_name_and_contact(&maintainer);
            if let Some((name, contact)) = typed {
                let maintainer_id = self.get_or_create_maintainer(&name, &contact)?;
                self.statements
                    .pkg_maintainer_insert
                    .execute(params![maintainer_id, package_id])?;
            }
        }

        Ok(())
    }
}
