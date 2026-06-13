use crate::error::Result;
use crate::pool::Param;

/// Trait for converting a Rust type into a database parameter value.
///
/// This is the inverse of [`FromSqlColumn`](crate::FromSqlColumn) and the primary
/// extension point for custom types on the write path. Implement this trait for
/// your newtypes and domain types to enable direct mapping to database parameters.
///
/// A blanket implementation covers all types that implement
/// `rusqlite::types::ToSql + Clone + Send`, so primitive types (`String`, `i64`,
/// `f64`, `bool`, `Vec<u8>`, `Option<T>`) work automatically.
///
/// # Example
///
/// ```ignore
/// struct Email(String);
///
/// impl ToSqlColumn for Email {
///     fn to_column(&self) -> Result<Param> {
///         self.0.to_column() // delegate to String's blanket impl
///     }
/// }
/// ```
pub trait ToSqlColumn {
    fn to_column(&self) -> Result<Param>;
}

/// Blanket implementation: any type that rusqlite can handle and that is
/// `Clone + Send`, we can handle.
///
/// This clones the value and boxes it as a [`Param`]. All primitive types
/// satisfy these bounds automatically.
impl<T: rusqlite::types::ToSql + Clone + Send + 'static> ToSqlColumn for T {
    fn to_column(&self) -> Result<Param> {
        Ok(Box::new(self.clone()))
    }
}
