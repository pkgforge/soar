use std::sync::{Arc, Mutex};

use rusqlite::{Connection, Row, ToSql};

use crate::{
    database::{
        models::{InstalledPackage, Package},
        packages::SortOrder,
    },
    error::SoarError,
    SoarResult,
};

use super::{Filter, FilterOp, FilterValue, PackageProvide, PaginatedResponse, QueryOptions};

pub struct PackageQuery {
    db: Arc<Mutex<Connection>>,
    options: QueryOptions,
}

impl PackageQuery {
    pub fn new(db: Arc<Mutex<Connection>>, options: QueryOptions) -> Self {
        Self { db, options }
    }

    pub fn execute(&self) -> SoarResult<PaginatedResponse<Package>> {
        let conn = self.db.lock().map_err(|_| SoarError::PoisonError)?;
        let shards = self.get_shards(&conn)?;
        let (query, params) = self.build_query(&shards)?;
        let mut stmt = conn.prepare(&query)?;

        let params_ref: Vec<&dyn rusqlite::ToSql> = params
            .iter()
            .map(|p| p.as_ref() as &dyn rusqlite::ToSql)
            .collect();
        let items = stmt
            .query_map(params_ref.as_slice(), map_package)?
            .filter_map(|r| match r {
                Ok(pkg) => Some(pkg),
                Err(err) => {
                    eprintln!("Package map error: {err:#?}");
                    None
                }
            })
            .collect();

        let (count_query, count_params) = self.build_count_query(&shards);
        let mut count_stmt = conn.prepare(&count_query)?;
        let count_params_ref: Vec<&dyn rusqlite::ToSql> = count_params
            .iter()
            .map(|p| p.as_ref() as &dyn rusqlite::ToSql)
            .collect();
        let total: u64 = count_stmt.query_row(count_params_ref.as_slice(), |row| row.get(0))?;

        let page = self.options.page;
        let limit = self.options.limit;
        let has_next = (page as u64 * limit as u64) < total;

        Ok(PaginatedResponse {
            items,
            page,
            limit,
            total,
            has_next,
        })
    }

    fn get_shards(&self, conn: &Connection) -> SoarResult<Vec<String>> {
        let mut stmt = conn.prepare("PRAGMA database_list")?;
        let shards = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(Result::ok)
            .collect();
        Ok(shards)
    }

    fn build_query(
        &self,
        shards: &[String],
    ) -> SoarResult<(String, Vec<Box<dyn rusqlite::ToSql>>)> {
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        let shard_queries: Vec<String> = shards
            .iter()
            .map(|shard| {
                let select_clause = format!(
                    "SELECT p.*, r.name FROM {0}.packages p JOIN {0}.repository r",
                    shard
                );
                self.build_shard_query(&select_clause, &mut params)
            })
            .collect();

        let combined_query = shard_queries.join("\nUNION ALL\n");

        let mut final_query = format!("WITH results AS ({}) SELECT * FROM results", combined_query);

        if !self.options.sort_by.is_empty() {
            let sort_clauses: Vec<String> = self
                .options
                .sort_by
                .iter()
                .map(|(field, order)| {
                    format!(
                        "{} {}",
                        field,
                        match order {
                            SortOrder::Asc => "ASC",
                            SortOrder::Desc => "DESC",
                        }
                    )
                })
                .collect();
            final_query.push_str(" ORDER BY ");
            final_query.push_str(&sort_clauses.join(", "));
        }

        let page = self.options.page;
        let limit = self.options.limit;
        let offset = (page - 1) * limit;
        final_query.push_str(" LIMIT ? OFFSET ?");
        params.push(Box::new(self.options.limit));
        params.push(Box::new(offset));

        Ok((final_query, params))
    }

