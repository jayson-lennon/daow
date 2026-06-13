//! Connection abstraction that unifies [`Pool`] and [`Transaction`] so that
//! generated DAO methods can run against either without code duplication.
//!
//! Both [`Pool`] and [`Transaction`] already expose identical
//! `query_one<T>` / `query_all<T>` / `execute` async signatures. Rather than
//! introduce an object-safe trait with a synchronous `block_on`-prone core (which
//! would deadlock when a `Pool` has to asynchronously acquire a permit), this
//! `Conn` enum simply delegates to whichever variant it holds. The generated DAO
//! methods call `self.conn.query_one::<T>(...)` and the dispatch is a single
//! `match` — no vtable, no raw-row re-parse.

use crate::entity_meta::ExecuteResult;
use crate::error::Result;
use crate::from_row::FromRow;
use crate::pool::{Param, Pool, Transaction};

/// Either a pooled connection or an in-flight transaction.
///
/// Constructed via [`Conn::from`] / `Into` from a [`Pool`] or [`Transaction`],
/// or built up by DAO code. The generated `#[dao]` structs store a `Conn` and
/// call its methods; `Pool` runs each statement as an autocommit, `Transaction`
/// runs it on the held connection inside the open transaction.
pub enum Conn {
    /// Autocommit: each statement checks out its own connection.
    Pool(Pool),
    /// Transactional: each statement runs on the held connection.
    Tx(Transaction),
}

impl From<Pool> for Conn {
    fn from(pool: Pool) -> Self {
        Conn::Pool(pool)
    }
}

impl From<Transaction> for Conn {
    fn from(tx: Transaction) -> Self {
        Conn::Tx(tx)
    }
}

impl Conn {
    /// Executes a query that returns zero or one row.
    pub async fn query_one<T: FromRow + Send + 'static>(
        &self,
        sql: &str,
        params: Vec<Param>,
    ) -> Result<Option<T>> {
        match self {
            Conn::Pool(pool) => pool.query_one(sql, params).await,
            Conn::Tx(tx) => tx.query_one(sql, params).await,
        }
    }

    /// Executes a query that returns zero or more rows.
    pub async fn query_all<T: FromRow + Send + 'static>(
        &self,
        sql: &str,
        params: Vec<Param>,
    ) -> Result<Vec<T>> {
        match self {
            Conn::Pool(pool) => pool.query_all(sql, params).await,
            Conn::Tx(tx) => tx.query_all(sql, params).await,
        }
    }

    /// Executes a write statement (INSERT/UPDATE/DELETE).
    pub async fn execute(&self, sql: &str, params: Vec<Param>) -> Result<ExecuteResult> {
        match self {
            Conn::Pool(pool) => pool.execute(sql, params).await,
            Conn::Tx(tx) => tx.execute(sql, params).await,
        }
    }
}
