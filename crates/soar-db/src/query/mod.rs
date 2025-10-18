//! The query builder.
//!
//! Start with [`SelectQuery::from`] and chain methods to construct SELECT queries.

//! The query builder.
//!
//! This module provides a strongly-typed interface for constructing SQL queries
//! without manually concatenating strings. Each query type (SELECT, INSERT, UPDATE, DELETE)
//! has its own builder with chainable methods for composing clauses safely and ergonomically.
//!
//! # Overview
//!
//! The query builder is organized into four main types:
//!
//! - [`SelectQuery`] — Builds `SELECT` statements with support for columns, filters,
//!   ordering, limits, joins, and more.
//! - [`InsertQuery`] — Builds `INSERT INTO` statements with column-value pairs or
//!   batch inserts.
//! - [`UpdateQuery`] — Builds `UPDATE` statements with `SET` and `WHERE` clauses.
//! - [`DeleteQuery`] — Builds `DELETE FROM` statements with filtering conditions.
//!
//! Each builder supports method chaining and can produce a final SQL string and bound
//! parameter list for execution with an underlying database library (`rusqlite`).
//!
//! # Example
//!
//! ```ignore
//! use crate::query::SelectQuery;
//!
//! let (sql, params) = SelectQuery::from("users")
//!     .columns(["id", "username", "email"])
//!     .filter("active = 1")
//!     .order_by("created_at DESC")
//!     .limit(10)
//!     .build();
//! ```
//!
//! # Submodules
//!
//! - [`clause`] — Common clause helpers shared between different query types.
//! - [`select`] — Implementation of [`SelectQuery`].
//! - [`insert`] — Implementation of [`InsertQuery`].
//! - [`update`] — Implementation of [`UpdateQuery`].
//! - [`delete`] — Implementation of [`DeleteQuery`].
//!
//! # See also
//!
//! Each query type’s documentation for detailed usage examples and supported clauses.

pub mod clause;
pub mod delete;
pub mod insert;
pub mod select;
pub mod update;

pub use delete::DeleteQuery;
pub use insert::InsertQuery;
pub use select::SelectQuery;
pub use update::UpdateQuery;
