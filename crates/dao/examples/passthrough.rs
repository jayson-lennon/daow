//! Pass-through method bodies — hand-written bodies inside a `#[dao]` trait that
//! the macro forwards verbatim into both the DAO and its `.with(&tx)` view.
//!
//! This is the escape hatch for raw SQL the annotations can't express (PRAGMAs,
//! DDL, complex joins). A pass-through method has a body and NO annotation; the
//! macro forwards the body unchanged and `self.query_one/query_all/execute`
//! resolve to inherent helpers that delegate to the underlying connection.
//!
//! Run with: cargo run --example passthrough

use dao::{async_trait, dao, Entity, ExecuteResult, Pool, Result};

#[derive(Debug, Clone, Entity)]
#[dao(table = "widgets")]
struct Widget {
    #[dao(pk)]
    id: i64,
    name: String,
}

/// Row shape for `PRAGMA wal_checkpoint(TRUNCATE)`: (busy, log, checkpointed).
#[derive(Debug, Clone)]
struct CheckpointResult {
    busy: i64,
    log: i64,
    checkpointed: i64,
}

impl dao::FromRow for CheckpointResult {
    fn from_row(row: &dao::Row) -> Result<Self> {
        use dao::FromSqlColumn;
        Ok(CheckpointResult {
            busy: i64::from_column(&row.get_column(0)?)?,
            log: i64::from_column(&row.get_column(1)?)?,
            checkpointed: i64::from_column(&row.get_column(2)?)?,
        })
    }
}

/// A typed DAO where most methods are generated, but `checkpoint()` is
/// hand-written (pass-through) because `PRAGMA wal_checkpoint(TRUNCATE)` isn't
/// a SELECT/INSERT/UPDATE/DELETE-shaped statement.
#[dao]
#[async_trait]
trait WidgetDao {
    #[query("SELECT id, name FROM widgets WHERE id = ?")]
    async fn get(&self, id: i64) -> Result<Option<Widget>>;

    #[insert]
    async fn insert(&self, widget: Widget) -> Result<ExecuteResult>;

    /// Pass-through: hand-written body, no annotation. `self.query_one` resolves
    /// to the inherent helper that delegates to the underlying connection.
    async fn checkpoint(&self) -> Result<Option<CheckpointResult>> {
        // PRAGMA wal_checkpoint(TRUNCATE) returns (busy, log, checkpointed).
        // NOTE: checkpoint needs exclusive access — it can NOT run inside an
        // active transaction (SQLite returns SQLITE_LOCKED). Use it on a pool-backed
        // DAO, not via `.with(&tx)`.
        self.query_one::<CheckpointResult>("PRAGMA wal_checkpoint(TRUNCATE)", vec![])
            .await
    }

    /// Pass-through: a plain SELECT expressed by hand. This one IS transaction-safe,
    /// so it works both on the pool and via `.with(&tx)`.
    async fn count(&self) -> Result<i64> {
        self.query_one::<i64>("SELECT COUNT(*) FROM widgets", vec![])
            .await
            .map(|opt| opt.unwrap_or(0))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let pool = Pool::open(":memory:")?;
    pool.execute(
        "CREATE TABLE widgets (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
        vec![],
    )
    .await?;

    let dao = WidgetDao::new(pool.clone());

    // 1. Generated method: insert + get.
    dao.insert(Widget {
        id: 1,
        name: "gadget".into(),
    })
    .await?;
    let fetched = dao.get(1).await?.expect("widget should exist");
    assert_eq!(fetched.name, "gadget");
    println!(
        "[pool] generated insert+get OK (id=1, name={})",
        fetched.name
    );

    // 2. Pass-through against the pool: checkpoint observable.
    let res = dao
        .checkpoint()
        .await?
        .expect("checkpoint should return a row");
    println!(
        "[pool] pass-through checkpoint OK (busy={}, log={}, checkpointed={})",
        res.busy, res.log, res.checkpointed
    );

    // 3. The same hand-written body runs inside a transaction via .with(&tx).
    //    (Using the transaction-safe `count`; `checkpoint` would deadlock here.)
    let tx = pool.begin().await?;
    dao.with(&tx)
        .insert(Widget {
            id: 2,
            name: "gizmo".into(),
        })
        .await?;
    let in_tx = dao.with(&tx).count().await?;
    assert_eq!(in_tx, 2, "inside tx: two widgets visible");
    println!(
        "[tx]   pass-through count OK ({} widgets visible inside tx)",
        in_tx
    );
    tx.commit().await?;

    // 4. After commit, the pool sees the tx's writes too.
    let after = dao.count().await?;
    assert_eq!(after, 2, "after commit: pool sees both rows");
    println!("[pool] post-commit count OK ({} widgets total)", after);

    println!("\nAll pass-through scenarios passed.");
    Ok(())
}
