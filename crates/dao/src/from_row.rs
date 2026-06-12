use crate::error::Result;
use crate::from_sql::FromSqlColumn;
use crate::row::Row;

/// Trait for converting a database row into a Rust type.
///
/// This trait is automatically implemented by the `#[derive(Entity)]` macro
/// on structs. For scalar (single-column) results, a blanket implementation
/// exists for any type that implements `FromSqlColumn`.
pub trait FromRow: Sized {
    fn from_row(row: &Row) -> Result<Self>;
}

/// Blanket implementation for scalar (single-column) results.
///
/// Any type that can be converted from a single column value can be returned
/// directly from a query without needing a struct. This enables queries like
/// `SELECT COUNT(*) FROM table` to return `i64` directly.
impl<T: FromSqlColumn> FromRow for T {
    fn from_row(row: &Row) -> Result<Self> {
        let col = row.get_column(0)?;
        T::from_column(&col)
    }
}
