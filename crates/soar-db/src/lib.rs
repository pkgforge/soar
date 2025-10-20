use diesel::{
    sql_query, sql_types::Text, Connection, ConnectionError, RunQueryDsl as _, SqliteConnection,
};

pub mod migration;
pub mod models;
pub mod schema;

pub struct Database {
    pub conn: SqliteConnection,
}

impl Database {
    pub fn new(path: &str) -> Result<Self, ConnectionError> {
        let conn = SqliteConnection::establish(path)?;
        Ok(Database {
            conn,
        })
    }

    pub fn new_multi(paths: &[&str]) -> Result<Self, ConnectionError> {
        let mut conn = SqliteConnection::establish(paths[0])?;
        sql_query("PRAGMA case_sensitive_like = ON;")
            .execute(&mut conn)
            .map_err(|err| ConnectionError::BadConnection(err.to_string()))?;
        for (idx, path) in paths.iter().enumerate().skip(1) {
            sql_query(format!("ATTACH DATABASE ?1 AS shard{}", idx))
                .bind::<Text, _>(path)
                .execute(&mut conn)
                .map_err(|err| ConnectionError::BadConnection(err.to_string()))?;
        }

        Ok(Database {
            conn,
        })
    }
}

impl std::ops::Deref for Database {
    type Target = SqliteConnection;

    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}

impl std::ops::DerefMut for Database {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.conn
    }
}
