use std::sync::{Arc, Mutex};

use rusqlite::{Connection, Row};

use crate::{
    database::models::{InstalledPackage, Package},
    error::SoarError,
    SoarResult,
};

use super::{
    models::{IterationState, PackageFilter, PackageSort},
    query::QueryBuilder,
    InstalledQueryBuilder,
};

#[derive(Debug)]
pub struct PackageIterator {
    db: Arc<Mutex<Connection>>,
    sort_method: PackageSort,
    filter: PackageFilter,
    state: IterationState,
    buffer: Vec<Package>,
    buffer_index: usize,
    buffer_size: usize,
    shard_index: usize,
    shard_count: usize,
    repo_name: Option<String>,
}

impl PackageIterator {
    pub fn new(
        db: Arc<Mutex<Connection>>,
        buffer_size: usize,
        sort_method: PackageSort,
        filter: PackageFilter,
    ) -> Self {
        Self {
            db,
            sort_method,
            filter,
            state: IterationState::default(),
            buffer: Vec::with_capacity(buffer_size),
            buffer_index: 0,
            buffer_size,
            shard_index: 0,
            shard_count: 0,
            repo_name: None,
        }
    }

    fn fetch_next_batch(&mut self) -> SoarResult<bool> {
        let db = self.db.clone();
        let conn = db.lock().map_err(|_| SoarError::PoisonError)?;

        if self.shard_count == 0 {
            self.initialize_shards(&conn)?;
        }

        if self.shard_index >= self.shard_count {
            return Ok(false);
        }

        let current_shard = self.get_current_shard(&conn)?;

        if self.should_skip_shard(&conn, &current_shard)? {
            self.shard_index += 1;
            return Ok(true);
        }

        self.fetch_packages(&conn, &current_shard)
    }

    fn initialize_shards(&mut self, conn: &Connection) -> SoarResult<()> {
        let mut stmt = conn.prepare("PRAGMA database_list")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        self.shard_count = rows.count();
        Ok(())
    }

    fn get_current_shard(&self, conn: &Connection) -> SoarResult<String> {
        let mut stmt = conn.prepare("PRAGMA database_list")?;
        let mut rows = stmt.query_map([], |row| row.get::<_, String>(1))?;

        let shard = rows
            .nth(self.shard_index)
            .ok_or(SoarError::DatabaseError(format!(
                "Shard index {} out of range",
                self.shard_index
            )))?;

        Ok(shard?)
    }

    fn should_skip_shard(&mut self, conn: &Connection, shard: &str) -> SoarResult<bool> {
        if self.repo_name.is_none() {
            let repo_name = self.get_repository_name(conn, shard)?;
            if let Some(ref filter_repo) = self.filter.repo_name {
                if filter_repo != &repo_name {
                    return Ok(true);
                }
            }
            self.repo_name = Some(repo_name);
        }
        Ok(false)
    }

    fn fetch_packages(&mut self, conn: &Connection, shard: &str) -> SoarResult<bool> {
        let query_builder = QueryBuilder::new(
            shard.to_string(),
            self.sort_method,
            self.filter.clone(),
            self.state.clone(),
            self.buffer_size,
        );

        let (query, params) = query_builder.build();
        let mut stmt = conn.prepare(&query)?;

        self.buffer.clear();
        self.buffer_index = 0;

        let params_ref: Vec<&dyn rusqlite::ToSql> = params
            .iter()
            .map(|p| p.as_ref() as &dyn rusqlite::ToSql)
            .collect();

        self.buffer = stmt
            .query_map(params_ref.as_slice(), |row| {
                map_package(row, self.repo_name.clone().unwrap())
            })?
            .filter_map(|res| match res {
                Ok(pkg) => Some(pkg),
                Err(err) => {
                    eprintln!("Map Error: {err:#?}");
                    None
                }
            })
            .collect();

        self.update_state();

        if self.buffer.is_empty() {
            self.advance_to_next_shard();
        }

        Ok(true)
    }