    fn build_shard_query(
        &self,
        select_clause: &str,
        params: &mut Vec<Box<dyn rusqlite::ToSql>>,
    ) -> String {
        let mut conditions = Vec::new();

        for (field, filter) in &self.options.filters {
            if let Some(condition) = self.build_filter_condition(field, filter, params) {
                conditions.push(condition);
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        format!("{} {}", select_clause, where_clause)
    }

    fn build_count_query(&self, shards: &[String]) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        let shard_queries: Vec<String> = shards
            .iter()
            .map(|shard| {
                let select_clause = format!(
                    "SELECT COUNT(*) as cnt FROM {0}.packages p JOIN {0}.repository r",
                    shard
                );
                self.build_shard_query(&select_clause, &mut params)
            })
            .collect();

        let query = format!(
            "SELECT SUM(cnt) FROM ({})",
            shard_queries.join("\nUNION ALL\n")
        );

        (query, params)
    }

    fn build_filter_condition(
        &self,
        field: &str,
        filter: &Filter,
        params: &mut Vec<Box<dyn ToSql>>,
    ) -> Option<String> {
        match (&filter.operator, &filter.value) {
            (FilterOp::IsNull, _) => Some(format!("{} IS NULL", field)),
            (FilterOp::IsNotNull, _) => Some(format!("{} IS NOT NULL", field)),
            (FilterOp::Between, FilterValue::Range(start, end)) => {
                params.push(Box::new(start.clone()));
                params.push(Box::new(end.clone()));
                Some(format!("{} BETWEEN ? AND ?", field))
            }
            (FilterOp::In | FilterOp::NotIn, FilterValue::Multiple(values)) => {
                let placeholders = vec!["?"; values.len()].join(",");
                for value in values {
                    params.push(Box::new(value.clone()));
                }
                Some(format!(
                    "{} {} ({})",
                    field,
                    filter.operator.to_sql(),
                    placeholders
                ))
            }
            (FilterOp::Like | FilterOp::ILike, FilterValue::Single(value)) => {
                let wildcard_value = format!("%{}%", value);
                params.push(Box::new(wildcard_value));
                if matches!(filter.operator, FilterOp::ILike) {
                    Some(format!("LOWER({}) LIKE LOWER(?)", field))
                } else {
                    Some(format!("{} LIKE ?", field))
                }
            }
            (_, FilterValue::Single(value)) => {
                params.push(Box::new(value.clone()));
                Some(format!("{} {} ?", field, filter.operator.to_sql()))
            }
            _ => None,
        }
    }

    pub fn execute_installed(&self) -> SoarResult<PaginatedResponse<InstalledPackage>> {
        let conn = self.db.lock().map_err(|_| SoarError::PoisonError)?;
        let (query, params) = self.build_installed_query()?;
        let mut stmt = conn.prepare(&query)?;

        let params_ref: Vec<&dyn rusqlite::ToSql> = params
            .iter()
            .map(|p| p.as_ref() as &dyn rusqlite::ToSql)
            .collect();
        let items = stmt
            .query_map(params_ref.as_slice(), map_installed_package)?
            .filter_map(|r| match r {
                Ok(pkg) => Some(pkg),
                Err(err) => {
                    eprintln!("Installed package map error: {err:#?}");
                    None
                }
            })
            .collect();

        let (count_query, count_params) = {
            let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
            let select_clause = "SELECT COUNT(*) FROM packages p";
            let query = self.build_shard_query(&select_clause, &mut params);
            (query, params)
        };
        let mut count_stmt = conn.prepare(&count_query)?;
        let count_params_ref: Vec<&dyn rusqlite::ToSql> = count_params
            .iter()
            .map(|p| p.as_ref() as &dyn rusqlite::ToSql)
            .collect();
        let total: u64 = count_stmt.query_row(count_params_ref.as_slice(), |row| row.get(0))?;

        let page = self.options.page;
        let limit = self.options.limit;
        let has_next = (page as u64 * limit as u64) < total;

        Ok(PaginatedResponse {
            items,
            page,
            limit,
            total,
            has_next,
        })
    }

    fn build_installed_query(&self) -> SoarResult<(String, Vec<Box<dyn rusqlite::ToSql>>)> {
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let select_clause = "SELECT p.* FROM packages p";
        let mut query = self.build_shard_query(select_clause, &mut params);

        if !self.options.sort_by.is_empty() {
            let sort_clauses: Vec<String> = self
                .options
                .sort_by
                .iter()
                .map(|(field, order)| {
                    format!(
                        "{} {}",
                        field,
                        match order {
                            SortOrder::Asc => "ASC",
                            SortOrder::Desc => "DESC",
                        }
                    )
                })
                .collect();
            query.push_str(" ORDER BY ");
            query.push_str(&sort_clauses.join(", "));
        }

        let offset = (self.options.page - 1) * self.options.limit;
        query.push_str(" LIMIT ? OFFSET ?");
        params.push(Box::new(self.options.limit));
        params.push(Box::new(offset));

        Ok((query, params))
    }
}

fn map_package(row: &Row) -> rusqlite::Result<Package> {
    let parse_json_vec = |idx: usize| -> rusqlite::Result<Option<Vec<String>>> {
        let value: String = row.get(idx)?;
        Ok(serde_json::from_str(&value).ok())
    };

    let parse_provides = |idx: usize| -> rusqlite::Result<Option<Vec<PackageProvide>>> {
        let value: String = row.get(idx)?;
        Ok(serde_json::from_str(&value).ok())
    };

    let homepages = parse_json_vec(18)?;
    let notes = parse_json_vec(19)?;
    let source_urls = parse_json_vec(20)?;
    let tags = parse_json_vec(21)?;
    let categories = parse_json_vec(22)?;
    let provides = parse_provides(27)?;

    Ok(Package {
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
        icon: row.get(16)?,
        desktop: row.get(17)?,
        homepages,
        notes,
        source_urls,
        tags,
        categories,
        build_id: row.get(23)?,
        build_date: row.get(24)?,
        build_script: row.get(25)?,
        build_log: row.get(26)?,
        provides,
        repo_name: row.get(28)?,
    })
}

pub struct PaginatedIterator<'a, T, F>
where
    F: Fn(QueryOptions) -> SoarResult<PaginatedResponse<T>>,
{
    query_options: QueryOptions,
    fetch_page: &'a F,
    current_page: u32,
    limit: u32,
    has_next: bool,
}

impl<'a, T, F> PaginatedIterator<'a, T, F>
where
    F: Fn(QueryOptions) -> SoarResult<PaginatedResponse<T>>,
{
    pub fn new(fetch_page: &'a F, query_options: QueryOptions) -> Self {
        let limit = query_options.limit;
        PaginatedIterator {
            query_options,
            fetch_page,
            current_page: 1,
            limit,
            has_next: true,
        }
    }
}

impl<'a, T, F> Iterator for PaginatedIterator<'a, T, F>
where
    T: Clone,
    F: Fn(QueryOptions) -> SoarResult<PaginatedResponse<T>>,
{
    type Item = SoarResult<Vec<T>>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.has_next {
            return None;
        }

        self.query_options.page = self.current_page;
        self.query_options.limit = self.limit;

        match (self.fetch_page)(self.query_options.clone()) {
            Ok(response) => {
                self.has_next = response.has_next;
                self.current_page += 1;
                Some(Ok(response.items))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

pub fn get_packages(
    db: Arc<Mutex<Connection>>,
    options: QueryOptions,
) -> SoarResult<PaginatedResponse<Package>> {
    PackageQuery::new(db, options).execute()
}

pub fn get_installed_packages(
    db: Arc<Mutex<Connection>>,
    options: QueryOptions,
) -> SoarResult<PaginatedResponse<InstalledPackage>> {
    PackageQuery::new(db, options).execute_installed()
}

pub fn map_installed_package(row: &Row) -> rusqlite::Result<InstalledPackage> {
    let parse_provides = |idx: usize| -> rusqlite::Result<Option<Vec<PackageProvide>>> {
        let value: String = row.get(idx)?;
        Ok(serde_json::from_str(&value).ok())
    };
    let provides = parse_provides(20)?;

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
        profile: row.get(14)?,
        pinned: row.get(15)?,
        is_installed: row.get(16)?,
        with_pkg_id: row.get(17)?,
        detached: row.get(18)?,
        unlinked: row.get(19)?,
        provides,
    })
}
