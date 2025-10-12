//! Phantom types for query builder states.
//!
//! These zero-sized types enforce valid method chaining at compile time.

/// Initial state: no filters applied.
pub struct Unfiltered;

/// State after at least one `.filter()` call.
pub struct Filtered;

/// (Reserved for future use)
pub struct Ready;