    fn update_state(&mut self) {
        if let Some(last_package) = self.buffer.last() {
            self.state = IterationState {
                id: last_package.id,
                pkg_name: Some(last_package.pkg.clone()),
                family: Some(last_package.pkg_id.clone()),
            };
        }
    }

    fn advance_to_next_shard(&mut self) {
        self.shard_index += 1;
        self.state = IterationState::default();
        self.repo_name = None;
    }

    fn get_repository_name(&self, conn: &Connection, shard_name: &str) -> SoarResult<String> {
        let query = format!("SELECT name FROM {0}.repository LIMIT 1", shard_name);
        let mut stmt = conn.prepare(&query)?;
        let repo_name: String = stmt.query_row([], |row| row.get(0))?;
        Ok(repo_name)
    }
}

impl Iterator for PackageIterator {
    type Item = SoarResult<Package>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buffer_index >= self.buffer.len() {
            match self.fetch_next_batch() {
                Ok(true) => {}
                Ok(false) => return None,
                Err(err) => return Some(Err(err)),
            }
        }

        if self.buffer_index < self.buffer.len() {
            let package = self.buffer[self.buffer_index].clone();
            self.buffer_index += 1;
            Some(Ok(package))
        } else if self.shard_index < self.shard_count {
            self.next()
        } else {
            None
        }
    }
}

pub fn get_all_packages(
    db: Arc<Mutex<Connection>>,
    buffer_size: usize,
) -> SoarResult<impl Iterator<Item = SoarResult<Package>>> {
    Ok(PackageIterator::new(
        db,
        buffer_size,
        PackageSort::Id,
        PackageFilter::default(),
    ))
}

pub fn get_packages_with_sort(
    db: Arc<Mutex<Connection>>,
    buffer_size: usize,
    sort_method: PackageSort,
) -> SoarResult<impl Iterator<Item = SoarResult<Package>>> {
    Ok(PackageIterator::new(
        db,
        buffer_size,
        sort_method,
        PackageFilter::default(),
    ))
}

pub fn get_packages_with_filter(
    db: Arc<Mutex<Connection>>,
    buffer_size: usize,
    filter: PackageFilter,
) -> SoarResult<impl Iterator<Item = SoarResult<Package>>> {
    Ok(PackageIterator::new(
        db,
        buffer_size,
        PackageSort::Id,
        filter,
    ))
}

pub fn get_packages_with_sort_and_filter(
    db: Arc<Mutex<Connection>>,
    buffer_size: usize,
    sort_method: PackageSort,
    filter: PackageFilter,
) -> SoarResult<impl Iterator<Item = SoarResult<Package>>> {
    Ok(PackageIterator::new(db, buffer_size, sort_method, filter))
}

#[derive(Debug)]
pub struct InstalledPackageIterator {
    db: Arc<Mutex<Connection>>,
    filter: PackageFilter,
    state: IterationState,
    buffer: Vec<InstalledPackage>,
    buffer_index: usize,
    buffer_size: usize,
}

impl InstalledPackageIterator {
    pub fn new(db: Arc<Mutex<Connection>>, buffer_size: usize, filter: PackageFilter) -> Self {
        Self {
            db,
            filter,
            state: IterationState::default(),
            buffer: Vec::with_capacity(buffer_size),
            buffer_index: 0,
            buffer_size,
        }
    }

    fn fetch_next_batch(&mut self) -> SoarResult<bool> {
        let db = self.db.clone();
        let conn = db.lock().map_err(|_| SoarError::PoisonError)?;

        self.fetch_packages(&conn)
    }

    fn fetch_packages(&mut self, conn: &Connection) -> SoarResult<bool> {
        let query_builder = InstalledQueryBuilder::new(
            PackageSort::Id,
            self.filter.clone(),
            self.state.clone(),
            self.buffer_size,
        );

        let (query, params) = query_builder.build();
        let mut stmt = conn.prepare(&query)?;

        self.buffer.clear();
        self.buffer_index = 0;

        let params_ref: Vec<&dyn rusqlite::ToSql> = params
            .iter()
            .map(|p| p.as_ref() as &dyn rusqlite::ToSql)
            .collect();

        self.buffer = stmt
            .query_map(params_ref.as_slice(), map_installed_package)?
            .filter_map(|res| match res {
                Ok(pkg) => Some(pkg),
                Err(err) => {
                    eprintln!("Map Error: {err:#?}");
                    None
                }
            })
            .collect();

        self.update_state();

        Ok(!self.buffer.is_empty())
    }

