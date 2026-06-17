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
