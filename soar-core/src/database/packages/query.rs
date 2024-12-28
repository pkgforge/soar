use rusqlite::ToSql;

use super::models::{IterationState, PackageFilter, PackageSort};

pub struct QueryBuilder {
    shard_name: String,
    sort_method: PackageSort,
    filter: PackageFilter,
    state: IterationState,
    buffer_size: usize,
}

impl QueryBuilder {
    pub fn new(
        shard_name: String,
        sort_method: PackageSort,
        filter: PackageFilter,
        state: IterationState,
        buffer_size: usize,
    ) -> Self {
        Self {
            shard_name,
            sort_method,
            filter,
            state,
            buffer_size,
        }
    }

    pub fn build(&self) -> (String, Vec<Box<dyn ToSql>>) {
        let (where_clause, filter_params) = self.build_filter_clause();
        let query = self.build_query_string(&where_clause);

        let mut params: Vec<Box<dyn ToSql>> = filter_params
            .into_iter()
            .map(|s| Box::new(s) as Box<dyn ToSql>)
            .collect();

        self.sort_method
            .bind_pagination_params(&mut params, &self.state);
        params.push(Box::new(self.buffer_size));

        (query, params)
    }

    fn build_filter_clause(&self) -> (String, Vec<String>) {
        let mut conditions = Vec::new();
        let mut params = Vec::new();

        if let Some(collection) = &self.filter.collection {
            conditions.push("c.name = ?".to_string());
            params.push(collection.clone());
        }

        if let Some(pkg_name) = &self.filter.pkg_name {
            conditions.push("p.pkg_name LIKE ?".to_string());
            params.push(format!("%{}%", pkg_name));
        }

        if let Some(pkg_name) = &self.filter.exact_pkg_name {
            conditions.push("p.pkg_name = ?".to_string());
            params.push(pkg_name.clone());
        }

        if let Some(family) = &self.filter.family {
            conditions.push("f.name = ?".to_string());
            params.push(family.clone());
        }

        if let Some(search) = &self.filter.search_term {
            conditions.push("(p.pkg_name LIKE ? OR p.description LIKE ?)".to_string());
            params.push(format!("%{}%", search));
            params.push(format!("%{}%", search));
        }

        let where_clause = if conditions.is_empty() {
            "1=1".to_string()
        } else {
            conditions.join(" AND ")
        };

        (where_clause, params)
    }

    fn build_query_string(&self, where_clause: &str) -> String {
        format!(
            r#"
            SELECT 
                p.id, c.name AS collection, p.pkg, p.pkg_id, p.pkg_name,
                p.app_id, f.name AS family, p.description, p.version,
                p.size, p.checksum, n.note AS note, p.download_url,
                p.build_date, p.build_script, p.build_log, h.url AS homepage,
                p.category, su.url AS source_url, i.url AS icon, p.desktop
            FROM
                {0}.packages p
            LEFT JOIN {0}.collections c ON c.id = p.collection_id
            LEFT JOIN {0}.families f ON f.id = p.family_id
            LEFT JOIN {0}.icons i ON i.id = p.icon_id
            LEFT JOIN {0}.homepages h ON h.package_id = p.id
            LEFT JOIN {0}.notes n ON n.package_id = p.id
            LEFT JOIN {0}.source_urls su ON su.package_id = p.id
            WHERE {1} AND {2}
            ORDER BY {3}
            LIMIT ?
            {4}
            "#,
            self.shard_name,
            where_clause,
            self.sort_method.get_next_page_condition(),
            self.sort_method.get_order_clause(),
            if self.filter.exact_case {
                "COLLATE BINARY"
            } else {
                ""
            }
        )
    }
}

pub struct InstalledQueryBuilder {
    filter: PackageFilter,
    sort_method: PackageSort,
    state: IterationState,
    buffer_size: usize,
}

impl InstalledQueryBuilder {
    pub fn new(
        sort_method: PackageSort,
        filter: PackageFilter,
        state: IterationState,
        buffer_size: usize,
    ) -> Self {
        Self {
            sort_method,
            filter,
            state,
            buffer_size,
        }
    }

    pub fn build(&self) -> (String, Vec<Box<dyn ToSql>>) {
        let (where_clause, filter_params) = self.build_filter_clause();
        let query = self.build_query_string(&where_clause);

        let mut params: Vec<Box<dyn ToSql>> = filter_params
            .into_iter()
            .map(|s| Box::new(s) as Box<dyn ToSql>)
            .collect();

        self.sort_method
            .bind_pagination_params(&mut params, &self.state);

        params.push(Box::new(self.buffer_size));

        (query, params)
    }

    fn build_filter_clause(&self) -> (String, Vec<String>) {
        let mut conditions = Vec::new();
        let mut params = Vec::new();

        if let Some(collection) = &self.filter.collection {
            conditions.push("collection = ?".to_string());
            params.push(collection.clone());
        }

        if let Some(pkg_name) = &self.filter.pkg_name {
            conditions.push("pkg_name LIKE ?".to_string());
            params.push(format!("%{}%", pkg_name));
        }

        if let Some(pkg_name) = &self.filter.exact_pkg_name {
            conditions.push("pkg_name = ?".to_string());
            params.push(pkg_name.clone());
        }

        if let Some(family) = &self.filter.family {
            conditions.push("family = ?".to_string());
            params.push(family.clone());
        }

        if let Some(search) = &self.filter.search_term {
            conditions.push("(pkg_name LIKE ? OR description LIKE ?)".to_string());
            params.push(format!("%{}%", search));
            params.push(format!("%{}%", search));
        }

        let where_clause = if conditions.is_empty() {
            "1=1".to_string()
        } else {
            conditions.join(" AND ")
        };

        (where_clause, params)
    }

    fn build_query_string(&self, where_clause: &str) -> String {
        format!(
            r#"
            SELECT 
                id, repo_name, collection, family, pkg_name, pkg,
                pkg_id, app_id, description, version, size, checksum,
                build_date, build_script, build_log, category, bin_path,
                installed_path, installed_date, disabled, pinned,
                is_installed, installed_with_family
            FROM
                packages p
            WHERE {0} AND {1}
            LIMIT ?
            {2}
            "#,
            where_clause,
            self.sort_method.get_next_page_condition(),
            if self.filter.exact_case {
                "COLLATE BINARY"
            } else {
                ""
            }
        )
    }
}
