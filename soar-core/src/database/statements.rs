use rusqlite::{Statement, Transaction};

pub struct DbStatements<'a> {
    pub repo_insert: Statement<'a>,
    pub package_insert: Statement<'a>,
    pub maintainer_insert: Statement<'a>,
    pub maintainer_check: Statement<'a>,
    pub pkg_maintainer_insert: Statement<'a>,
}

impl<'a> DbStatements<'a> {
    pub fn new(tx: &'a Transaction) -> rusqlite::Result<Self> {
        Ok(Self {
            repo_insert: tx.prepare(
                "INSERT INTO repository (name, etag)
                VALUES (?1, ?2)
                ON CONFLICT (name) DO UPDATE SET etag = ?2",
            )?,
            maintainer_insert: tx
                .prepare("INSERT INTO maintainers (name, contact) VALUES (?1, ?2)")?,
            maintainer_check: tx.prepare("SELECT id FROM maintainers WHERE contact=?1 LIMIT 1")?,
            pkg_maintainer_insert: tx.prepare(
                "INSERT INTO package_maintainers (
                        maintainer_id, package_id
                    ) VALUES (?1, ?2)
                    ON CONFLICT (maintainer_id, package_id) DO NOTHING",
            )?,
            package_insert: tx.prepare(
                "INSERT INTO packages (
                    disabled, disabled_reason, rank, pkg, pkg_id, pkg_name,
                    pkg_family, pkg_type, pkg_webpage, app_id, description,
                    version, version_upstream, licenses, download_url,
                    size, ghcr_pkg, ghcr_size, ghcr_files, ghcr_blob, ghcr_url,
                    bsum, shasum, icon, desktop, appstream, homepages, notes,
                    source_urls, tags, categories, build_id, build_date,
                    build_action, build_script, build_log, provides, snapshots,
                    repology, replaces, download_count, download_count_week,
                    download_count_month, bundle, bundle_type, soar_syms,
                    deprecated, desktop_integration, external, installable,
                    portable, recurse_provides, trusted, version_latest,
                    version_outdated
                )
                VALUES
                (
                    ?1, jsonb(?2), ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                    ?13, jsonb(?14), ?15, ?16, ?17, ?18, jsonb(?19), ?20, ?21,
                    ?22, ?23, ?24, ?25, ?26, jsonb(?27), jsonb(?28), jsonb(?29),
                    jsonb(?30), jsonb(?31), ?32, ?33, ?34, ?35, ?36, jsonb(?37),
                    jsonb(?38), jsonb(?39), jsonb(?40), ?41, ?42, ?43, ?44,
                    ?45, ?46, ?47, ?48, ?49, ?50, ?51, ?52, ?53, ?54, ?55
                )
                ON CONFLICT (pkg_id, pkg_name, version) DO NOTHING",
            )?,
        })
    }
}
