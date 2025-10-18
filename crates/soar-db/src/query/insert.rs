use std::sync::{Arc, Mutex};

use rusqlite::{types::Value, Connection, ToSql};

use crate::expr::Col;

pub struct InsertQuery {
    db: Arc<Mutex<Connection>>,
    table: &'static str,
    columns: Vec<String>,
    values: Vec<Value>,
    on_conflict: Option<String>,
}

impl InsertQuery {
    pub fn into(db: Arc<Mutex<Connection>>, table: &'static str) -> Self {
        Self {
            db,
            table,
            columns: vec![],
            values: vec![],
            on_conflict: None,
        }
    }

    pub fn set<T, V: Into<Value>>(mut self, col: Col<T>, value: V) -> Self {
        self.columns.push(col.name.to_string());
        self.values.push(value.into());
        self
    }

    pub fn on_conflict_do_nothing(mut self) -> Self {
        self.on_conflict = Some("ON CONFLICT DO NOTHING".to_string());
        self
    }

    pub fn on_conflict_update(mut self, conflict_cols: &[&str], update_cols: &[&str]) -> Self {
        let conflict = conflict_cols.join(", ");
        let updates: Vec<String> = update_cols
            .iter()
            .map(|col| format!("{} = excluded.{}", col, col))
            .collect();
        self.on_conflict = Some(format!(
            "ON CONFLICT({}) DO UPDATE SET {}",
            conflict,
            updates.join(", ")
        ));
        self
    }

    pub fn execute(self) -> rusqlite::Result<i64> {
        let (sql, params) = self.build_sql();
        let conn = self.db.lock().unwrap();

        let params_ref: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
        conn.execute(&sql, params_ref.as_slice())?;
        Ok(conn.last_insert_rowid())
    }

    fn build_sql(&self) -> (String, Vec<Value>) {
        let columns = self.columns.join(", ");
        let placeholders = vec!["?"; self.values.len()].join(", ");

        let mut sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            self.table, columns, placeholders
        );

        if let Some(conflict) = &self.on_conflict {
            sql.push_str(" ");
            sql.push_str(conflict);
        }

        (sql, self.values.clone())
    }
}
