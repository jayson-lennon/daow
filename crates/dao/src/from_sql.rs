// Copyright (C) 2026 Jayson Lennon
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU Lesser General Public
// License as published by the Free Software Foundation; either
// version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
// Lesser General Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License
// along with this program; if not, see <https://opensource.org/license/lgpl-3-0>.

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
