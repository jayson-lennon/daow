use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::timeout;

use crate::entity_meta::ExecuteResult;
use crate::error::{Error, Result};
use crate::from_row::FromRow;
use crate::row::Row;

/// Type-erased parameter that can be sent across threads.
pub type Param = Box<dyn rusqlite::types::ToSql + Send>;

/// Default bound on the number of concurrent checked-out connections.
const DEFAULT_MAX_SIZE: usize = 4;

/// Default deadline for acquiring a checked-out connection.
const DEFAULT_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

/// Immutable configuration shared by every clone of a [`Pool`].
struct PoolConfig {
    path: String,
    max_size: usize,
    acquire_timeout: Duration,
    pragmas: Vec<(String, String)>,
}

/// Shared interior of a [`Pool`]: a bounded set of permits plus an idle-conn stack.
///
/// Permits (`sem`) bound the number of connections *in use* at once. Idle connections
/// (not currently checked out) live in `idle` and do **not** hold a permit. A checkout
/// is: acquire a permit → pop an idle conn (or open a fresh one) → return a [`PooledConn`]
/// guard. The guard releases its permit and pushes its conn back to idle on `Drop`.
struct PoolState {
    config: PoolConfig,
    sem: Arc<Semaphore>,
    idle: Arc<Mutex<VecDeque<Connection>>>,
}

/// A bounded pool of SQLite connections backed by `tokio::sync::Semaphore`.
///
/// The `Pool` is `Send + Sync`, cheaply clonable (Arc increment), and issues all SQL via
/// `tokio::task::spawn_blocking` so blocking work never stalls the async runtime. Every
/// freshly-opened connection has the builder's pragmas applied (e.g. `foreign_keys=ON`),
/// which is required for `ON DELETE CASCADE` to function.
#[derive(Clone)]
pub struct Pool {
    inner: Arc<PoolState>,
}

/// Builder for [`Pool`].
pub struct PoolBuilder {
    path: Option<String>,
    max_size: usize,
    acquire_timeout: Duration,
    pragmas: Vec<(String, String)>,
}

impl PoolBuilder {
    /// Start a new builder with defaults (`max_size = 4`, `acquire_timeout = 5s`, no pragmas).
    pub fn new() -> Self {
        Self {
            path: None,
            max_size: DEFAULT_MAX_SIZE,
            acquire_timeout: DEFAULT_ACQUIRE_TIMEOUT,
            pragmas: Vec::new(),
        }
    }

    /// The database file path. Required — `build()` errors without it.
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Maximum number of connections that may be checked out concurrently.
    pub fn max_size(mut self, max_size: usize) -> Self {
        self.max_size = max_size;
        self
    }

    /// How long to wait for an available connection before erroring with
    /// [`Error::AcquireTimeout`](crate::Error::AcquireTimeout).
    pub fn acquire_timeout(mut self, acquire_timeout: Duration) -> Self {
        self.acquire_timeout = acquire_timeout;
        self
    }

    /// Append a `PRAGMA <key> = <value>` applied to every freshly-opened connection.
    ///
    /// Values are interpolated as raw SQL (the values originate from the application, not
    /// user input), e.g. `.pragma("foreign_keys", "ON")` → `PRAGMA foreign_keys = ON`.
    pub fn pragma(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.pragmas.push((key.into(), value.into()));
        self
    }

    /// Finalize the builder. Opens one connection eagerly so that (a) the database file is
    /// created, (b) an invalid path fails immediately, and (c) pragmas are applied once up
    /// front. The eager connection sits in the idle stack (no permit held).
    pub fn build(self) -> Result<Pool> {
        let path = self
            .path
            .ok_or_else(|| Error::custom("PoolBuilder requires a path (call .path(...) first)"))?;
        let config = PoolConfig {
            path,
            max_size: self.max_size,
            acquire_timeout: self.acquire_timeout,
            pragmas: self.pragmas,
        };
        let idle = Arc::new(Mutex::new(VecDeque::new()));
        let sem = Arc::new(Semaphore::new(config.max_size));
        let pool = Pool {
            inner: Arc::new(PoolState { config, sem, idle }),
        };
        let conn = pool.create_connection()?;
        pool.inner
            .idle
            .lock()
            .map_err(|_| Error::custom("idle lock poisoned"))?
            .push_back(conn);
        Ok(pool)
    }
}

