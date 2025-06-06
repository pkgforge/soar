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

    pub fn import_packages(&mut self, metadata: &[RemotePackage]) -> Result<()> {
        self.statements
            .repo_insert
            // to prevent incomplete sync, etag should only be updated once
            // all checks are done
            .execute(params![self.repo_name, ""])?;

        for package in metadata {
            self.insert_package(package)?;
        }
        Ok(())
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
        let replaces = serde_json::to_string(&package.replaces).unwrap();

        const PROVIDES_DELIMITERS: &[&str] = &["==", "=>", ":"];

        let provides = package.provides.as_ref().map(|vec| {
            vec.iter()
                .filter_map(|p| {
                    let include = *p == package.pkg_name
                        || matches!(package.recurse_provides, Some(true))
                        || p.strip_prefix(&package.pkg_name).is_some_and(|rest| {
                            PROVIDES_DELIMITERS.iter().any(|d| rest.starts_with(d))
                        });

                    include.then(|| PackageProvide::from_string(p))
                })
                .collect::<Vec<_>>()
        });

        let provides = serde_json::to_string(&provides).unwrap();
        let inserted = self.statements.package_insert.execute(params![
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
            replaces,
            package.download_count,
            package.download_count_week,
            package.download_count_month,
            package.bundle.unwrap_or(false),
            package.bundle_type,
            package.soar_syms.unwrap_or(false),
            package.deprecated.unwrap_or(false),
            package.desktop_integration,
            package.external,
            package.installable,
            package.portable,
            package.recurse_provides,
            package.trusted,
            package.version_latest,
            package.version_outdated
        ])?;
        if inserted == 0 {
            return Ok(());
        }
        let package_id = self.tx.last_insert_rowid();
        for maintainer in &package.maintainers.clone().unwrap_or_default() {
            let typed = self.extract_name_and_contact(maintainer);
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
