use crate::error::Result;
use crate::pool::Param;

/// Trait for converting a Rust struct into database parameter values.
///
/// This is the inverse of [`FromRow`](crate::FromRow). The `Entity` derive macro
/// generates this implementation automatically when `#[dao(table = "...")]` is
/// present on the struct.
///
/// Three methods produce params in different orderings to match the SQL generated
/// by [`EntityMeta`](crate::EntityMeta):
///
/// - `to_insert_params` — fields in declaration order (matches `INSERT INTO t (...) VALUES (?, ...)`).
/// - `to_update_params` — non-PK fields first (SET clause), then PK fields last (WHERE clause).
/// - `to_delete_params` — PK fields only (WHERE clause).
pub trait ToRow {
    /// Convert to parameter values in declaration order (for INSERT).
    fn to_insert_params(&self) -> Result<Vec<Param>>;

    /// Convert to parameter values with non-PK fields first, PK fields last (for UPDATE).
    fn to_update_params(&self) -> Result<Vec<Param>>;

    /// Convert to parameter values containing only PK fields (for DELETE).
    fn to_delete_params(&self) -> Result<Vec<Param>>;
}
