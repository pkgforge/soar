//! Nest database repository for nest configuration management.

use diesel::prelude::*;

use crate::models::nest::{Nest, NewNest};
use crate::schema::nest::nests;

/// Repository for nest operations.
pub struct NestRepository;

impl NestRepository {
    /// Lists all nests.
    pub fn list_all(conn: &mut SqliteConnection) -> QueryResult<Vec<Nest>> {
        nests::table.select(Nest::as_select()).load(conn)
    }

    /// Finds a nest by ID.
    pub fn find_by_id(conn: &mut SqliteConnection, id: i32) -> QueryResult<Option<Nest>> {
        nests::table
            .filter(nests::id.eq(id))
            .select(Nest::as_select())
            .first(conn)
            .optional()
    }

    /// Finds a nest by name.
    pub fn find_by_name(conn: &mut SqliteConnection, name: &str) -> QueryResult<Option<Nest>> {
        nests::table
            .filter(nests::name.eq(name))
            .select(Nest::as_select())
            .first(conn)
            .optional()
    }

    /// Finds a nest by URL.
    pub fn find_by_url(conn: &mut SqliteConnection, url: &str) -> QueryResult<Option<Nest>> {
        nests::table
            .filter(nests::url.eq(url))
            .select(Nest::as_select())
            .first(conn)
            .optional()
    }

    /// Inserts a new nest.
    pub fn insert(conn: &mut SqliteConnection, nest: &NewNest) -> QueryResult<usize> {
        diesel::insert_into(nests::table)
            .values(nest)
            .execute(conn)
    }

    /// Deletes a nest by name.
    pub fn delete_by_name(conn: &mut SqliteConnection, name: &str) -> QueryResult<usize> {
        diesel::delete(nests::table.filter(nests::name.eq(name))).execute(conn)
    }

    /// Deletes a nest by ID.
    pub fn delete_by_id(conn: &mut SqliteConnection, id: i32) -> QueryResult<usize> {
        diesel::delete(nests::table.filter(nests::id.eq(id))).execute(conn)
    }

    /// Checks if a nest with the given name exists.
    pub fn exists_by_name(conn: &mut SqliteConnection, name: &str) -> QueryResult<bool> {
        use diesel::dsl::exists;
        diesel::select(exists(nests::table.filter(nests::name.eq(name)))).get_result(conn)
    }
}
