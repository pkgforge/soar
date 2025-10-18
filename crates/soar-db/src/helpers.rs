use serde::{Deserialize, Serialize};

pub fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

pub fn from_json<T: for<'de> Deserialize<'de> + Default>(s: String) -> T {
    serde_json::from_str(&s).unwrap_or_default()
}

pub fn from_optional_json<T: for<'de> Deserialize<'de>>(
    result: rusqlite::Result<String>,
) -> Option<T> {
    match result {
        Ok(s) if !s.is_empty() && s != "null" => serde_json::from_str(&s).ok(),
        _ => None,
    }
}
