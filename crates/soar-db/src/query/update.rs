use std::sync::{Arc, Mutex};

use rusqlite::{types::Value, Connection, ToSql};

use crate::{expr::Col, traits::Expression};

pub struct UpdateQuery {
    db: Arc<Mutex<Connection>>,
    table: &'static str,
    updates: Vec<(String, Value)>,
    wheres: Vec<Box<dyn Fn(&mut Vec<Value>) -> String>>,
}

impl UpdateQuery {
    pub fn table(db: Arc<Mutex<Connection>>, table: &'static str) -> Self {
        Self {
            db,
            table,
            updates: vec![],
            wheres: vec![],
        }
    }

    pub fn set<T, V: Into<Value>>(mut self, col: Col<T>, value: V) -> Self {
        self.updates.push((col.name.to_string(), value.into()));
        self
    }

    pub fn filter<Expr: Expression + 'static>(mut self, expr: Expr) -> Self {
        self.wheres
            .push(Box::new(move |params| expr.to_sql(params)));
        self
    }

    pub fn execute(self) -> rusqlite::Result<usize> {
        let (sql, params) = self.build_sql();
        let conn = self.db.lock().unwrap();

        let params_ref: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
        conn.execute(&sql, params_ref.as_slice())
    }

    fn build_sql(&self) -> (String, Vec<Value>) {
        let mut params = Vec::new();

        let sets: Vec<String> = self
            .updates
            .iter()
            .map(|(col, val)| {
                params.push(val.clone());
                format!("{} = ?", col)
            })
            .collect();

        let mut sql = format!("UPDATE {} SET {}", self.table, sets.join(", "));

        if !self.wheres.is_empty() {
            sql.push_str(" WHERE ");
            let conditions: Vec<String> = self.wheres.iter().map(|w| w(&mut params)).collect();

            sql.push_str(&conditions.join(" AND "));
        }

        (sql, params)
    }
}
