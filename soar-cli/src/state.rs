use std::{
    fs::File,
    path::PathBuf,
    sync::{Arc, Mutex, RwLockReadGuard},
};

use once_cell::sync::OnceCell;
use rusqlite::Connection;
use soar_core::{
    config::{get_config, Config},
    constants::CORE_MIGRATIONS,
    database::{connection::Database, migration::MigrationManager},
    error::{ErrorContext, SoarError},
    metadata::fetch_metadata,
    SoarResult,
};

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    config: RwLockReadGuard<'static, Config>,
    repo_db: OnceCell<Database>,
    core_db: OnceCell<Database>,
}

impl AppState {
    pub fn new() -> Self {
        let config = get_config();

        Self {
            inner: Arc::new(AppStateInner {
                config,
                repo_db: OnceCell::new(),
                core_db: OnceCell::new(),
            }),
        }
    }

    pub async fn sync(&self) -> SoarResult<()> {
        self.init_repo_dbs(true).await
    }

    async fn init_repo_dbs(&self, force: bool) -> SoarResult<()> {
        let mut tasks = Vec::new();

        for repo in &self.inner.config.repositories {
            let repo_clone = repo.clone();
            let task = tokio::task::spawn(async move { fetch_metadata(repo_clone, force).await });
            tasks.push(task);
        }

        for task in tasks {
            task.await
                .map_err(|err| SoarError::Custom(format!("Join handle error: {}", err)))??;
        }
        Ok(())
    }

    fn create_repo_db(&self) -> SoarResult<Database> {
        let repo_paths: Vec<PathBuf> = self
            .inner
            .config
            .repositories
            .iter()
            .map(|r| r.get_path().unwrap().join("metadata.db"))
            .collect();

        Database::new_multi(repo_paths.as_ref())
    }

    fn create_core_db(&self) -> SoarResult<Database> {
        let core_db_file = self.inner.config.get_db_path()?.join("soar.db");
        if !core_db_file.exists() {
            File::create(&core_db_file)
                .with_context(|| format!("creating database file {}", core_db_file.display()))?;
        }

        let conn = Connection::open(&core_db_file)?;
        let mut manager = MigrationManager::new(conn)?;
        manager.migrate_from_dir(CORE_MIGRATIONS)?;
        Database::new(&core_db_file)
    }

    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    pub async fn repo_db(&self) -> SoarResult<&Arc<Mutex<Connection>>> {
        self.init_repo_dbs(false).await?;
        self.inner
            .repo_db
            .get_or_try_init(|| self.create_repo_db())
            .map(|db| &db.conn)
    }

    pub fn core_db(&self) -> SoarResult<&Arc<Mutex<Connection>>> {
        self.inner
            .core_db
            .get_or_try_init(|| self.create_core_db())
            .map(|db| &db.conn)
    }
}
