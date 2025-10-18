//! Represents a typed database column.
//!
//! `Col<T>` ties a column name to a Rust type `T`, enabling compile-time
//! safety when constructing queries. It implements [`Expression`], so it can
//! be used directly in filters.

use std::marker::PhantomData;

use rusqlite::types::Value;

use crate::traits::Expression;

/// A typed reference to a database column.
///
/// The type parameter `T` indicates the expected Rust type when reading this column,
/// though it is not enforced at runtime—ensure your `FromRow` implementation matches.
///
/// # Example
///
/// ```rust
/// use soar_db::expr::Col;
/// const NAME: Col<String> = Col::new("name");
/// ```
#[derive(Clone, Copy)]
pub struct Col<T> {
    pub name: &'static str,
    pub is_json: bool,
    _type: PhantomData<T>,
}

impl<T> Col<T> {
    /// Creates a new column reference.
    ///
    /// # Parameters
    ///
    /// - `name`: the actual column name in the database (e.g., `"user_name"`)
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            is_json: false,
            _type: PhantomData,
        }
    }

    /// Mark this column as JSON for proper SELECT handling
    pub const fn json(name: &'static str) -> Self {
        Self {
            name,
            is_json: true,
            _type: PhantomData,
        }
    }

    /// Get the select expression for this column
    pub fn select_expr(&self) -> String {
        if self.is_json {
            format!("json({}) AS {}", self.name, self.name)
        } else {
            self.name.to_string()
        }
    }
}

impl<T> Expression for Col<T> {
    fn to_sql(&self, _params: &mut Vec<Value>) -> String {
        self.name.to_string()
    }
}
