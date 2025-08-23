use rusqlite::{params, Result};

use crate::database::models::FromRow;

use super::models::Nest;

pub fn add(tx: &rusqlite::Transaction, nest: &Nest) -> Result<()> {
    tx.execute(
        "INSERT INTO nests (name, url) VALUES (?1, ?2)",
        params![nest.name, nest.url],
    )?;
    Ok(())
}

pub fn list(tx: &rusqlite::Transaction) -> Result<Vec<Nest>> {
    let mut stmt = tx.prepare("SELECT id, name, url FROM nests")?;
    let nests = stmt
        .query_map([], Nest::from_row)?
        .filter_map(|n| match n {
            Ok(nest) => Some(nest),
            Err(err) => {
                eprintln!("Nest map error: {err:#?}");
                None
            }
        })
        .collect();
    Ok(nests)
}

pub fn remove(tx: &rusqlite::Transaction, name: &str) -> Result<()> {
    tx.execute("DELETE FROM nests WHERE name = ?1", params![name])?;
    Ok(())
}
