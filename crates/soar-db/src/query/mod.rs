//! The query builder.
//!
//! Start with [`Query::from`] and chain methods to construct SELECT queries.
//! The builder uses a state machine to ensure valid query construction:
//! - [`super::state::Unfiltered`]: initial state (no WHERE clause)
//! - [`super::state::Filtered`]: after `.filter()` is called
//!
//! Only `Filtered` queries can be executed (`fetch`, `count`, etc.).

pub mod builder;
pub mod clause;
pub mod state;

pub use builder::Query;
