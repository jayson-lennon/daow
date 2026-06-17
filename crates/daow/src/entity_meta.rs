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

/// Metadata about an entity's database mapping.
///
/// This trait is automatically implemented by the `#[derive(Entity)]` macro
/// when the struct has a `#[dao(table = "...")]` attribute. It provides the
/// table name, field count, primary key indices, and generated SQL statements
/// used by the `#[insert]`, `#[update]`, and `#[delete]` annotations.
///
/// You should not implement this trait manually — let the derive macro handle it.
pub trait EntityMeta {
    /// The database table name.
    const TABLE_NAME: &'static str;

    /// The number of fields in the entity (used for compile-time validation).
    const FIELD_COUNT: usize;

    /// Indices of fields marked as primary key (0-based, in declaration order).
    const PK_INDICES: &'static [usize];

    /// Returns the generated INSERT SQL for this entity.
    ///
    /// Format: `INSERT INTO table_name (col1, col2, ...) VALUES (?, ?, ...)`
    fn insert_sql() -> &'static str;

    /// Returns the generated UPSERT SQL for this entity.
    ///
    /// Format: `INSERT INTO table_name (cols) VALUES (?, ...) ON CONFLICT(pk) DO UPDATE SET
    /// non_pk = excluded.non_pk`.
    ///
    /// For all-PK entities (junction tables), emits `... ON CONFLICT(pk) DO NOTHING` since there
    /// are no non-PK columns to SET. Requires SQLite >= 3.24 for the `excluded.<col>` form.
    fn upsert_sql() -> &'static str;


    /// Returns the generated UPDATE SQL for this entity.
    ///
    /// Format: `UPDATE table_name SET non_pk1 = ?, non_pk2 = ?, ... WHERE pk1 = ?`
    fn update_sql() -> &'static str;

    /// Returns the generated DELETE SQL for this entity.
    ///
    /// Format: `DELETE FROM table_name WHERE pk1 = ?`
    fn delete_sql() -> &'static str;
}

/// Result of a write operation (`INSERT`, `UPDATE`, `DELETE`).
#[derive(Debug)]
pub struct ExecuteResult {
    /// Number of rows affected by the statement.
    pub rows_affected: u64,
    /// The rowid of the last inserted row (0 if not an INSERT).
    pub last_insert_rowid: i64,
}
