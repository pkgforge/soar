use std::{collections::HashMap, fmt::Display};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy)]
pub enum FilterOp {
    Eq,
    Like,
    ILike,
    Gt,
    Gte,
    Lt,
    Lte,
    NotEq,
    In,
    NotIn,
    IsNull,
    IsNotNull,
    Between,
}

#[derive(Debug, Clone)]
pub enum FilterValue {
    Single(String),
    Multiple(Vec<String>),
    Range(String, String),
    None,
}

#[derive(Debug, Clone)]
pub struct Filter {
    pub operator: FilterOp,
    pub value: FilterValue,
}

impl From<(String, String)> for FilterValue {
    fn from(value: (String, String)) -> Self {
        FilterValue::Range(value.0, value.1)
    }
}

impl From<String> for FilterValue {
    fn from(value: String) -> Self {
        FilterValue::Single(value)
    }
}

impl From<bool> for FilterValue {
    fn from(value: bool) -> Self {
        FilterValue::Single(value.then(|| "1").unwrap_or("0").to_string())
    }
}

impl From<Vec<String>> for FilterValue {
    fn from(value: Vec<String>) -> Self {
        FilterValue::Multiple(value)
    }
}

impl From<(FilterOp, FilterValue)> for Filter {
    fn from(value: (FilterOp, FilterValue)) -> Self {
        Filter {
            operator: value.0,
            value: value.1,
        }
    }
}

impl FilterOp {
    pub fn to_sql(&self) -> &'static str {
        match self {
            FilterOp::Eq => "=",
            FilterOp::Like => "LIKE",
            FilterOp::ILike => "LIKE",
            FilterOp::Gt => ">",
            FilterOp::Gte => ">=",
            FilterOp::Lt => "<",
            FilterOp::Lte => "<=",
            FilterOp::NotEq => "!=",
            FilterOp::In => "IN",
            FilterOp::NotIn => "NOT IN",
            FilterOp::IsNull => "IS NULL",
            FilterOp::IsNotNull => "IS NOT NULL",
            FilterOp::Between => "BETWEEN",
        }
    }
}

#[derive(Debug, Clone)]
pub struct QueryOptions {
    pub page: u32,
    pub limit: u32,
    pub filters: HashMap<String, Filter>,
    pub sort_by: Vec<(String, SortOrder)>,
}

impl Default for QueryOptions {
    fn default() -> Self {
        Self {
            page: 1,
            limit: u32::MAX,
            filters: HashMap::new(),
            sort_by: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub page: u32,
    pub limit: u32,
    pub total: u64,
    pub has_next: bool,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub enum ProvideStrategy {
    KeepTargetOnly,
    KeepBoth,
    Alias,
    #[default]
    None,
}

impl Display for ProvideStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            ProvideStrategy::KeepTargetOnly => "=>",
            ProvideStrategy::KeepBoth => "==",
            ProvideStrategy::Alias => ":",
            _ => "",
        };
        write!(f, "{}", msg)
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct PackageProvide {
    pub name: String,
    pub target_name: Option<String>,
    pub strategy: ProvideStrategy,
}

impl PackageProvide {
    pub fn from_string(provide: &str) -> Self {
        if let Some((name, target_name)) = provide.split_once("==") {
            Self {
                name: name.to_string(),
                target_name: Some(target_name.to_string()),
                strategy: ProvideStrategy::KeepBoth,
            }
        } else if let Some((name, target_name)) = provide.split_once("=>") {
            Self {
                name: name.to_string(),
                target_name: Some(target_name.to_string()),
                strategy: ProvideStrategy::KeepTargetOnly,
            }
        } else if let Some((name, target_name)) = provide.split_once(":") {
            Self {
                name: name.to_string(),
                target_name: Some(target_name.to_string()),
                strategy: ProvideStrategy::Alias,
            }
        } else {
            Self {
                name: provide.to_string(),
                target_name: None,
                strategy: ProvideStrategy::None,
            }
        }
    }
}
