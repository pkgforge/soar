//! Database connection management.

use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use diesel::Connection as DieselConnection;
use soar_db::{connection::DbConnection, migration::DbType};

use crate::error::SoarError;

type Result<T> = std::result::Result<T, SoarError>;

/// Diesel-based database connection wrapper.
/// Provides a thread-safe wrapper around soar_db::DbConnection.
pub struct DieselDatabase {
    conn: Arc<Mutex<DbConnection>>,
}

impl DieselDatabase {
    /// Opens a core database connection with migrations.
    pub fn open_core<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = DbConnection::open(path, DbType::Core)
            .map_err(|e| SoarError::Custom(format!("opening core database: {}", e)))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Opens a metadata database connection.
    pub fn open_metadata<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = DbConnection::open_metadata(path)
            .map_err(|e| SoarError::Custom(format!("opening metadata database: {}", e)))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Opens a nests database connection with migrations.
    pub fn open_nests<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = DbConnection::open(path, DbType::Nest)
            .map_err(|e| SoarError::Custom(format!("opening nests database: {}", e)))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Gets a mutable reference to the underlying connection.
    /// Locks the mutex and returns a guard.
    pub fn conn(&self) -> Result<std::sync::MutexGuard<'_, DbConnection>> {
        self.conn.lock().map_err(|_| SoarError::PoisonError)
    }

    /// Executes a function with the connection.
    pub fn with_conn<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut diesel::SqliteConnection) -> diesel::QueryResult<T>,
    {
        let mut conn = self.conn.lock().map_err(|_| SoarError::PoisonError)?;
        f(conn.conn()).map_err(|e| SoarError::Custom(format!("database error: {}", e)))
    }

    /// Executes a function within a transaction.
    pub fn transaction<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut diesel::SqliteConnection) -> diesel::QueryResult<T>,
    {
        let mut conn = self.conn.lock().map_err(|_| SoarError::PoisonError)?;
        conn.conn()
            .transaction(f)
            .map_err(|e| SoarError::Custom(format!("transaction error: {}", e)))
    }

    /// Gets a clone of the Arc for sharing.
    pub fn clone_arc(&self) -> Arc<Mutex<DbConnection>> {
        self.conn.clone()
    }
}

impl Clone for DieselDatabase {
    fn clone(&self) -> Self {
        Self {
            conn: self.conn.clone(),
        }
    }
}

/// Manager for multiple metadata databases (one per repository).
/// Replaces the ATTACH DATABASE pattern with separate connections.
pub struct MetadataManager {
    databases: Vec<(String, DieselDatabase)>,
}

impl MetadataManager {
    pub fn new() -> Self {
        Self {
            databases: Vec::new(),
        }
    }

    /// Adds a metadata database for a repository.
    pub fn add_repo<P: AsRef<Path>>(&mut self, repo_name: &str, path: P) -> Result<()> {
        let db = DieselDatabase::open_metadata(path)?;
        self.databases.push((repo_name.to_string(), db));
        Ok(())
    }

    /// Executes a query function across all repositories and collects results.
    pub fn query_all<F, T>(&self, f: F) -> Result<Vec<(String, T)>>
    where
        F: Fn(&str, &mut diesel::SqliteConnection) -> diesel::QueryResult<T>,
    {
        let mut results = Vec::new();
        for (repo_name, db) in &self.databases {
            let result = db.with_conn(|conn| f(repo_name, conn))?;
            results.push((repo_name.clone(), result));
        }
        Ok(results)
    }

    /// Queries all repositories and flattens results into a single Vec.
    pub fn query_all_flat<F, T>(&self, f: F) -> Result<Vec<T>>
    where
        F: Fn(&str, &mut diesel::SqliteConnection) -> diesel::QueryResult<Vec<T>>,
    {
        let mut results = Vec::new();
        for (repo_name, db) in &self.databases {
            let items = db.with_conn(|conn| f(repo_name, conn))?;
            results.extend(items);
        }
        Ok(results)
    }

    /// Queries a specific repository.
    pub fn query_repo<F, T>(&self, repo_name: &str, f: F) -> Result<Option<T>>
    where
        F: FnOnce(&mut diesel::SqliteConnection) -> diesel::QueryResult<T>,
    {
        for (name, db) in &self.databases {
            if name == repo_name {
                return db.with_conn(f).map(Some);
            }
        }
        Ok(None)
    }

    /// Gets the first match from any repository.
    pub fn find_first<F, T>(&self, f: F) -> Result<Option<T>>
    where
        F: Fn(&str, &mut diesel::SqliteConnection) -> diesel::QueryResult<Option<T>>,
    {
        for (repo_name, db) in &self.databases {
            if let Some(result) = db.with_conn(|conn| f(repo_name, conn))? {
                return Ok(Some(result));
            }
        }
        Ok(None)
    }

    /// Returns the number of repositories.
    pub fn repo_count(&self) -> usize {
        self.databases.len()
    }

    /// Returns the list of repository names.
    pub fn repo_names(&self) -> Vec<&str> {
        self.databases
            .iter()
            .map(|(name, _)| name.as_str())
            .collect()
    }
}

impl Default for MetadataManager {
    fn default() -> Self {
        Self::new()
    }
}
