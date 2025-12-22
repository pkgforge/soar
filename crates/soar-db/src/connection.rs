//! Database connection management.
//!
//! This module provides connection management for the soar database system.
//! It supports multiple database types:
//!
//! - **Core database**: Tracks installed packages
//! - **Metadata databases**: One per repository, contains package metadata
//! - **Nests database**: Tracks nest configurations

use std::collections::HashMap;
use std::path::Path;

use diesel::{sql_query, Connection, ConnectionError, RunQueryDsl, SqliteConnection};

use crate::migration::{apply_migrations, migrate_json_to_jsonb, DbType};

/// Database connection wrapper with migration support.
pub struct DbConnection {
    conn: SqliteConnection,
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
        let mut conn = SqliteConnection::establish(&path_str)?;

        // WAL mode for better concurrent access
        sql_query("PRAGMA journal_mode = WAL;")
            .execute(&mut conn)
            .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;

        apply_migrations(&mut conn, &db_type)
            .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;

        // Migrate text JSON to JSONB for databases we manage (Core, Nest)
        // Metadata databases are generated externally and migrated on fetch
        if matches!(db_type, DbType::Core | DbType::Nest) {
            migrate_json_to_jsonb(&mut conn, db_type)
                .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;
        }

        Ok(Self { conn })
    }

    /// Opens a database connection without running migrations.
    ///
    /// Use this when you know the database is already migrated.
    pub fn open_without_migrations<P: AsRef<Path>>(path: P) -> Result<Self, ConnectionError> {
        let path_str = path.as_ref().to_string_lossy();
        let conn = SqliteConnection::establish(&path_str)?;
        Ok(Self { conn })
    }

    /// Opens a metadata database and migrates JSON text columns to JSONB.
    ///
    /// This is used for metadata databases that are generated externally (e.g., by rusqlite)
    /// and may contain JSON stored as text instead of JSONB binary format.
    ///
    /// Does NOT run schema migrations since the schema is managed externally.
    pub fn open_metadata<P: AsRef<Path>>(path: P) -> Result<Self, ConnectionError> {
        let path_str = path.as_ref().to_string_lossy();
        let mut conn = SqliteConnection::establish(&path_str)?;

        // Migrate text JSON to JSONB binary format
        migrate_json_to_jsonb(&mut conn, DbType::Metadata)
            .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;

        Ok(Self { conn })
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
/// - The nests database (nest configurations)
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
    /// Nests database connection.
    nests: DbConnection,
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
    /// - `{base_dir}/nests.db` - Nest configurations
    ///
    /// Metadata databases are added separately via `add_metadata_db`.
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self, ConnectionError> {
        let base = base_dir.as_ref();

        let core_path = base.join("core.db");
        let nests_path = base.join("nests.db");

        let core = DbConnection::open(&core_path, DbType::Core)?;
        let nests = DbConnection::open(&nests_path, DbType::Nest)?;

        Ok(Self {
            core,
            metadata: HashMap::new(),
            nests,
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
        let conn = DbConnection::open_metadata(path)?;
        self.metadata.insert(repo_name.to_string(), conn);
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

    /// Gets a mutable reference to the nests database connection.
    pub fn nests(&mut self) -> &mut DbConnection {
        &mut self.nests
    }

    /// Returns the names of all loaded metadata databases.
    pub fn metadata_names(&self) -> impl Iterator<Item = &String> {
        self.metadata.keys()
    }

    /// Removes a metadata database connection.
    pub fn remove_metadata_db(&mut self, repo_name: &str) -> Option<DbConnection> {
        self.metadata.remove(repo_name)
    }
}