impl Default for PoolBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Pool {
    /// Start a [`PoolBuilder`].
    pub fn builder() -> PoolBuilder {
        PoolBuilder::new()
    }

    /// Opens a SQLite database at the given path, creating the file if it doesn't exist.
    ///
    /// Equivalent to `Pool::builder().path(path).build()` (defaults: `max_size = 4`,
    /// `acquire_timeout = 5s`, no pragmas).
    pub fn open(path: &str) -> Result<Self> {
        Self::builder().path(path).build()
    }

    /// Open a fresh connection, applying all configured pragmas (the gap-#3 fix).
    fn create_connection(&self) -> Result<Connection> {
        let conn = Connection::open(&self.inner.config.path)?;
        for (key, value) in &self.inner.config.pragmas {
            // Raw interpolation: values are keyword-like ("ON", "WAL") not string literals,
            // and originate from the application developer (same trust level as execute()).
            conn.execute_batch(&format!("PRAGMA {key} = {value}"))?;
        }
        Ok(conn)
    }

    /// Acquire a connection within the configured timeout. The returned guard owns a permit
    /// and the connection; dropping it returns the connection to the idle stack and releases
    /// the permit.
    async fn acquire(&self) -> Result<PooledConn> {
        let permit = match timeout(
            self.inner.config.acquire_timeout,
            self.inner.sem.clone().acquire_owned(),
        )
        .await
        {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => return Err(Error::custom("pool semaphore closed")),
            Err(_) => return Err(Error::AcquireTimeout),
        };
        let conn = match self
            .inner
            .idle
            .lock()
            .map_err(|_| Error::custom("idle lock poisoned"))?
            .pop_front()
        {
            Some(conn) => conn,
            None => self.create_connection()?,
        };
        Ok(PooledConn {
            permit: Some(permit),
            conn: Some(conn),
            idle: self.inner.idle.clone(),
        })
    }

