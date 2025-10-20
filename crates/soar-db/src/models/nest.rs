use diesel::prelude::*;

use crate::schema::nest::*;

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = nests)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Nest {
    pub id: i32,
    pub name: String,
    pub url: String,
}

#[derive(Insertable)]
#[diesel(table_name = nests)]
pub struct NewNest<'a> {
    pub name: &'a str,
    pub url: &'a str,
}
