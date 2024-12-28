use rusqlite::{Statement, Transaction};

pub struct DbStatements<'a> {
    pub repo_insert: Statement<'a>,
    pub repo_check: Statement<'a>,
    pub collection_insert: Statement<'a>,
    pub collection_check: Statement<'a>,
    pub family_insert: Statement<'a>,
    pub homepage_insert: Statement<'a>,
    pub note_insert: Statement<'a>,
    pub source_url_insert: Statement<'a>,
    pub icon_insert: Statement<'a>,
    pub icon_check: Statement<'a>,
    pub provides_insert: Statement<'a>,
    pub package_insert: Statement<'a>,
}

impl<'a> DbStatements<'a> {
    pub fn new(tx: &'a Transaction) -> rusqlite::Result<Self> {
        Ok(Self {
            repo_insert: tx.prepare("INSERT INTO repository (name) VALUES (?1)")?,
            repo_check: tx.prepare("SELECT name FROM repository LIMIT 1")?,
            collection_insert: tx.prepare("INSERT INTO collections (name) VALUES (?1)")?,
            collection_check: tx.prepare("SELECT id FROM collections WHERE name = ?1")?,
            family_insert: tx.prepare("INSERT INTO families (name) Values (?1)")?,
            homepage_insert: tx
                .prepare("INSERT INTO homepages (url, package_id) Values (?1, ?2)")?,
            note_insert: tx.prepare("INSERT INTO notes (note, package_id) Values (?1, ?2)")?,
            source_url_insert: tx
                .prepare("INSERT INTO source_urls (url, package_id) Values (?1, ?2)")?,
            icon_insert: tx.prepare("INSERT INTO icons (url) Values (?1)")?,
            icon_check: tx.prepare("SELECT id FROM icons WHERE url = ?1")?,
            provides_insert: tx
                .prepare("INSERT INTO provides (family_id, package_id) Values (?1, ?2)")?,
            package_insert: tx.prepare(
                "INSERT INTO packages (
                    pkg, pkg_name, pkg_id, description, version, download_url, size,
                    checksum, build_date, build_script, build_log, category,
                    desktop, family_id, icon_id, collection_id
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            )?,
        })
    }
}