    fn update_state(&mut self) {
        if let Some(last_package) = self.buffer.last() {
            self.state = IterationState {
                id: last_package.id,
                pkg_name: Some(last_package.pkg.clone()),
                family: Some(last_package.pkg_id.clone()),
            };
        }
    }
}

impl Iterator for InstalledPackageIterator {
    type Item = SoarResult<InstalledPackage>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buffer_index >= self.buffer.len() {
            match self.fetch_next_batch() {
                Ok(true) => {}
                Ok(false) => return None,
                Err(err) => return Some(Err(err)),
            }
        }

        if self.buffer_index < self.buffer.len() {
            let package = self.buffer[self.buffer_index].clone();
            self.buffer_index += 1;
            Some(Ok(package))
        } else {
            None
        }
    }
}

pub fn get_installed_packages(
    db: Arc<Mutex<Connection>>,
    buffer_size: usize,
) -> SoarResult<impl Iterator<Item = SoarResult<InstalledPackage>>> {
    Ok(InstalledPackageIterator::new(
        db,
        buffer_size,
        PackageFilter::default(),
    ))
}

pub fn get_installed_packages_with_filter(
    db: Arc<Mutex<Connection>>,
    buffer_size: usize,
    filter: PackageFilter,
) -> SoarResult<impl Iterator<Item = SoarResult<InstalledPackage>>> {
    Ok(InstalledPackageIterator::new(db, buffer_size, filter))
}

fn map_package(row: &Row, repo_name: String) -> rusqlite::Result<Package> {
    let parse_json_vec = |idx: usize| -> rusqlite::Result<Option<Vec<String>>> {
        let value: String = row.get(idx)?;
        Ok(serde_json::from_str(&value).ok())
    };

    let homepages = parse_json_vec(16)?;
    let notes = parse_json_vec(17)?;
    let source_urls = parse_json_vec(18)?;
    let tags = parse_json_vec(19)?;
    let categories = parse_json_vec(20)?;

    Ok(Package {
        repo_name,
        id: row.get(0)?,
        disabled: row.get(1)?,
        disabled_reason: row.get(2)?,
        pkg: row.get(3)?,
        pkg_id: row.get(4)?,
        pkg_name: row.get(5)?,
        pkg_type: row.get(6)?,
        pkg_webpage: row.get(7)?,
        app_id: row.get(8)?,
        description: row.get(9)?,
        version: row.get(10)?,
        download_url: row.get(11)?,
        size: row.get(12)?,
        ghcr_pkg: row.get(13)?,
        ghcr_size: row.get(14)?,
        checksum: row.get(15)?,
        homepages,
        notes,
        source_urls,
        tags,
        categories,
        icon: row.get(21)?,
        desktop: row.get(22)?,
        build_id: row.get(23)?,
        build_date: row.get(24)?,
        build_script: row.get(25)?,
        build_log: row.get(26)?,
    })
}

pub fn map_installed_package(row: &Row) -> rusqlite::Result<InstalledPackage> {
    Ok(InstalledPackage {
        id: row.get(0)?,
        repo_name: row.get(1)?,
        pkg: row.get(2)?,
        pkg_id: row.get(3)?,
        pkg_name: row.get(4)?,
        version: row.get(5)?,
        size: row.get(6)?,
        checksum: row.get(7)?,
        installed_path: row.get(8)?,
        installed_date: row.get(9)?,
        bin_path: row.get(10)?,
        icon_path: row.get(11)?,
        desktop_path: row.get(12)?,
        appstream_path: row.get(13)?,
        pinned: row.get(14)?,
        is_installed: row.get(15)?,
        installed_with_family: row.get(16)?,
        profile: row.get(17)?,
    })
}
