use std::sync::{Arc, Mutex};

use rusqlite::{Connection, ToSql};

use crate::{
    database::models::{FromRow, InstalledPackage},
    error::SoarError,
    SoarResult,
};

use super::{FilterCondition, LogicalOp, PaginatedResponse, QueryFilter, SortDirection};

#[derive(Debug, Clone)]
pub struct PackageQueryBuilder {
    db: Arc<Mutex<Connection>>,
    filters: Vec<QueryFilter>,
    sort_fields: Vec<(String, SortDirection)>,
    limit: Option<u32>,
    shards: Option<Vec<String>>,
    page: u32,
    select_columns: Vec<String>,
}

impl PackageQueryBuilder {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self {
            db,
            filters: Vec::new(),
            sort_fields: Vec::new(),
            limit: None,
            shards: None,
            page: 1,
            select_columns: Vec::new(),
        }
    }

    pub fn select(mut self, columns: &[&str]) -> Self {
        self.select_columns
            .extend(columns.iter().map(|&col| col.to_string()));
        self
    }

    pub fn clear_filters(mut self) -> Self {
        self.filters = Vec::new();
        self
    }

    pub fn where_and(mut self, field: &str, condition: FilterCondition) -> Self {
        self.filters.push(QueryFilter {
            field: field.to_string(),
            condition,
            logical_op: Some(LogicalOp::And),
        });
        self
    }

    pub fn where_or(mut self, field: &str, condition: FilterCondition) -> Self {
        self.filters.push(QueryFilter {
            field: field.to_string(),
            condition,
            logical_op: Some(LogicalOp::Or),
        });
        self
    }

    pub fn json_where_or(
        mut self,
        field: &str,
        json_field: &str,
        condition: FilterCondition,
    ) -> Self {
        let select_clause = format!("SELECT 1 FROM json_each({})", field);
        let extract_value = format!("json_extract(value, '$.{}')", json_field);
        let where_clause = self.build_subquery_where_clause(&extract_value, condition);

        let query = format!("EXISTS ({} WHERE {})", select_clause, where_clause);

        self.filters.push(QueryFilter {
            field: query,
            condition: FilterCondition::None,
            logical_op: Some(LogicalOp::Or),
        });
        self
    }

    pub fn json_where_and(
        mut self,
        field: &str,
        json_field: &str,
        condition: FilterCondition,
    ) -> Self {
        let select_clause = format!("SELECT 1 FROM json_each({})", field);
        let extract_value = format!("json_extract(value, '$.{}')", json_field);
        let where_clause = self.build_subquery_where_clause(&extract_value, condition);

        let query = format!("EXISTS ({} WHERE {})", select_clause, where_clause);

        self.filters.push(QueryFilter {
            field: query,
            condition: FilterCondition::None,
            logical_op: Some(LogicalOp::And),
        });
        self
    }

    pub fn database(mut self, db: Arc<Mutex<Connection>>) -> Self {
        self.db = db;
        self
    }

    pub fn sort_by(mut self, field: &str, direction: SortDirection) -> Self {
        self.sort_fields.push((field.to_string(), direction));
        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn clear_limit(mut self) -> Self {
        self.limit = None;
        self
    }

    pub fn page(mut self, page: u32) -> Self {
        self.page = page;
        self
    }

    pub fn shards(mut self, shards: Vec<String>) -> Self {
        self.shards = Some(shards);
        self
    }

    pub fn load<T: FromRow>(&self) -> SoarResult<PaginatedResponse<T>> {
        let conn = self.db.lock().map_err(|_| SoarError::PoisonError)?;
        let shards = self.get_shards(&conn)?;

        let (query, params) = self.build_query(&shards)?;
        let mut stmt = conn.prepare(&query)?;

        let params_ref: Vec<&dyn rusqlite::ToSql> = params
            .iter()
            .map(|p| p.as_ref() as &dyn rusqlite::ToSql)
            .collect();

        let items = stmt
            .query_map(params_ref.as_slice(), T::from_row)?
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

        let page = self.page;
        let limit = self.limit;

        let has_next = limit.map_or_else(|| false, |v| (self.page as u64 * v as u64) < total);

        Ok(PaginatedResponse {
            items,
            page,
            limit,
            total,
            has_next,
        })
    }

    fn get_shards(&self, conn: &Connection) -> SoarResult<Vec<String>> {
        let shards = self.shards.clone().unwrap_or_else(|| {
            let mut stmt = conn.prepare("PRAGMA database_list").unwrap();
            stmt.query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .filter_map(Result::ok)
                .collect()
        });
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
                let cols = if self.select_columns.is_empty() {
                    vec![
                        "p.id",
                        "disabled",
                        "json(disabled_reason) AS disabled_reason",
                        "rank",
                        "pkg",
                        "pkg_id",
                        "pkg_name",
                        "pkg_family",
                        "pkg_type",
                        "pkg_webpage",
                        "app_id",
                        "description",
                        "version",
                        "version_upstream",
                        "json(licenses) AS licenses",
                        "download_url",
                        "size",
                        "ghcr_pkg",
                        "ghcr_size",
                        "json(ghcr_files) AS ghcr_files",
                        "ghcr_blob",
                        "ghcr_url",
                        "bsum",
                        "shasum",
                        "icon",
                        "desktop",
                        "appstream",
                        "json(homepages) AS homepages",
                        "json(notes) AS notes",
                        "json(source_urls) AS source_urls",
                        "json(tags) AS tags",
                        "json(categories) AS categories",
                        "build_id",
                        "build_date",
                        "build_action",
                        "build_script",
                        "build_log",
                        "json(provides) AS provides",
                        "json(snapshots) AS snapshots",
                        "json(repology) AS repology",
                        "json(replaces) AS replaces",
                        "download_count",
                        "download_count_week",
                        "download_count_month",
                        "bundle",
                        "bundle_type",
                        "soar_syms",
                        "deprecated",
                        "desktop_integration",
                        "external",
                        "installable",
                        "portable",
                        "trusted",
                        "version_latest",
                        "version_outdated",
                    ]
                    .join(",")
                } else {
                    self.select_columns.join(",")
                };
                let select_clause = format!(
                    "SELECT
                        {cols}, r.name AS repo_name,
                        json_group_array(
                            json_object(
                                'name', m.name,
                                'contact', m.contact
                            )
                        ) FILTER (WHERE m.id IS NOT NULL) as maintainers
                     FROM
                         {0}.packages p
                         JOIN {0}.repository r
                         LEFT JOIN {0}.package_maintainers pm ON p.id = pm.package_id
                         LEFT JOIN {0}.maintainers m ON m.id = pm.maintainer_id
                    ",
                    shard
                );

                let where_clause = self.build_where_clause(&mut params);

                let mut query = format!("{} {}", select_clause, where_clause);
                query.push_str(" GROUP BY p.id, repo_name");
                query
            })
            .collect();

        let combined_query = shard_queries.join("\nUNION ALL\n");
        let mut final_query = format!("WITH results AS ({}) SELECT * FROM results", combined_query);

        if !self.sort_fields.is_empty() {
            let sort_clauses: Vec<String> = self
                .sort_fields
                .iter()
                .map(|(field, direction)| {
                    format!(
                        "{} {}",
                        field,
                        match direction {
                            SortDirection::Asc => "ASC",
                            SortDirection::Desc => "DESC",
                        }
                    )
                })
                .collect();
            final_query.push_str(" ORDER BY ");
            final_query.push_str(&sort_clauses.join(", "));
        }

        if let Some(limit) = self.limit {
            final_query.push_str(" LIMIT ?");
            params.push(Box::new(limit));

            let offset = self.limit.map(|limit| (self.page - 1) * limit);
            if let Some(offset) = offset {
                final_query.push_str(" OFFSET ?");
                params.push(Box::new(offset));
            }
        }

        Ok((final_query, params))
    }

    fn build_count_query(&self, shards: &[String]) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        let shard_queries: Vec<String> = shards
            .iter()
            .map(|shard| {
                let select_clause = format!(
                    "SELECT COUNT(1) as cnt, r.name as repo_name FROM {0}.packages p JOIN {0}.repository r",
                    shard
                );

                let where_clause = self.build_where_clause(&mut params);
                format!("{} {}", select_clause, where_clause)
            })
            .collect();

        let query = format!(
            "SELECT SUM(cnt) FROM ({})",
            shard_queries.join("\nUNION ALL\n")
        );

        (query, params)
    }

    pub fn load_installed(&self) -> SoarResult<PaginatedResponse<InstalledPackage>> {
        let conn = self.db.lock().map_err(|_| SoarError::PoisonError)?;
        let (query, params) = self.build_installed_query()?;
        let mut stmt = conn.prepare(&query)?;

        let params_ref: Vec<&dyn rusqlite::ToSql> = params
            .iter()
            .map(|p| p.as_ref() as &dyn rusqlite::ToSql)
            .collect();
        let items = stmt
            .query_map(params_ref.as_slice(), InstalledPackage::from_row)?
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
            let select_clause = "SELECT COUNT(1) FROM packages p";
            let where_clause = self.build_where_clause(&mut params);
            let query = format!("{} {}", select_clause, where_clause);
            (query, params)
        };
        let mut count_stmt = conn.prepare(&count_query)?;
        let count_params_ref: Vec<&dyn rusqlite::ToSql> = count_params
            .iter()
            .map(|p| p.as_ref() as &dyn rusqlite::ToSql)
            .collect();
        let total: u64 = count_stmt.query_row(count_params_ref.as_slice(), |row| row.get(0))?;

        let page = self.page;
        let limit = self.limit;

        let has_next = limit.map_or_else(|| false, |v| (self.page as u64 * v as u64) < total);

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
        let select_clause = "SELECT p.*, pp.* FROM packages p
            LEFT JOIN portable_package pp
            ON pp.package_id = p.id";
        let where_clause = self.build_where_clause(&mut params);
        let mut query = format!("{} {}", select_clause, where_clause);

        if !self.sort_fields.is_empty() {
            let sort_clauses: Vec<String> = self
                .sort_fields
                .iter()
                .map(|(field, direction)| {
                    format!(
                        "{} {}",
                        field,
                        match direction {
                            SortDirection::Asc => "ASC",
                            SortDirection::Desc => "DESC",
                        }
                    )
                })
                .collect();
            query.push_str(" ORDER BY ");
            query.push_str(&sort_clauses.join(", "));
        }

        if let Some(limit) = self.limit {
            query.push_str(" LIMIT ?");
            params.push(Box::new(limit));

            let offset = self.limit.map(|limit| (self.page - 1) * limit);
            if let Some(offset) = offset {
                query.push_str(" OFFSET ?");
                params.push(Box::new(offset));
            }
        }

        Ok((query, params))
    }

    fn build_where_clause(&self, params: &mut Vec<Box<dyn ToSql>>) -> String {
        if self.filters.is_empty() {
            return String::new();
        }

        let conditions: Vec<String> = self
            .filters
            .iter()
            .enumerate()
            .map(|(idx, filter)| {
                let condition = match &filter.condition {
                    FilterCondition::Eq(val) => {
                        params.push(Box::new(val.clone()));
                        format!("{} = ?", filter.field)
                    }
                    FilterCondition::Ne(val) => {
                        params.push(Box::new(val.clone()));
                        format!("{} != ?", filter.field)
                    }
                    FilterCondition::Gt(val) => {
                        params.push(Box::new(val.clone()));
                        format!("{} > ?", filter.field)
                    }
                    FilterCondition::Gte(val) => {
                        params.push(Box::new(val.clone()));
                        format!("{} >= ?", filter.field)
                    }
                    FilterCondition::Lt(val) => {
                        params.push(Box::new(val.clone()));
                        format!("{} < ?", filter.field)
                    }
                    FilterCondition::Lte(val) => {
                        params.push(Box::new(val.clone()));
                        format!("{} <= ?", filter.field)
                    }
                    FilterCondition::Like(val) => {
                        params.push(Box::new(format!("%{}%", val)));
                        format!("{} LIKE ?", filter.field)
                    }
                    FilterCondition::ILike(val) => {
                        params.push(Box::new(format!("%{}%", val)));
                        format!("LOWER({}) LIKE LOWER(?)", filter.field)
                    }
                    FilterCondition::In(vals) => {
                        let placeholders = vec!["?"; vals.len()].join(", ");
                        for val in vals {
                            params.push(Box::new(val.clone()));
                        }
                        format!("{} IN ({})", filter.field, placeholders)
                    }
                    FilterCondition::NotIn(vals) => {
                        let placeholders = vec!["?"; vals.len()].join(", ");
                        for val in vals {
                            params.push(Box::new(val.clone()));
                        }
                        format!("{} NOT IN ({})", filter.field, placeholders)
                    }
                    FilterCondition::Between(start, end) => {
                        params.push(Box::new(start.clone()));
                        params.push(Box::new(end.clone()));
                        format!("{} BETWEEN ? AND ?", filter.field)
                    }
                    FilterCondition::IsNull => {
                        format!("{} IS NULL", filter.field)
                    }
                    FilterCondition::IsNotNull => {
                        format!("{} IS NOT NULL", filter.field)
                    }
                    FilterCondition::None => filter.field.to_string(),
                };

                if idx > 0 {
                    match filter.logical_op {
                        Some(LogicalOp::And) => format!("AND {}", condition),
                        Some(LogicalOp::Or) => format!("OR {}", condition),
                        None => condition,
                    }
                } else {
                    condition
                }
            })
            .collect();
        format!("WHERE {}", conditions.join(" "))
    }

    fn build_subquery_where_clause(&self, value: &str, condition: FilterCondition) -> String {
        match condition {
            FilterCondition::Eq(val) => {
                format!("{} = '{}'", value, val)
            }
            FilterCondition::Ne(val) => {
                format!("{} != '{}'", value, val)
            }
            FilterCondition::Gt(val) => {
                format!("{} > '{}'", value, val)
            }
            FilterCondition::Gte(val) => {
                format!("{} >= '{}'", value, val)
            }
            FilterCondition::Lt(val) => {
                format!("{} < '{}'", value, val)
            }
            FilterCondition::Lte(val) => {
                format!("{} <= '{}'", value, val)
            }
            FilterCondition::Like(val) => {
                format!("{} LIKE '%{}%'", value, val)
            }
            FilterCondition::ILike(val) => {
                format!("LOWER({}) LIKE LOWER('%{}%')", value, val)
            }
            FilterCondition::In(vals) => {
                format!(
                    "{} IN ({})",
                    value,
                    vals.iter()
                        .map(|v| format!("'{}'", v))
                        .collect::<Vec<String>>()
                        .join(",")
                )
            }
            FilterCondition::NotIn(vals) => {
                format!(
                    "{} NOT IN ({})",
                    value,
                    vals.iter()
                        .map(|v| format!("'{}'", v))
                        .collect::<Vec<String>>()
                        .join(",")
                )
            }
            FilterCondition::Between(start, end) => {
                format!("{} BETWEEN '{}' AND '{}'", value, start, end)
            }
            FilterCondition::IsNull => {
                format!("{} IS NULL", value)
            }
            FilterCondition::IsNotNull => {
                format!("{} IS NOT NULL", value)
            }
            FilterCondition::None => String::new(),
        }
    }
}
