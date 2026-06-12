use crate::error::{Error, Result};
use crate::row::ColumnValue;

/// Trait for converting a single database column value into a Rust type.
///
/// This is the primary extension point for custom types. Implement this trait
/// for your newtypes and domain types to enable direct mapping from database
/// columns.
///
/// A blanket implementation covers all types that implement
/// `rusqlite::types::FromSql`, so primitive types (`String`, `i64`, `f64`,
/// `bool`, `Vec<u8>`, `Option<T>`) work automatically.
///
/// # Example
///
/// ```ignore
/// struct Email(String);
///
/// impl FromSqlColumn for Email {
///     fn from_column(value: &ColumnValue) -> Result<Self> {
///         let s = String::from_column(value)?;
///         if s.contains('@') {
///             Ok(Email(s))
///         } else {
///             Err(Error::custom("not a valid email"))
///         }
///     }
/// }
/// ```
pub trait FromSqlColumn: Sized {
    fn from_column(value: &ColumnValue) -> Result<Self>;
}

/// Blanket implementation: any type that rusqlite can handle, we can handle.
///
/// This works because rusqlite's `FromSql::column_result()` accepts a
/// `ValueRef`, which is exactly what our `ColumnValue` wraps internally.
impl<T: rusqlite::types::FromSql> FromSqlColumn for T {
    fn from_column(value: &ColumnValue) -> Result<Self> {
        // Pass the ValueRef directly to rusqlite's FromSql::column_result.
        T::column_result(value.inner())
            .map_err(|e| Error::custom(format!("column conversion error: {e}")))
    }
}
