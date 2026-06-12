use std::sync::{Arc, Mutex};

use crate::error::{Error, Result};
use crate::from_row::FromRow;
use crate::row::Row;

use rusqlite::Connection;

/// Type-erased parameter that can be sent across threads.
pub type Param = Box<dyn rusqlite::types::ToSql + Send>;

/// A connection pool for SQLite database access.
///
/// Currently wraps a single `rusqlite::Connection` in an `Arc<Mutex>`.
/// All queries run via `tokio::task::spawn_blocking` to avoid
/// blocking the async runtime.
///
/// The `Pool` is `Send + Sync` and can be freely shared across tasks.
/// It is cheaply clonable (Arc increment).
#[derive(Clone)]
pub struct Pool {
    inner: Arc<Mutex<Connection>>,
}

impl Pool {
    /// Opens a SQLite database at the given path.
    ///
    /// Creates the file if it doesn't exist.
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
        })
    }

    /// Executes a query that returns zero or one row.
    ///
    /// Returns `Ok(Some(T))` if a row was found, `Ok(None)` if no row matched.
    pub async fn query_one<T: FromRow + Send + 'static>(
        &self,
        sql: &str,
        params: Vec<Param>,
    ) -> Result<Option<T>> {
        let sql = sql.to_owned();
        let inner = self.inner.clone();

        tokio::task::spawn_blocking(move || {
            let conn = inner.lock().map_err(|_| {
                Error::custom("connection lock poisoned")
            })?;

            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query(
                rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            )?;

            match rows.next()? {
                Some(row) => Ok(Some(T::from_row(&Row::new(row))?)),
                None => Ok(None),
            }
        })
        .await
        .map_err(|e| Error::custom(format!("spawn_blocking panicked: {e}")))?
    }

    /// Executes a query that returns zero or more rows.
    ///
    /// Returns all matching rows as a `Vec<T>`.
    pub async fn query_all<T: FromRow + Send + 'static>(
        &self,
        sql: &str,
        params: Vec<Param>,
    ) -> Result<Vec<T>> {
        let sql = sql.to_owned();
        let inner = self.inner.clone();

        tokio::task::spawn_blocking(move || {
            let conn = inner.lock().map_err(|_| {
                Error::custom("connection lock poisoned")
            })?;

            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query(
                rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            )?;

            let mut results = Vec::new();
            while let Some(row) = rows.next()? {
                results.push(T::from_row(&Row::new(row))?);
            }

            Ok(results)
        })
        .await
        .map_err(|e| Error::custom(format!("spawn_blocking panicked: {e}")))?
    }
}
