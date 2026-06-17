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
