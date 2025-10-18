use std::sync::{Arc, Mutex};

use rusqlite::{types::Value, Connection, ToSql};

use crate::traits::Expression;

pub struct DeleteQuery {
    db: Arc<Mutex<Connection>>,
    table: &'static str,
    wheres: Vec<Box<dyn Fn(&mut Vec<Value>) -> String>>,
}

impl DeleteQuery {
    pub fn from(db: Arc<Mutex<Connection>>, table: &'static str) -> Self {
        Self {
            db,
            table,
            wheres: Vec::new(),
        }
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
        let mut sql = format!("DELETE FROM {}", self.table);

        if !self.wheres.is_empty() {
            sql.push_str(" WHERE ");
            let conditions: Vec<String> = self.wheres.iter().map(|w| w(&mut params)).collect();
            sql.push_str(&conditions.join(" AND "));
        }

        (sql, params)
    }
}
