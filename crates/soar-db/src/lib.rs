pub mod expr;
pub mod macros;
pub mod query;
pub mod traits;

pub use query::*;
pub use traits::FromRow;

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use rusqlite::{params, Connection, Row};

    use super::*;
    use crate::{
        query::builder::Query,
        traits::{Expression as _, FromRow},
    };

    #[derive(Debug, Clone)]
    struct Package {
        pub id: u64,
        pub name: String,
        pub version: String,
        pub downloads: u64,
    }

    impl FromRow for Package {
        fn from_row(row: &Row) -> rusqlite::Result<Self> {
            Ok(Self {
                id: row.get("id")?,
                name: row.get("name")?,
                version: row.get("version")?,
                downloads: row.get("downloads")?,
            })
        }
    }

    define_entity!(
        packages {
            table: "packages",
            columns: {
                ID: i64 => "id",
                NAME: String => "name",
                VERSION: String => "version",
                DOWNLOADS: u64 => "downloads"
            }
        }
    );

    fn setup_db() -> Arc<Mutex<Connection>> {
        let conn = Connection::open_in_memory().unwrap();

        conn.execute(
            "CREATE TABLE packages (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                downloads INTEGER NOT NULL
            )",
            [],
        )
        .unwrap();

        // insert test data
        conn.execute(
            "INSERT INTO packages (name, version, downloads) VALUES (?1, ?2, ?3)",
            params!["soar", "1.0.0", 120],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO packages (name, version, downloads) VALUES (?1, ?2, ?3)",
            params!["soar", "1.1.0", 340],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO packages (name, version, downloads) VALUES (?1, ?2, ?3)",
            params!["glide", "0.9.1", 50],
        )
        .unwrap();

        Arc::new(Mutex::new(conn))
    }

    #[test]
    fn test_basic_fetch() {
        let db = setup_db();

        let results = Query::<Package>::from(db.clone(), "packages")
            .filter(packages::NAME.eq("soar".to_string()))
            .order_by(packages::VERSION, false)
            .fetch()
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "soar");
    }

    #[test]
    fn test_fetch_one() {
        let db = setup_db();

        let one = Query::<Package>::from(db.clone(), "packages")
            .filter(packages::NAME.eq("glide".to_string()))
            .fetch_one()
            .unwrap()
            .unwrap();

        assert_eq!(one.name, "glide");
        assert_eq!(one.version, "0.9.1");
    }

    #[test]
    fn test_count() {
        let db = setup_db();

        let count = Query::<Package>::from(db.clone(), "packages")
            .filter(packages::NAME.like("soar"))
            .count()
            .unwrap();

        assert_eq!(count, 2);
    }

    #[test]
    fn test_pagination() {
        let db = setup_db();

        let results = Query::<Package>::from(db.clone(), "packages")
            .filter(packages::DOWNLOADS.gt(100))
            .order_by(packages::DOWNLOADS, true)
            .page(1, 1) // first page, one item
            .fetch()
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].downloads, 340);
    }

    #[test]
    fn test_in_clause() {
        let db = setup_db();

        let results = Query::<Package>::from(db.clone(), "packages")
            .filter(packages::NAME.in_(["soar", "glide"].iter().map(|s| s.to_string())))
            .fetch()
            .unwrap();

        assert_eq!(results.len(), 3);
    }
}
