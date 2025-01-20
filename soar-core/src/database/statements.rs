use rusqlite::{Statement, Transaction};

pub struct DbStatements<'a> {
    pub repo_insert: Statement<'a>,
    pub repo_check: Statement<'a>,
    pub package_insert: Statement<'a>,
}

impl<'a> DbStatements<'a> {
    pub fn new(tx: &'a Transaction) -> rusqlite::Result<Self> {
        Ok(Self {
            repo_insert: tx.prepare("INSERT INTO repository (name, etag) VALUES (?1, ?2)")?,
            repo_check: tx.prepare("SELECT name FROM repository LIMIT 1")?,
            package_insert: tx.prepare(
                "INSERT INTO packages (
                    disabled, disabled_reason, pkg, pkg_id, pkg_name, pkg_type,
                    pkg_webpage, app_id, description, version, download_url,
                    size, ghcr_pkg, ghcr_size, checksum, homepages, notes,
                    source_urls, tags, categories, icon, desktop, build_id,
                    build_date, build_script, build_log, provides
                )
                VALUES
                (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                    ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25,
                    ?26, ?27
                )",
            )?,
        })
    }
}
