//! Database connection management.
//!
//! This module provides connection management for the soar database system.
//! It supports multiple database types:
//!
//! - **Core database**: Tracks installed packages
//! - **Metadata databases**: One per repository, contains package metadata

use std::{collections::HashMap, path::Path};

use diesel::{Connection, ConnectionError, RunQueryDsl, SqliteConnection, sql_query};
use tracing::{debug, trace};

use crate::migration::{DbType, apply_migrations, migrate_metadata_json_to_jsonb};

/// How long to wait for another process to let go of the database.
///
/// Without it SQLite gives up the moment it finds a lock, so two soar
/// processes running at once fail rather than queue.
const BUSY_TIMEOUT_MS: u32 = 5_000;

/// Database connection wrapper with migration support.
pub struct DbConnection {
    conn: SqliteConnection,
}

/// Set the pragmas every connection wants, before anything reads or writes.
fn prepare(conn: &mut SqliteConnection) -> Result<(), ConnectionError> {
    sql_query(format!("PRAGMA busy_timeout = {BUSY_TIMEOUT_MS};"))
        .execute(conn)
        .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;

    // WAL mode for better concurrent access
    sql_query("PRAGMA journal_mode = WAL;")
        .execute(conn)
        .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;

    trace!("busy timeout and WAL journal mode set");

    Ok(())
}

/// Percent-encodes the characters that end or escape the path part of a
/// SQLite URI, so a path holding them still names the right file.
fn uri_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for c in path.chars() {
        match c {
            '%' => encoded.push_str("%25"),
            '?' => encoded.push_str("%3f"),
            '#' => encoded.push_str("%23"),
            c => encoded.push(c),
        }
    }
    encoded
}

impl DbConnection {
    /// Opens a database connection and runs migrations.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the SQLite database file
    /// * `db_type` - Type of database for selecting correct migrations
    ///
    /// # Errors
    ///
    /// Returns an error if the connection fails or migrations fail.
    pub fn open<P: AsRef<Path>>(path: P, db_type: DbType) -> Result<Self, ConnectionError> {
        let path_str = path.as_ref().to_string_lossy();
        debug!(path = %path_str, db_type = ?db_type, "opening database connection");

        let mut conn = SqliteConnection::establish(&path_str)?;
        trace!("database connection established");

        prepare(&mut conn)?;

        apply_migrations(&mut conn, &db_type)
            .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;
        trace!("migrations applied");

        debug!(path = %path_str, "database opened successfully");
        Ok(Self {
            conn,
        })
    }

    /// Opens a database connection in read-only mode.
    ///
    /// Uses `immutable=1` to skip all locking and WAL handling, which avoids
    /// needing to create `-shm`/`-wal` files in root-owned directories.
    /// Skips WAL mode, migrations, and JSON-to-JSONB conversion since
    /// all of those require write access. Suitable for reading system
    /// databases owned by root.
    pub fn open_readonly<P: AsRef<Path>>(path: P) -> Result<Self, ConnectionError> {
        let path_str = format!(
            "file:{}?mode=ro&immutable=1",
            uri_path(&path.as_ref().to_string_lossy())
        );
        debug!(path = %path_str, "opening database in read-only mode");

        let conn = SqliteConnection::establish(&path_str)?;
        trace!("read-only database connection established");

        debug!(path = %path_str, "read-only database opened successfully");
        Ok(Self {
            conn,
        })
    }

    /// Opens a database connection without running migrations.
    ///
    /// Use this when you know the database is already migrated.
    pub fn open_without_migrations<P: AsRef<Path>>(path: P) -> Result<Self, ConnectionError> {
        let path_str = path.as_ref().to_string_lossy();
        debug!(path = %path_str, "opening database without migrations");

        let mut conn = SqliteConnection::establish(&path_str)?;
        trace!("database connection established");

        prepare(&mut conn)?;

        debug!(path = %path_str, "database opened successfully");
        Ok(Self {
            conn,
        })
    }

