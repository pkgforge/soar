use diesel::{
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    serialize::{self, Output, ToSql},
    sql_types::{Jsonb, Text},
    sqlite::Sqlite,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, FromSqlRow, AsExpression)]
#[diesel(sql_type = Jsonb)]
pub struct JsonValue<T>(pub T);

impl<T> FromSql<Jsonb, Sqlite> for JsonValue<T>
where
    T: for<'de> Deserialize<'de>,
{
    fn from_sql(bytes: diesel::sqlite::SqliteValue) -> deserialize::Result<Self> {
        let text = <String as FromSql<Text, Sqlite>>::from_sql(bytes)?;
        let value = serde_json::from_str::<T>(&text)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(JsonValue(value))
    }
}

impl<T> ToSql<Jsonb, Sqlite> for JsonValue<T>
where
    T: Serialize + std::fmt::Debug,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        let json = serde_json::to_vec(&self.0)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        out.set_value(json);
        Ok(serialize::IsNull::No)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum ProvideStrategy {
    KeepTargetOnly,
    KeepBoth,
    Alias,
}

impl std::fmt::Display for ProvideStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            ProvideStrategy::KeepTargetOnly => "=>",
            ProvideStrategy::KeepBoth => "==",
            ProvideStrategy::Alias => ":",
        };
        write!(f, "{msg}")
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct PackageProvide {
    pub name: String,
    pub target: Option<String>,
    pub strategy: Option<ProvideStrategy>,
}

impl PackageProvide {
    pub fn from_string(provide: &str) -> Self {
        if let Some((name, target_name)) = provide.split_once("==") {
            Self {
                name: name.to_string(),
                target: Some(target_name.to_string()),
                strategy: Some(ProvideStrategy::KeepBoth),
            }
        } else if let Some((name, target_name)) = provide.split_once("=>") {
            Self {
                name: name.to_string(),
                target: Some(target_name.to_string()),
                strategy: Some(ProvideStrategy::KeepTargetOnly),
            }
        } else if let Some((name, target_name)) = provide.split_once(":") {
            Self {
                name: name.to_string(),
                target: Some(target_name.to_string()),
                strategy: Some(ProvideStrategy::Alias),
            }
        } else {
            Self {
                name: provide.to_string(),
                target: None,
                strategy: None,
            }
        }
    }
}
