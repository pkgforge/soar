//! The main query builder implementation.

use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use rusqlite::{types::Value, Connection, ToSql};

use crate::{
    expr::column::Col,
    query::clause::{OrderClause, WhereClause},
    traits::{Expression, FromRow},
};

/// An ergonomic SQL query builder for SQLite.
///
/// Constructed via [`SelectQuery::from`], then chained with `.filter()`, `.order_by()`, etc.
///
/// # Type Parameters
///
/// - `E`: the entity type (must implement [`FromRow`])
/// - `State`: current builder state (`Unfiltered` or `Filtered`)
///
/// # Example
///
/// ```rust
/// use soar_db::{SelectQuery, FromRow, define_entity};
/// use soar_db::traits::Expression as _;
/// use std::sync::{Arc, Mutex};
/// use rusqlite::Connection;
///
/// #[derive(Debug)]
/// struct User {
///     id: i64
/// }
///
/// impl FromRow for User {
///     fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
///         Ok(User {
///             id: row.get("id")?
///         })
///     }
/// }
///
/// define_entity!(
///     users {
///         table: "users",
///         columns: {
///             ID: i64 => "id"
///         }
///     }
/// );
///
/// let conn = Connection::open_in_memory().unwrap();
/// conn.execute(
///     "CREATE TABLE users (
///         id INTEGER PRIMARY KEY
///     )",
///     [],
/// ).unwrap();
///
///
/// let db = Arc::new(Mutex::new(conn));
/// let users = SelectQuery::<User>::from(db, "users")
///     .filter(users::ID.gt(0))
///     .order_by(users::ID, false)
///     .limit(10)
///     .fetch()
///     .unwrap();
/// ```
pub struct SelectQuery<E> {
    db: Arc<Mutex<Connection>>,
    table: &'static str,
    columns: Vec<String>,
    joins: Vec<String>,
    wheres: Vec<WhereClause>,
    orders: Vec<OrderClause>,
    limit: Option<u32>,
    offset: Option<u32>,
    _entity: PhantomData<E>,
}

impl<E> SelectQuery<E> {
    /// Starts a new query on the given table.
    ///
    /// # Parameters
    ///
    /// - `db`: shared database connection
    /// - `table`: table name (e.g., `"users"`)
    pub fn from(db: Arc<Mutex<Connection>>, table: &'static str) -> Self {
        Self {
            db,
            table,
            columns: vec![],
            joins: vec![],
            wheres: vec![],
            orders: vec![],
            limit: None,
            offset: None,
            _entity: PhantomData,
        }
    }

    /// Select specific columns from the table.
    pub fn select<T>(mut self, cols: &[Col<T>]) -> Self {
        self.columns.extend(cols.iter().map(|c| c.select_expr()));
        self
    }

    /// Select all columns from the table
    pub fn select_all(mut self) -> Self {
        self.columns.clear();
        self
    }

    /// Adds a JOIN clause.
    pub fn join(mut self, join: impl Into<String>) -> Self {
        self.joins.push(join.into());
        self
    }

    /// Applies the WHERE condition.
    pub fn filter<Expr: Expression + 'static>(mut self, expr: Expr) -> Self {
        self.wheres.push(WhereClause {
            sql_fn: Box::new(move |params| expr.to_sql(params)),
        });
        self
    }

    /// Adds an ORDER BY clause.
    pub fn order_by<T>(mut self, col: Col<T>, desc: bool) -> Self {
        self.orders.push(OrderClause {
            column: col.name.to_string(),
            desc,
        });
        self
    }

    /// Limit the number of results
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set query offset
    pub fn offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Set pagination params
    pub fn page(mut self, page: u32, per_page: u32) -> Self {
        self.limit = Some(per_page);
        self.offset = Some((page - 1) * per_page);
        self
    }
}

impl<E: FromRow> SelectQuery<E> {
    pub fn fetch(self) -> rusqlite::Result<Vec<E>> {
        let (sql, params) = self.build_sql();
        let conn = self.db.lock().unwrap();
        let mut stmt = conn.prepare(&sql)?;

        let params_ref: Vec<&dyn ToSql> = params.iter().map(|v| v as &dyn ToSql).collect();
        let rows = stmt.query_map(params_ref.as_slice(), E::from_row)?;
        rows.collect()
    }

    pub fn fetch_one(self) -> rusqlite::Result<Option<E>> {
        let mut results = self.limit(1).fetch()?;
        Ok(results.pop())
    }

    pub fn count(self) -> rusqlite::Result<u64> {
        let (sql, params) = self.build_count_sql();
        let conn = self.db.lock().unwrap();
        let mut stmt = conn.prepare(&sql)?;

        let params_ref: Vec<&dyn ToSql> = params.iter().map(|v| v as &dyn ToSql).collect();
        stmt.query_row(params_ref.as_slice(), |row| row.get(0))
    }

    fn build_sql(&self) -> (String, Vec<Value>) {
        let mut params = vec![];

        let select = if self.columns.is_empty() {
            "*".to_string()
        } else {
            self.columns.join(", ")
        };

        let mut sql = format!("SELECT {} FROM {}", select, self.table);

        for join in &self.joins {
            sql.push_str(&format!(" {}", join));
        }

        if !self.wheres.is_empty() {
            sql.push_str(" WHERE ");
            let conditions = self
                .wheres
                .iter()
                .map(|w| (w.sql_fn)(&mut params))
                .collect::<Vec<_>>();
            sql.push_str(&conditions.join(" AND "));
        }

        if !self.orders.is_empty() {
            sql.push_str(" ORDER BY ");
            let orders = self
                .orders
                .iter()
                .map(|o| format!("{} {}", o.column, if o.desc { "DESC" } else { "ASC" }))
                .collect::<Vec<_>>();
            sql.push_str(&orders.join(", "));
        }

        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = self.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        (sql, params)
    }

    fn build_count_sql(&self) -> (String, Vec<Value>) {
        let mut params = vec![];
        let mut sql = format!("SELECT COUNT(*) FROM {}", self.table);

        for join in &self.joins {
            sql.push_str(&format!(" {}", join));
        }

        if !self.wheres.is_empty() {
            sql.push_str(" WHERE ");
            let conditions = self
                .wheres
                .iter()
                .map(|w| (w.sql_fn)(&mut params))
                .collect::<Vec<_>>();
            sql.push_str(&conditions.join(" AND "));
        }

        (sql, params)
    }
}
