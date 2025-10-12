//! The main query builder implementation.

use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use rusqlite::{types::Value, Connection, ToSql};

use crate::{
    expr::column::Col,
    query::{
        clause::{OrderClause, WhereClause},
        state::{Filtered, Unfiltered},
    },
    traits::{Expression, FromRow},
};

/// An ergonomic SQL query builder for SQLite.
///
/// Constructed via [`Query::from`], then chained with `.filter()`, `.order_by()`, etc.
///
/// # Type Parameters
///
/// - `E`: the entity type (must implement [`FromRow`])
/// - `State`: current builder state (`Unfiltered` or `Filtered`)
///
/// # Example
///
/// ```rust
/// use soar_db::{Query, FromRow, define_entity};
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
/// let users = Query::<User>::from(db, "users")
///     .filter(users::ID.gt(0))
///     .order_by(users::ID, false)
///     .limit(10)
///     .fetch()
///     .unwrap();
/// ```
pub struct Query<E, State = Unfiltered> {
    db: Arc<Mutex<Connection>>,
    table: &'static str,
    joins: Vec<String>,
    wheres: Vec<WhereClause>,
    orders: Vec<OrderClause>,
    limit: Option<u32>,
    offset: Option<u32>,
    pub(crate) _entity: PhantomData<E>,
    _state: PhantomData<State>,
}

impl<E> Query<E, Unfiltered> {
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
            joins: vec![],
            wheres: vec![],
            orders: vec![],
            limit: None,
            offset: None,
            _entity: PhantomData,
            _state: PhantomData,
        }
    }

    /// Adds a JOIN clause.
    ///
    /// # Example
    /// ```ignore
    /// .join("JOIN profiles ON users.id = profiles.user_id")
    /// ```
    pub fn join(mut self, join: impl Into<String>) -> Self {
        self.joins.push(join.into());
        self
    }

    /// Applies the first filter, transitioning to `Filtered` state.
    pub fn filter<Expr: Expression + 'static>(self, expr: Expr) -> Query<E, Filtered> {
        let mut query = Query {
            db: self.db,
            table: self.table,
            joins: self.joins,
            wheres: self.wheres,
            orders: self.orders,
            limit: self.limit,
            offset: self.offset,
            _entity: PhantomData,
            _state: PhantomData,
        };
        query.wheres.push(WhereClause {
            sql_fn: Box::new(move |params| expr.to_sql(params)),
        });
        query
    }
}

impl<E> Query<E, Filtered> {
    /// Adds an additional WHERE condition.
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

    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }

    pub fn page(mut self, page: u32, per_page: u32) -> Self {
        self.limit = Some(per_page);
        self.offset = Some((page - 1) * per_page);
        self
    }
}

impl<E: FromRow> Query<E, Filtered> {
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
        let mut sql = format!("SELECT * FROM {}", self.table);

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
