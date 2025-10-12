//! Internal representation of query clauses.
//!
//! These types are used internally by the [`super::Query`] builder and are not part of the public API.

use rusqlite::types::Value;

/// A WHERE clause represented as a closure that generates SQL and binds parameters.
pub(crate) struct WhereClause {
    pub sql_fn: Box<dyn Fn(&mut Vec<Value>) -> String>,
}

/// An ORDER BY clause.
pub(crate) struct OrderClause {
    pub column: String,
    pub desc: bool,
}
