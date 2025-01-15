use rusqlite::{Statement, Transaction};

pub struct DbStatements<'a> {
    pub repo_insert: Statement<'a>,
    pub repo_check: Statement<'a>,
    pub family_insert: Statement<'a>,
    pub family_check: Statement<'a>,
    pub provides_insert: Statement<'a>,
    pub package_insert: Statement<'a>,
}

impl<'a> DbStatements<'a> {
    pub fn new(tx: &'a Transaction) -> rusqlite::Result<Self> {
        Ok(Self {
            repo_insert: tx.prepare("INSERT INTO repository (name) VALUES (?1)")?,
            repo_check: tx.prepare("SELECT name FROM repository LIMIT 1")?,
            family_insert: tx.prepare("INSERT INTO families (value) VALUES (?1)")?,
            family_check: tx.prepare("SELECT id FROM families WHERE value = ?1")?,
            provides_insert: tx
                .prepare("INSERT INTO provides (family_id, package_id) VALUES (?1, ?2)")?,
            package_insert: tx.prepare(
                "INSERT INTO packages (
                    pkg, pkg_name, pkg_id, description, version, download_url,
                    size, checksum, build_date, build_script, build_log,
                    desktop, icon, family_id, homepages, notes, source_urls,
                    categories
                )
                VALUES
                (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                    ?14, ?15, ?16, ?17, ?18
                )",
            )?,
        })
    }
}
