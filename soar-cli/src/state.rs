use std::{
    fs::File,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use nu_ansi_term::Color::{Blue, Green, Magenta, Red};
use once_cell::sync::OnceCell;
use rusqlite::{params, Connection};
use soar_core::{
    config::{get_config, Config, Repository},
    constants::CORE_MIGRATIONS,
    database::{
        connection::Database,
        migration::MigrationManager,
        models::FromRow,
        packages::{FilterCondition, PackageQueryBuilder},
    },
    error::{ErrorContext, SoarError},
    metadata::fetch_metadata,
    SoarResult,
};
use tracing::{error, info};

use crate::utils::Colored;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    config: Config,
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
            tasks.push((task, repo));
        }

        for (task, repo) in tasks {
            match task
                .await
                .map_err(|err| SoarError::Custom(format!("Join handle error: {err}")))?
            {
                Ok(Some(etag)) => {
                    self.validate_packages(repo, &etag).await?;
                    info!("[{}] Repository synced", Colored(Magenta, &repo.name));
                }
                Err(err) => {
                    if !matches!(err, SoarError::FailedToFetchRemote(_)) {
                        return Err(err);
                    }
                    error!("{err}");
                }
                _ => {}
            };
        }

        Ok(())
    }

    async fn validate_packages(&self, repo: &Repository, etag: &str) -> SoarResult<()> {
        let core_db = self.core_db()?;
        let repo_name = repo.name.clone();

        let repo_path = repo.get_path()?;
        let metadata_db = repo_path.join("metadata.db");

        let repo_db = Arc::new(Mutex::new(Connection::open(&metadata_db)?));

        let installed_packages = PackageQueryBuilder::new(core_db.clone())
            .where_and("repo_name", FilterCondition::Eq(repo_name.to_string()))
            .load_installed()?;

        struct RepoPackage {
            pkg_id: String,
        }

        impl FromRow for RepoPackage {
            fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
                Ok(Self {
                    pkg_id: row.get("pkg_id")?,
                })
            }
        }

        for pkg in installed_packages.items {
            let repo_package: Vec<RepoPackage> = PackageQueryBuilder::new(repo_db.clone())
                .select(&["pkg_id"])
                .where_and("pkg_id", FilterCondition::Eq(pkg.pkg_id.clone()))
                .where_and("repo_name", FilterCondition::Eq(pkg.repo_name.clone()))
                .load()?
                .items;

            if repo_package.is_empty() {
                let replaced_by: Vec<RepoPackage> = PackageQueryBuilder::new(repo_db.clone())
                    .select(&["pkg_id"])
                    .where_and("repo_name", FilterCondition::Eq(pkg.repo_name))
                    // there's no easy way to do this, could create scalar SQL
                    // function, but this is enough for now
                    .where_and(
                        &format!("EXISTS (SELECT 1 FROM json_each(p.replaces) WHERE json_each.value = '{}')", pkg.pkg_id),
                        FilterCondition::None,
                    )
                    .limit(1)
                    .load()?
                    .items;

                if !replaced_by.is_empty() {
                    let new_pkg_id = &replaced_by.first().unwrap().pkg_id;
                    info!(
                        "[{}] {} is replaced by {} in {}",
                        Colored(Blue, "Note"),
                        Colored(Red, &pkg.pkg_id),
                        Colored(Green, new_pkg_id),
                        Colored(Magenta, &repo_name)
                    );

                    let conn = core_db.lock()?;
                    conn.execute(
                        "UPDATE packages SET pkg_id = ? WHERE pkg_id = ? AND repo_name = ?",
                        params![new_pkg_id, pkg.pkg_id, repo_name],
                    )?;
                }
            }
        }

        let conn = repo_db.lock()?;
        conn.execute(
            "UPDATE repository SET name = ?, etag = ?",
            params![repo.name, etag],
        )?;

        Ok(())
    }

    fn create_repo_db(&self) -> SoarResult<Database> {
        let repo_paths: Vec<PathBuf> = self
            .inner
            .config
            .repositories
            .iter()
            .filter_map(|r| {
                r.get_path()
                    .ok()
                    .map(|path| path.join("metadata.db"))
                    .filter(|db_path| db_path.is_file())
            })
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
