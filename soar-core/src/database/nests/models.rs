use serde::{Deserialize, Serialize};

use crate::database::models::FromRow;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Nest {
    pub id: i64,
    pub name: String,
    pub url: String,
}

impl FromRow for Nest {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Nest {
            id: row.get("id")?,
            name: row
                .get::<_, String>("name")?
                .strip_prefix("nest-")
                .unwrap()
                .to_string(),
            url: row.get("url")?,
        })
    }
}
