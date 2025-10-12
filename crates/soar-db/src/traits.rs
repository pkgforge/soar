//! Core traits that power the query builder.
//!
//! These traits define the contract for:
//! - Converting database rows into Rust types (`FromRow`)
//! - Building SQL expressions (`Expression`)

use rusqlite::{types::Value, Row};

use crate::expr::ops::{BinaryOp, InOp, LikeOp, LogicalOp, NullOp};

/// A trait for types that can be converted into SQL expressions.
///
/// This enables ergonomic query construction using operators like `.eq()`, `.like()`, etc.
/// Implementors include:
/// - [`super::expr::Col<T>`]: a table column
/// - [`BinaryOp`], [`LikeOp`], etc.: compound expressions
///
/// When `to_sql` is called, it appends bound parameters to the provided `params` vector
/// and returns the SQL fragment (with `?` placeholders).
pub trait Expression: Sized {
    /// Converts this expression into a SQL string fragment and appends bound parameters.
    ///
    /// # Parameters
    ///
    /// - `params`: A mutable vector to which bound values (e.g., strings, integers) are pushed.
    ///
    /// # Returns
    ///
    /// A SQL string fragment using `?` as placeholders for parameters.
    ///
    /// # Example
    ///
    /// ```rust
    /// use soar_db::expr::Col;
    /// use soar_db::traits::Expression as _;
    ///
    /// let col = Col::<String>::new("name");
    /// let expr = col.eq("User".to_string());
    /// let mut params = vec![];
    /// let sql = expr.to_sql(&mut params); // sql = "name = ?", params = [Value::Text("User".into())]
    /// ```
    fn to_sql(&self, params: &mut Vec<Value>) -> String;

    /// Creates a SQL `=` condition.
    fn eq<T: Into<Value>>(self, value: T) -> BinaryOp<Self> {
        BinaryOp::new(self, "=", value.into())
    }

    /// Creates a SQL `!=` condition.
    fn ne<T: Into<Value>>(self, value: T) -> BinaryOp<Self> {
        BinaryOp::new(self, "!=", value.into())
    }

    /// Creates a SQL `>` condition.
    fn gt<T: Into<Value>>(self, value: T) -> BinaryOp<Self> {
        BinaryOp::new(self, ">", value.into())
    }

    /// Creates a SQL `<` condition.
    fn lt<T: Into<Value>>(self, value: T) -> BinaryOp<Self> {
        BinaryOp::new(self, "<", value.into())
    }

    /// Creates a SQL `>=` condition.
    fn gte<T: Into<Value>>(self, value: T) -> BinaryOp<Self> {
        BinaryOp::new(self, ">=", value.into())
    }

    /// Creates a SQL `<=` condition.
    fn lte<T: Into<Value>>(self, value: T) -> BinaryOp<Self> {
        BinaryOp::new(self, "<=", value.into())
    }

    /// Creates a SQL `LIKE` condition.
    fn like(self, pattern: impl Into<String>) -> LikeOp<Self> {
        LikeOp::new(self, pattern.into(), false)
    }

    /// Creates a SQL `ILIKE` condition.
    fn ilike(self, pattern: impl Into<String>) -> LikeOp<Self> {
        LikeOp::new(self, pattern.into(), true)
    }

    /// Creates a SQL `IN` condition.
    fn in_<T, I>(self, values: I) -> InOp<Self>
    where
        T: Into<Value> + Clone,
        I: IntoIterator<Item = T>,
    {
        let values = values.into_iter().map(|v| v.into()).collect();
        InOp::new(self, values, false)
    }

    /// Creates a SQL `NOT IN` condition.
    fn not_in<T, I>(self, values: I) -> InOp<Self>
    where
        T: Into<Value> + Clone,
        I: IntoIterator<Item = T>,
    {
        let values = values.into_iter().map(|v| v.into()).collect();
        InOp::new(self, values, true)
    }

    /// Creates a SQL `IS NULL` condition.
    fn null(self) -> NullOp<Self> {
        NullOp::new(self, true)
    }

    /// Creates a SQL `IS NOT NULL` condition.
    fn not_null(self) -> NullOp<Self> {
        NullOp::new(self, false)
    }

    /// Combines two expressions with `AND`.
    fn and<E: Expression>(self, other: E) -> LogicalOp<Self, E> {
        LogicalOp::new(self, other, "AND")
    }

    /// Combines two expressions with `OR`.
    fn or<E: Expression>(self, other: E) -> LogicalOp<Self, E> {
        LogicalOp::new(self, other, "OR")
    }
}

/// A trait for types that can be constructed from a SQLite row.
///
/// This is used by [`super::Query::fetch`] and [`super::Query::fetch_one`] to map query results.
///
/// # Example
///
/// ```rust
/// use soar_db::FromRow;
/// struct User {
///     id: i64,
///     name: String
/// }
///
/// impl FromRow for User {
///     fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
///         Ok(User {
///             id: row.get("id")?,
///             name: row.get("name")?,
///         })
///     }
/// }
/// ```
pub trait FromRow: Sized {
    fn from_row(row: &Row) -> rusqlite::Result<Self>;
}
