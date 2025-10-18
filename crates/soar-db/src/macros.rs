//! Macros for defining entity schemas.
//!
//! The [`define_entity!`] macro generates column constants for a table,
//! tying database column names to Rust types.

/// Defines a module with typed column constants for a database table.
///
/// This macro generates a public module containing `const` declarations
/// for each column, making it easy to reference columns in queries.
///
/// # Syntax
///
/// ```ignore
/// define_entity!(
///     users {
///         table: "users",
///         columns: {
///             ID: i64 => "id",
///             NAME: String => "name"
///         }
///     }
/// );
/// ```
///
/// This expands to:
///
/// ```ignore
/// pub mod users {
///     pub const ID: soar_db::Col<i64> = soar_db::Col::new("id");
///     pub const NAME: soar_db::Col<String> = soar_db::Col::new("name");
/// }
/// ```
///
/// # Usage
///
/// ```ignore
/// use soar_db::{SelectQuery, define_entity, FromRow};
/// define_entity!(
///     users {
///         table: "users",
///         columns: {
///             ID: i64 => "id",
///             NAME: String => "name"
///         }
///     }
/// );
/// #[derive(Debug)]
/// struct User {
///     name: String
/// }
///
/// impl FromRow for User {
///     fn from_row(_: &rusqlite::Row) -> rusqlite::Result<Self> {
///         Ok(User {
///             name: "User".to_string()
///         })
///     }
/// }
///
/// let db = Arc::new(Mutex::new(rusqlite::Connection::open_in_memory().unwrap()));
/// SelectQuery::<User>::from(db, "users")
///     .filter(users::NAME.eq("User".to_string()));
/// ```
#[macro_export]
macro_rules! define_entity {
    (
        $entity:ident {
            table: $table:literal,
            columns: {
                $($col_name:ident: $col_type:ty => $db_col:literal),* $(,)?
            }
        }
    ) => {
        pub mod $entity {
            use $crate::expr::column::Col;

            pub const TABLE: &str = $table;

            $(
                $crate::define_column!($col_name, $col_type, $db_col);
            )*
        }
    };
}

#[macro_export]
macro_rules! define_column {
    // JSON detection - Vec<T>
    ($name:ident, Vec<$inner:ty>, $db_col:literal) => {
        pub const $name: Col<String> = Col::json($db_col);
    };

    // JSON detection - Option<Vec<T>>
    ($name:ident, Option<Vec<$inner:ty>>, $db_col:literal) => {
        pub const $name: Col<Option<String>> = Col::json($db_col);
    };

    // Optional regular types
    ($name:ident, Option<$inner:ty>, $db_col:literal) => {
        pub const $name: Col<Option<$inner>> = Col::new($db_col);
    };

    // Regular types (fallback)
    ($name:ident, $type:ty, $db_col:literal) => {
        pub const $name: Col<$type> = Col::new($db_col);
    };
}