    /// Hold one connection for the lifetime of `f`, which runs in a single
    /// `spawn_blocking` task.
    ///
    /// This is the escape hatch for multi-statement blocks that must run on a *single*
    /// connection with interleaved reads and writes: `f` gets a `&mut rusqlite::Connection`
    /// and may call `conn.transaction(|tx| …)` directly, or run a sequence of statements.
    ///
    /// **Does not begin a transaction itself** — `f` owns its own transaction if it wants one.
    /// This is deliberate: some operations (notably the migration runner) must toggle
    /// `PRAGMA foreign_keys = OFF` on the bare connection, and pragmas are a **no-op inside
    /// an active transaction**. Use [`Pool::begin`](Self::begin) instead for an async
    /// transaction handle.
    pub async fn with_conn<F, R>(&self, f: F) -> Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&mut rusqlite::Connection) -> Result<R> + Send + 'static,
    {
        let mut conn = self.acquire().await?;
        tokio::task::spawn_blocking(move || f(&mut *conn))
            .await
            .map_err(|e| Error::custom(format!("spawn_blocking panicked: {e}")))?
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
        let conn = self.acquire().await?;

        tokio::task::spawn_blocking(move || {
            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query(rusqlite::params_from_iter(
                params.iter().map(|p| p.as_ref()),
            ))?;

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
        let conn = self.acquire().await?;

        tokio::task::spawn_blocking(move || {
            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query(rusqlite::params_from_iter(
                params.iter().map(|p| p.as_ref()),
            ))?;

            let mut results = Vec::new();
            while let Some(row) = rows.next()? {
                results.push(T::from_row(&Row::new(row))?);
            }

            Ok(results)
        })
        .await
        .map_err(|e| Error::custom(format!("spawn_blocking panicked: {e}")))?
    }

    /// Executes a write statement (INSERT, UPDATE, DELETE) and returns
    /// the number of affected rows and the last insert rowid.
    pub async fn execute(&self, sql: &str, params: Vec<Param>) -> Result<ExecuteResult> {
        let sql = sql.to_owned();
        let conn = self.acquire().await?;

        tokio::task::spawn_blocking(move || {
            let rows_affected = conn.execute(
                &sql,
                rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            )?;

            Ok(ExecuteResult {
                rows_affected: rows_affected as u64,
                last_insert_rowid: conn.last_insert_rowid(),
            })
        })
        .await
        .map_err(|e| Error::custom(format!("spawn_blocking panicked: {e}")))?
    }

    /// Begin a transaction. Checks out one connection, issues `BEGIN`, and returns an
    /// async [`Transaction`] that holds that connection across `.await` points.
    ///
    /// Statements run on the *same* connection via [`Transaction::query_one`] /
    /// [`Transaction::query_all`] / [`Transaction::execute`]. Call [`Transaction::commit`]
    /// to persist; dropping the transaction without committing rolls it back.
    ///
    /// Statements within a transaction must be `await`ed **sequentially** (never
    /// `tokio::join!`'d) — they share one connection. This is correct for SQLite, which
    /// has a single writer anyway.
    pub async fn begin(&self) -> Result<Transaction> {
        let conn = self.acquire().await?;
        let conn = tokio::task::spawn_blocking(move || {
            // execute_batch takes &self; no mut needed.
            conn.execute_batch("BEGIN")?;
            Ok::<PooledConn, Error>(conn)
        })
        .await
        .map_err(|e| Error::custom(format!("spawn_blocking panicked: {e}")))??;

        Ok(Transaction {
            inner: Arc::new(Mutex::new(TransactionInner {
                conn: Some(conn),
                committed: false,
            })),
        })
    }
}

/// RAII guard for a checked-out connection. Owns a permit and the connection; `Drop`
/// pushes the connection back onto the idle stack and releases the permit.
///
/// This is `Send` so it can be moved into a `spawn_blocking` task, which is how every
/// statement is executed. The connection returns to the pool when the task (and thus the
/// guard) completes — even on panic.
struct PooledConn {
    permit: Option<OwnedSemaphorePermit>,
    conn: Option<Connection>,
    idle: Arc<Mutex<VecDeque<Connection>>>,
}

impl Deref for PooledConn {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        self.conn
            .as_ref()
            .expect("PooledConn conn accessed after Drop")
    }
}

impl DerefMut for PooledConn {
    fn deref_mut(&mut self) -> &mut Connection {
        self.conn
            .as_mut()
            .expect("PooledConn conn accessed after Drop")
    }
}

impl Drop for PooledConn {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            // Best-effort: if the idle lock is poisoned we drop the connection instead of
            // returning it, which simply shrinks the pool (next acquire opens a fresh one).
            if let Ok(mut idle) = self.idle.lock() {
                idle.push_back(conn);
            }
        }
        // Releasing the permit happens implicitly when the field drops, but dropping it
        // explicitly documents intent and matches the "return-on-Drop" invariant.
        drop(self.permit.take());
    }
}

/// Interior of a [`Transaction`]: the held connection and whether it has been committed.
struct TransactionInner {
    conn: Option<PooledConn>,
    committed: bool,
}

/// A database transaction holding a single checked-out connection across `.await` points.
///
/// Created by [`Pool::begin`]. The connection stays checked out (a pool permit is held)
/// for the lifetime of this value. [`commit`](Transaction::commit) ends the transaction
/// and returns the connection; dropping an uncommitted transaction rolls it back. In
/// either case the connection returns to the pool.
///
/// A `Transaction` is cheap to clone (`Arc` bump) and clonable handles share one
/// underlying connection — but statements must still be `await`ed sequentially.
#[derive(Clone)]
pub struct Transaction {
    inner: Arc<Mutex<TransactionInner>>,
}

