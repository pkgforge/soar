use std::{
    fs::{self, File},
    path::Path,
    sync::Arc,
};

use nu_ansi_term::Color::{Blue, Green, Magenta, Red};
use once_cell::sync::OnceCell;
use soar_config::{
    config::{get_config, Config},
    repository::Repository,
};
use soar_core::{
    database::connection::{DieselDatabase, MetadataManager},
    error::{ErrorContext, SoarError},
    utils::get_nests_db_conn,
    SoarResult,
};
use soar_db::{
    connection::DbConnection,
    migration::DbType,
    repository::{core::CoreRepository, metadata::MetadataRepository, nest::NestRepository},
};
use soar_registry::{
    fetch_metadata, fetch_nest_metadata, write_metadata_db, MetadataContent, RemotePackage,
};
use tokio::sync::OnceCell as AsyncOnceCell;
use tracing::{debug, error, info, trace};

use crate::utils::Colored;

fn handle_json_metadata<P: AsRef<Path>>(
    metadata: &[RemotePackage],
    metadata_db: P,
    repo_name: &str,
) -> SoarResult<()> {
    let metadata_db = metadata_db.as_ref();
    if metadata_db.exists() {
        fs::remove_file(metadata_db)
            .with_context(|| format!("removing metadata file {}", metadata_db.display()))?;
    }

    let mut conn = DbConnection::open(metadata_db, DbType::Metadata)
        .map_err(|e| SoarError::Custom(format!("opening metadata database: {}", e)))?;

    MetadataRepository::import_packages(conn.conn(), metadata, repo_name)
        .map_err(|e| SoarError::Custom(format!("importing packages: {}", e)))?;

    Ok(())
}

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    config: Config,
    diesel_core_db: OnceCell<DieselDatabase>,
    metadata_manager: AsyncOnceCell<MetadataManager>,
}

impl AppState {
    pub fn new() -> Self {
        trace!("creating new AppState");
        let config = get_config();

        Self {
            inner: Arc::new(AppStateInner {
                config,
                diesel_core_db: OnceCell::new(),
                metadata_manager: AsyncOnceCell::new(),
            }),
        }
    }

    pub async fn sync(&self) -> SoarResult<()> {
        debug!("starting sync");
        self.init_repo_dbs(true).await?;
        self.sync_nests(true).await
    }

    async fn sync_nests(&self, force: bool) -> SoarResult<()> {
        debug!(force = force, "syncing nests");
        let mut nests_db = get_nests_db_conn()?;
        let nests = NestRepository::list_all(nests_db.conn())
            .map_err(|e| SoarError::Custom(format!("listing nests: {}", e)))?;
        trace!(count = nests.len(), "found nests to sync");

        let nests_repo_path = self.config().get_repositories_path()?.join("nests");

        let mut tasks = Vec::new();

        for nest in nests {
            let etag = self.read_nest_etag(&nest.name);
            let registry_nest = soar_registry::Nest {
                id: nest.id as i64,
                name: nest.name.clone(),
                url: nest.url.clone(),
            };
            let task = tokio::task::spawn(async move {
                fetch_nest_metadata(&registry_nest, force, etag).await
            });
            tasks.push((task, nest));
        }

        for (task, nest) in tasks {
            match task
                .await
                .map_err(|err| SoarError::Custom(format!("Join handle error: {err}")))?
            {
                Ok(Some((etag, content))) => {
                    let nest_path = nests_repo_path.join(&nest.name);
                    let metadata_db_path = nest_path.join("metadata.db");
                    let nest_name = format!("nest-{}", nest.name);

                    match content {
                        MetadataContent::SqliteDb(db_bytes) => {
                            write_metadata_db(&db_bytes, &metadata_db_path)
                                .map_err(|e| SoarError::Custom(e.to_string()))?;
                        }
                        MetadataContent::Json(packages) => {
                            handle_json_metadata(&packages, &metadata_db_path, &nest_name)?;
                        }
                    }

                    let db = DieselDatabase::open_metadata(&metadata_db_path)?;
                    db.with_conn(|conn| {
                        MetadataRepository::update_repo_metadata(conn, &nest_name, &etag)
                    })?;
                    info!("[{}] Nest synced", Colored(Magenta, &nest.name))
                }
                Err(err) => error!("Failed to sync nest {}: {err}", nest.name),
                _ => {}
            }
        }

        Ok(())
    }

    async fn init_repo_dbs(&self, force: bool) -> SoarResult<()> {
        debug!(
            force = force,
            repos = self.inner.config.repositories.len(),
            "initializing repository databases"
        );
        let mut tasks = Vec::new();

        for repo in &self.inner.config.repositories {
            trace!(repo_name = repo.name, "scheduling repository sync");
            let repo_clone = repo.clone();
            let etag = self.read_repo_etag(&repo_clone);
            let task =
                tokio::task::spawn(async move { fetch_metadata(&repo_clone, force, etag).await });
            tasks.push((task, repo));
        }

        for (task, repo) in tasks {
            match task
                .await
                .map_err(|err| SoarError::Custom(format!("Join handle error: {err}")))?
            {
                Ok(Some((etag, content))) => {
                    let repo_path = repo.get_path()?;
                    let metadata_db_path = repo_path.join("metadata.db");

                    match content {
                        MetadataContent::SqliteDb(db_bytes) => {
                            write_metadata_db(&db_bytes, &metadata_db_path)
                                .map_err(|e| SoarError::Custom(e.to_string()))?;
                        }
                        MetadataContent::Json(packages) => {
                            handle_json_metadata(&packages, &metadata_db_path, &repo.name)?;
                        }
                    }

                    self.validate_packages(repo, &etag).await?;
                    info!("[{}] Repository synced", Colored(Magenta, &repo.name));
                }
                Err(err) => error!("Failed to sync repository {}: {err}", repo.name),
                _ => {}
            };
        }

        Ok(())
    }