    /// Opens a metadata database and migrates JSON text columns to JSONB.
    ///
    /// This is used for metadata databases that are generated externally (e.g., by rusqlite)
    /// and may contain JSON stored as text instead of JSONB binary format.
    ///
    /// Does NOT run schema migrations since the schema is managed externally.
    pub fn open_metadata<P: AsRef<Path>>(path: P) -> Result<Self, ConnectionError> {
        let path_str = path.as_ref().to_string_lossy();
        debug!(path = %path_str, "opening metadata database");

        let mut conn = SqliteConnection::establish(&path_str)?;
        trace!("metadata database connection established");

        prepare(&mut conn)?;

        migrate_metadata_json_to_jsonb(&mut conn)
            .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;
        trace!("JSON to JSONB migration completed");

        debug!(path = %path_str, "metadata database opened successfully");
        Ok(Self {
            conn,
        })
    }

    /// Gets a mutable reference to the underlying connection.
    pub fn conn(&mut self) -> &mut SqliteConnection {
        &mut self.conn
    }
}

impl std::ops::Deref for DbConnection {
    type Target = SqliteConnection;

    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}

impl std::ops::DerefMut for DbConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.conn
    }
}

/// Manages database connections for the soar package manager.
///
/// This struct manages separate connections for:
/// - The core database (installed packages)
/// - Multiple metadata databases (one per repository)
///
/// # Example
///
/// ```ignore
/// use soar_db::connection::DatabaseManager;
///
/// let manager = DatabaseManager::new("/path/to/db")?;
///
/// // Access installed packages
/// let installed = manager.core().list_installed()?;
///
/// // Access repository metadata
/// if let Some(metadata_conn) = manager.metadata("pkgforge") {
///     let packages = metadata_conn.search("firefox")?;
/// }
/// ```
pub struct DatabaseManager {
    /// Core database connection (installed packages).
    core: DbConnection,
    /// Metadata database connections, keyed by repository name.
    metadata: HashMap<String, DbConnection>,
}

impl DatabaseManager {
    /// Creates a new database manager with the given base directory.
    ///
    /// # Arguments
    ///
    /// * `base_dir` - Base directory for database files
    ///
    /// The following databases will be created/opened:
    /// - `{base_dir}/core.db` - Installed packages
    ///
    /// Metadata databases are added separately via `add_metadata_db`.
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self, ConnectionError> {
        let base = base_dir.as_ref();
        debug!(base_dir = %base.display(), "initializing database manager");

        let core_path = base.join("core.db");

        let core = DbConnection::open(&core_path, DbType::Core)?;

        debug!("database manager initialized");
        Ok(Self {
            core,
            metadata: HashMap::new(),
        })
    }

    /// Adds or opens a metadata database for a repository.
    ///
    /// This method opens the metadata database and migrates any JSON text columns
    /// to JSONB binary format. It does NOT run schema migrations since metadata
    /// databases are generated externally (e.g., by rusqlite).
    ///
    /// # Arguments
    ///
    /// * `repo_name` - Name of the repository
    /// * `path` - Path to the metadata database file
    pub fn add_metadata_db<P: AsRef<Path>>(
        &mut self,
        repo_name: &str,
        path: P,
    ) -> Result<(), ConnectionError> {
        debug!(repo_name = repo_name, "adding metadata database");
        let conn = DbConnection::open_metadata(path)?;
        self.metadata.insert(repo_name.to_string(), conn);
        trace!(repo_name = repo_name, "metadata database added to manager");
        Ok(())
    }

    /// Gets a mutable reference to the core database connection.
    pub fn core(&mut self) -> &mut DbConnection {
        &mut self.core
    }

    /// Gets a mutable reference to a metadata database connection.
    ///
    /// Returns `None` if no metadata database exists for the given repository.
    pub fn metadata(&mut self, repo_name: &str) -> Option<&mut DbConnection> {
        self.metadata.get_mut(repo_name)
    }

