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
use crate::from_sql::FromSqlColumn;
use rusqlite::types::ValueRef;

/// A borrowed value from a single column in a database row.
///
/// Wraps rusqlite's `ValueRef` to hide the implementation detail.
pub struct ColumnValue<'a> {
    inner: ValueRef<'a>,
}

impl<'a> ColumnValue<'a> {
    pub(crate) fn new(inner: ValueRef<'a>) -> Self {
        Self { inner }
    }

    /// Returns the inner rusqlite ValueRef.
    /// Used by the blanket FromSqlColumn impl to delegate to rusqlite.
    pub(crate) fn inner(&self) -> ValueRef<'a> {
        self.inner
    }

    /// Returns the value as a string slice.
    ///
    /// Returns an error if the column is not a text value.
    pub fn as_str(&self) -> Result<&str> {
        match &self.inner {
            ValueRef::Text(s) => std::str::from_utf8(s)
                .map_err(|e| Error::custom(format!("invalid utf-8 in column: {e}"))),
            ValueRef::Null => Err(Error::TypeMismatch {
                column: "unknown".to_string(),
                expected: "text",
                got: "null",
            }),
            ValueRef::Integer(_) => Err(Error::TypeMismatch {
                column: "unknown".to_string(),
                expected: "text",
                got: "integer",
            }),
            ValueRef::Real(_) => Err(Error::TypeMismatch {
                column: "unknown".to_string(),
                expected: "text",
                got: "real",
            }),
            ValueRef::Blob(_) => Err(Error::TypeMismatch {
                column: "unknown".to_string(),
                expected: "text",
                got: "blob",
            }),
        }
    }

    /// Returns the value as an i64.
    pub fn as_i64(&self) -> Result<i64> {
        match &self.inner {
            ValueRef::Integer(i) => Ok(*i),
            other => Err(Error::TypeMismatch {
                column: "unknown".to_string(),
                expected: "integer",
                got: value_type_name(other),
            }),
        }
    }

    /// Returns the value as an f64.
    pub fn as_f64(&self) -> Result<f64> {
        match &self.inner {
            ValueRef::Real(f) => Ok(*f),
            other => Err(Error::TypeMismatch {
                column: "unknown".to_string(),
                expected: "real",
                got: value_type_name(other),
            }),
        }
    }

    /// Returns the value as a byte slice.
    pub fn as_blob(&self) -> Result<&[u8]> {
        match &self.inner {
            ValueRef::Blob(b) => Ok(b),
            other => Err(Error::TypeMismatch {
                column: "unknown".to_string(),
                expected: "blob",
                got: value_type_name(other),
            }),
        }
    }

    /// Returns true if the column value is NULL.
    pub fn is_null(&self) -> bool {
        matches!(self.inner, ValueRef::Null)
    }
}

fn value_type_name(v: &ValueRef<'_>) -> &'static str {
    match v {
        ValueRef::Null => "null",
        ValueRef::Integer(_) => "integer",
        ValueRef::Real(_) => "real",
        ValueRef::Text(_) => "text",
        ValueRef::Blob(_) => "blob",
    }
}

/// A borrowed row from a database query result.
///
/// Wraps rusqlite's `Row` type. Provides methods to extract column values
/// by name or index, converting them through the `FromSqlColumn` trait.
pub struct Row<'a> {
    inner: &'a rusqlite::Row<'a>,
}

impl<'a> Row<'a> {
    pub(crate) fn new(inner: &'a rusqlite::Row<'a>) -> Self {
        Self { inner }
    }

    /// Extracts a column value by name and converts it via `FromSqlColumn`.
    ///
    /// Internally: reads the rusqlite column value, wraps it in `ColumnValue`,
    /// then delegates to `T::from_column()`.
    pub fn get<T: FromSqlColumn>(&self, column_name: &str) -> Result<T> {
        let value_ref = self.inner.get_ref(column_name).map_err(|e| match e {
            rusqlite::Error::InvalidColumnIndex(_) => Error::ColumnNotFound {
                name: column_name.to_string(),
            },
            other => Error::Database(other),
        })?;
        let col_val = ColumnValue::new(value_ref);
        T::from_column(&col_val)
    }

    /// Extracts a column value by index and converts it via `FromSqlColumn`.
    pub fn get_by_index<T: FromSqlColumn>(&self, index: usize) -> Result<T> {
        let value_ref = self.inner.get_ref(index)?;
        let col_val = ColumnValue::new(value_ref);
        T::from_column(&col_val)
    }

    /// Returns a `ColumnValue` for the given column index.
    pub fn get_column(&self, index: usize) -> Result<ColumnValue<'_>> {
        let value_ref = self.inner.get_ref(index)?;
        Ok(ColumnValue::new(value_ref))
    }
}