    async fn validate_packages(&self, repo: &Repository, etag: &str) -> SoarResult<()> {
        trace!(
            repo_name = repo.name,
            "validating installed packages against repository"
        );
        let diesel_core_db = self.diesel_core_db()?;
        let repo_name = repo.name.clone();

        let repo_path = repo.get_path()?;
        let metadata_db_path = repo_path.join("metadata.db");

        let metadata_db = DieselDatabase::open_metadata(&metadata_db_path)?;

        let installed_packages = diesel_core_db.with_conn(|conn| {
            CoreRepository::list_filtered(
                conn,
                Some(&repo_name),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )
        })?;

        for pkg in installed_packages {
            let exists = metadata_db
                .with_conn(|conn| MetadataRepository::exists_by_pkg_id(conn, &pkg.pkg_id))?;

            if !exists {
                let replacement = metadata_db.with_conn(|conn| {
                    MetadataRepository::find_replacement_pkg_id(conn, &pkg.pkg_id)
                })?;

                if let Some(new_pkg_id) = replacement {
                    info!(
                        "[{}] {} is replaced by {} in {}",
                        Colored(Blue, "Note"),
                        Colored(Red, &pkg.pkg_id),
                        Colored(Green, &new_pkg_id),
                        Colored(Magenta, &repo_name)
                    );

                    diesel_core_db.with_conn(|conn| {
                        CoreRepository::update_pkg_id(conn, &repo_name, &pkg.pkg_id, &new_pkg_id)
                    })?;
                }
            }
        }

        metadata_db
            .with_conn(|conn| MetadataRepository::update_repo_metadata(conn, &repo.name, etag))?;

        Ok(())
    }

    fn create_diesel_core_db(&self) -> SoarResult<DieselDatabase> {
        let core_db_file = self.config().get_db_path()?.join("soar.db");
        if !core_db_file.exists() {
            File::create(&core_db_file)
                .with_context(|| format!("creating database file {}", core_db_file.display()))?;
        }

        DieselDatabase::open_core(&core_db_file)
    }

    fn create_metadata_manager(&self) -> SoarResult<MetadataManager> {
        debug!("creating metadata manager");
        let mut manager = MetadataManager::new();

        for repo in &self.inner.config.repositories {
            if let Ok(repo_path) = repo.get_path() {
                let metadata_db = repo_path.join("metadata.db");
                if metadata_db.is_file() {
                    trace!(
                        repo_name = repo.name,
                        "adding repository to metadata manager"
                    );
                    manager.add_repo(&repo.name, metadata_db)?;
                }
            }
        }

        if let Ok(mut nests_db) = get_nests_db_conn() {
            if let Ok(nests) = NestRepository::list_all(nests_db.conn()) {
                if let Ok(nests_repo_path) = self.config().get_repositories_path() {
                    let nests_repo_path = nests_repo_path.join("nests");
                    for nest in nests {
                        let nest_path = nests_repo_path.join(&nest.name);
                        let metadata_db = nest_path.join("metadata.db");
                        if metadata_db.is_file() {
                            let nest_name = format!("nest-{}", nest.name);
                            trace!(nest_name = nest_name, "adding nest to metadata manager");
                            manager.add_repo(&nest_name, metadata_db)?;
                        }
                    }
                }
            }
        }

        debug!(repos = manager.repo_count(), "metadata manager created");
        Ok(manager)
    }

    #[inline]
    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    /// Reads the etag from an existing metadata database.
    fn read_repo_etag(&self, repo: &Repository) -> Option<String> {
        let repo_path = repo.get_path().ok()?;
        let metadata_db = repo_path.join("metadata.db");

        if !metadata_db.exists() {
            return None;
        }

        let mut conn = DbConnection::open(&metadata_db, DbType::Metadata).ok()?;
        MetadataRepository::get_repo_etag(conn.conn())
            .ok()
            .flatten()
    }

    /// Reads the etag from an existing nest metadata database.
    fn read_nest_etag(&self, nest_name: &str) -> Option<String> {
        let nests_repo_path = self.config().get_repositories_path().ok()?.join("nests");
        let nest_path = nests_repo_path.join(nest_name);
        let metadata_db = nest_path.join("metadata.db");

        if !metadata_db.exists() {
            return None;
        }

        let mut conn = DbConnection::open(&metadata_db, DbType::Metadata).ok()?;
        MetadataRepository::get_repo_etag(conn.conn())
            .ok()
            .flatten()
    }

    /// Returns the diesel-based core database connection.
    pub fn diesel_core_db(&self) -> SoarResult<&DieselDatabase> {
        self.inner
            .diesel_core_db
            .get_or_try_init(|| self.create_diesel_core_db())
    }

    /// Returns the metadata manager for querying package metadata across all repos.
    pub async fn metadata_manager(&self) -> SoarResult<&MetadataManager> {
        self.inner
            .metadata_manager
            .get_or_try_init(|| {
                async {
                    self.init_repo_dbs(false).await?;
                    self.sync_nests(false).await?;
                    self.create_metadata_manager()
                }
            })
            .await
    }
}