    /// Gets an iterator over all metadata database connections.
    pub fn all_metadata(&mut self) -> impl Iterator<Item = (&String, &mut DbConnection)> {
        self.metadata.iter_mut()
    }

    /// Returns the names of all loaded metadata databases.
    pub fn metadata_names(&self) -> impl Iterator<Item = &String> {
        self.metadata.keys()
    }

    /// Removes a metadata database connection.
    pub fn remove_metadata_db(&mut self, repo_name: &str) -> Option<DbConnection> {
        debug!(repo_name = repo_name, "removing metadata database");
        self.metadata.remove(repo_name)
    }
}

#[cfg(test)]
mod tests {
    use diesel::{QueryResult, QueryableByName, sql_types::Integer};
    use diesel_migrations::MigrationHarness;

    use super::*;
    use crate::migration::CORE_MIGRATIONS;

    #[derive(QueryableByName)]
    struct Count {
        #[diesel(sql_type = Integer)]
        n: i32,
    }

    fn portable_rows(conn: &mut SqliteConnection) -> i32 {
        sql_query("SELECT count(*) AS n FROM portable_package")
            .get_result::<Count>(conn)
            .unwrap()
            .n
    }

    fn add_package(conn: &mut SqliteConnection, id: i32, old_schema: bool) {
        // The old schema requires a `pkg_id`.
        let (columns, values) = if old_schema {
            (", pkg_id", ", 'x'")
        } else {
            ("", "")
        };
        sql_query(format!(
            "INSERT INTO packages (id, repo_name, pkg_name, version, size, installed_path, \
             installed_date, profile{columns}) VALUES ({id}, 'r', 'p', '1', 0, '/p', 'd', \
             'default'{values})"
        ))
        .execute(conn)
        .unwrap();
    }

    fn add_portable(conn: &mut SqliteConnection, package_id: i32) -> QueryResult<usize> {
        sql_query(format!(
            "INSERT INTO portable_package (package_id, portable_home) VALUES ({package_id}, '/h')"
        ))
        .execute(conn)
    }

    #[test]
    fn core_foreign_keys_are_enforced() {
        let mut db = DbConnection::open(":memory:", DbType::Core).unwrap();
        let conn = db.conn();
        add_package(conn, 1, false);
        add_portable(conn, 1).unwrap();

        assert!(add_portable(conn, 99).is_err(), "row for a missing package");

        sql_query("DELETE FROM packages WHERE id = 1")
            .execute(conn)
            .unwrap();
        assert_eq!(portable_rows(conn), 0, "portable row outlived its package");
    }

    #[test]
    fn upgrading_keeps_portable_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("soar.db");
        {
            // The schema as it stood before `packages` was rebuilt.
            let mut conn = SqliteConnection::establish(path.to_str().unwrap()).unwrap();
            for _ in 0..3 {
                conn.run_next_migration(CORE_MIGRATIONS).unwrap();
            }
            add_package(&mut conn, 1, true);
            add_portable(&mut conn, 1).unwrap();
        }

        let mut db = DbConnection::open(&path, DbType::Core).unwrap();
        assert_eq!(
            portable_rows(db.conn()),
            1,
            "upgrading lost the package's portable settings"
        );
    }

    #[test]
    fn readonly_opens_a_path_with_uri_characters() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("a?b#c%d");
        std::fs::create_dir(&sub).unwrap();
        let path = sub.join("soar.db");
        drop(DbConnection::open(&path, DbType::Core).unwrap());

        let mut db = DbConnection::open_readonly(&path).unwrap();
        let tables = sql_query(
            "SELECT count(*) AS n FROM sqlite_master WHERE type = 'table' AND name = 'packages'",
        )
        .get_result::<Count>(db.conn())
        .unwrap();
        assert_eq!(tables.n, 1);
    }
}
