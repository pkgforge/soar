use std::any::type_name;

use documented::{Documented, DocumentedFields};
use toml_edit::{ArrayOfTables, Decor, Item, RawString, Table};
use tracing::warn;

use crate::error::ConfigError;

/// Appends documentation lines as TOML comments to the given `Decor`.
///
/// This function transforms each line in the provided documentation string
/// into a TOML comment (prefixed with `#`) and appends it to the existing
/// comment prefix in `decor`, preserving formatting.
///
/// # Arguments
/// * `decor` - Mutable reference to the TOML `Decor` where comments should be inserted.
/// * `docs` - The documentation string to convert to TOML comments.
pub fn append_docs_as_toml_comments(decor: &mut Decor, docs: &str) {
    let old_prefix = decor.prefix().and_then(RawString::as_str);
    let last_line = old_prefix.and_then(|prefix| prefix.lines().last());

    let comments = docs
        .lines()
        .map(|l| {
            if l.is_empty() {
                "#\n".into()
            } else {
                format!("# {l}\n")
            }
        })
        .collect();

    let new_prefix = match (old_prefix, last_line) {
        (None | Some(""), None) => comments,
        (None, Some(_)) => unreachable!(),
        (Some(_), None) => unreachable!(),
        (Some(prefix), Some("")) => format!("{prefix}{comments}"),
        (Some(prefix), Some(_)) => format!("{prefix}#\n{comments}"),
    };
    decor.set_prefix(new_prefix);
}

/// Annotates a TOML `Table` with documentation extracted from a struct `T` that implements
/// the `Documented` and `DocumentedFields` traits.
///
/// This adds documentation comments above each key in the table based on field-level documentation,
/// and optionally includes the struct-level documentation if `is_root` is false.
///
/// # Arguments
/// * `table` - Mutable reference to the TOML table to annotate.
/// * `is_root` - Whether this table is the root; root tables don't get container-level doc comments.
///
/// # Returns
/// Returns `Ok(())` if successful, or a `ConfigError` if a TOML item is unexpectedly `None`.
pub fn annotate_toml_table<T>(table: &mut Table, is_root: bool) -> Result<(), ConfigError>
where
    T: Documented + DocumentedFields,
{
    if !is_root {
        append_docs_as_toml_comments(table.decor_mut(), T::DOCS);
    }

    for (mut key_mut, value_item) in table.iter_mut() {
        let key_str = key_mut.get();
        match T::get_field_docs(key_str) {
            Ok(docs) => match value_item {
                Item::None => {
                    return Err(ConfigError::Custom(format!(
                        "Encountered TomlEditItem::None for key '{key_str}' unexpectedly",
                    )))
                }
                Item::Value(_) => append_docs_as_toml_comments(key_mut.leaf_decor_mut(), docs),
                Item::Table(sub_table) => append_docs_as_toml_comments(sub_table.decor_mut(), docs),
                Item::ArrayOfTables(array) => {
                    let first_table = array
                        .iter_mut()
                        .next()
                        .expect("Array of table should not be empty");
                    append_docs_as_toml_comments(first_table.decor_mut(), docs);
                }
            },
            Err(_) => {
                warn!(
                    "Field '{}' found in TOML but not in struct '{}' for documentation lookup, or it's a complex case like flatten.",
                    key_str,
                    type_name::<T>()
                );
            }
        }
    }

    Ok(())
}

/// Annotates the first table in a TOML `ArrayOfTables` using documentation from the given struct `T`.
///
/// This assumes that the structure of all tables in the array is the same, so only the first table is annotated.
///
/// # Arguments
/// * `array` - Mutable reference to the TOML array of tables to annotate.
///
/// # Returns
/// Returns `Ok(())` if annotation succeeds, or a `ConfigError` if annotation fails on the first table.
pub fn annotate_toml_array_of_tables<T>(array: &mut ArrayOfTables) -> Result<(), ConfigError>
where
    T: Documented + DocumentedFields,
{
    if let Some(first_table) = array.iter_mut().next() {
        annotate_toml_table::<T>(first_table, false).map_err(|err| {
            ConfigError::Custom(format!("Failed to annotate first table in array: {err}"))
        })?;
    }
    Ok(())
}