impl Transaction {
    /// Run a statement on the held connection, returning its raw result. This is the
    /// shared primitive: lock → take the conn out → `spawn_blocking` → put it back. The
    /// mutex is held only for the brief `Option` swap, **not** across the await — so
    /// sequential awaits are cheap and a panicked statement returns the conn cleanly.
    async fn run_stmt<F, R>(&self, f: F) -> Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&mut rusqlite::Connection) -> Result<R> + Send + 'static,
    {
        let conn = {
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| Error::custom("transaction lock poisoned"))?;
            guard.conn.take().ok_or_else(|| Error::custom("transaction connection missing"))?
        };
        let (result, conn) = tokio::task::spawn_blocking(move || {
            let mut conn = conn;
            let result = f(&mut *conn);
            (result, conn)
        })
        .await
        .map_err(|e| Error::custom(format!("spawn_blocking panicked: {e}")))?;

        // Put the conn back (best-effort).
        if let Ok(mut guard) = self.inner.lock() {
            guard.conn = Some(conn);
        }
        result
    }

    /// Executes a query that returns zero or one row, on the transaction's connection.
    pub async fn query_one<T: FromRow + Send + 'static>(
        &self,
        sql: &str,
        params: Vec<Param>,
    ) -> Result<Option<T>> {
        let sql = sql.to_owned();
        self.run_stmt(move |conn| {
            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query(rusqlite::params_from_iter(
                params.iter().map(|p| p.as_ref()),
            ))?;
            match rows.next()? {
                Some(row) => Ok(Some(T::from_row(&Row::new(row))?)),
                None => Ok(None),
            }
        })
        .await
    }

    /// Executes a query that returns zero or more rows, on the transaction's connection.
    pub async fn query_all<T: FromRow + Send + 'static>(
        &self,
        sql: &str,
        params: Vec<Param>,
    ) -> Result<Vec<T>> {
        let sql = sql.to_owned();
        self.run_stmt(move |conn| {
            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query(rusqlite::params_from_iter(
                params.iter().map(|p| p.as_ref()),
            ))?;
            let mut results = Vec::new();
            while let Some(row) = rows.next()? {
                results.push(T::from_row(&Row::new(row))?);
            }
            Ok(results)
        })
        .await
    }

    /// Executes a write statement (INSERT, UPDATE, DELETE) on the transaction's connection.
    pub async fn execute(&self, sql: &str, params: Vec<Param>) -> Result<ExecuteResult> {
        let sql = sql.to_owned();
        self.run_stmt(move |conn| {
            let rows_affected = conn.execute(
                &sql,
                rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            )?;
            Ok(ExecuteResult {
                rows_affected: rows_affected as u64,
                last_insert_rowid: conn.last_insert_rowid(),
            })
        })
        .await
    }

    /// Commit the transaction. Consumes `self` so it cannot run while a `.with(&self)` view
    /// (see Phase 3) borrows the transaction — the borrow checker rejects that.
    pub async fn commit(self) -> Result<()> {
        // Take the conn out and flip the committed flag under a *brief* lock; do not hold
        // the mutex across the spawn_blocking await.
        let conn = {
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| Error::custom("transaction lock poisoned"))?;
            guard.committed = true;
            guard
                .conn
                .take()
                .ok_or_else(|| Error::custom("transaction connection missing"))?
        };
        // Run COMMIT on the blocking pool. conn drops here → returned to idle by PooledConn::Drop.
        tokio::task::spawn_blocking(move || {
            conn.execute_batch("COMMIT")?;
            Ok::<(), Error>(())
        })
        .await
        .map_err(|e| Error::custom(format!("spawn_blocking panicked: {e}")))??;
        Ok(())
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        // Only roll back if not committed and we still hold the connection.
        let conn = {
            let Ok(mut guard) = self.inner.lock() else {
                return;
            };
            if guard.committed {
                return;
            }
            guard.conn.take()
        };
        if let Some(conn) = conn {
            // Best-effort explicit ROLLBACK. We CANNOT rely on rusqlite's auto-rollback-on-
            // Drop here: the underlying Connection is returned to the pool's idle stack by
            // PooledConn::Drop (it is NOT closed), so auto-rollback never fires. A dangling
            // open BEGIN would leak into the next checkout. Run ROLLBACK explicitly; if it
            // fails (conn in a bad state) we still return the conn — closing it later or
            // the next use will surface the problem.
            let _ = conn.execute_batch("ROLLBACK");
            // conn (PooledConn) drops here → returned to idle by PooledConn::Drop.
        }
    }
}
