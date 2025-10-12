//! SQL expression operators.
//!
//! These structs represent compound expressions like `col = ?`, `col LIKE ?`, etc.
//! Each implements [`Expression`] and recursively builds SQL fragments.

use rusqlite::types::Value;

use crate::traits::Expression;

/// Represents a binary comparison (e.g., `=`, `>`, `<=`).
pub struct BinaryOp<L> {
    left: L,
    op: &'static str,
    right: Value,
}

impl<L> BinaryOp<L> {
    pub fn new(left: L, op: &'static str, right: Value) -> Self {
        Self {
            left,
            op,
            right,
        }
    }
}

impl<L: Expression> Expression for BinaryOp<L> {
    fn to_sql(&self, params: &mut Vec<Value>) -> String {
        let left_sql = self.left.to_sql(params);
        params.push(self.right.clone());
        format!("{} {} ?", left_sql, self.op)
    }
}

/// Represents a `LIKE` or case-insensitive `LIKE` pattern match.
pub struct LikeOp<L> {
    left: L,
    pattern: String,
    case_insensitive: bool,
}

impl<L> LikeOp<L> {
    pub const fn new(left: L, pattern: String, case_insensitive: bool) -> Self {
        Self {
            left,
            pattern,
            case_insensitive,
        }
    }
}

impl<L: Expression> Expression for LikeOp<L> {
    fn to_sql(&self, params: &mut Vec<Value>) -> String {
        let left_sql = self.left.to_sql(params);
        params.push(format!("%{}%", self.pattern).into());
        if self.case_insensitive {
            format!("LOWER({}) LIKE LOWER(?)", left_sql)
        } else {
            format!("{} LIKE ?", left_sql)
        }
    }
}

/// Represents an `IN` or `NOT IN` clause.
pub struct InOp<L> {
    left: L,
    values: Vec<Value>,
    negated: bool,
}

impl<L> InOp<L> {
    pub fn new(left: L, values: Vec<Value>, negated: bool) -> Self {
        Self {
            left,
            values,
            negated,
        }
    }
}

impl<L: Expression> Expression for InOp<L> {
    fn to_sql(&self, params: &mut Vec<Value>) -> String {
        let left_sql = self.left.to_sql(params);
        let placeholders = vec!["?"; self.values.len()].join(", ");
        for v in &self.values {
            params.push(v.clone());
        }
        let op = if self.negated { "NOT IN" } else { "IN" };
        format!("{} {} ({})", left_sql, op, placeholders)
    }
}

/// Represents an `IS NULL` or `IS NOT NULL` check.
pub struct NullOp<L> {
    left: L,
    is_null: bool,
}

impl<L> NullOp<L> {
    pub fn new(left: L, is_null: bool) -> Self {
        Self {
            left,
            is_null,
        }
    }
}

impl<L: Expression> Expression for NullOp<L> {
    fn to_sql(&self, params: &mut Vec<Value>) -> String {
        let left_sql = self.left.to_sql(params);
        let op = if self.is_null {
            "IS NULL"
        } else {
            "IS NOT NULL"
        };
        format!("{} {}", left_sql, op)
    }
}

/// Combines two expressions with `AND` or `OR`.
pub struct LogicalOp<L, R> {
    left: L,
    right: R,
    op: &'static str,
}

impl<L, R> LogicalOp<L, R> {
    pub fn new(left: L, right: R, op: &'static str) -> Self {
        Self {
            left,
            right,
            op,
        }
    }
}

impl<L: Expression, R: Expression> Expression for LogicalOp<L, R> {
    fn to_sql(&self, params: &mut Vec<Value>) -> String {
        let left_sql = self.left.to_sql(params);
        let right_sql = self.right.to_sql(params);
        format!("({} {} {})", left_sql, self.op, right_sql)
    }
}
