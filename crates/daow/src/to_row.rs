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
