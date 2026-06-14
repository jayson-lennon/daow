//! Integration tests for the `#[upsert]` annotation.
//!
//! Covers:
//! - insert path (absent row), update path (existing PK), identity preservation (rowid),
//! - FK cascade safety (no DELETE triggers fire), transaction rollback,
//! - `Result<_, Report<UnitErr>>` error wrapping on the upsert path,
//! - all-PK entities (junction tables) emit `ON CONFLICT(pk) DO NOTHING`.

use dao::{async_trait, dao, Entity, EntityMeta, ExecuteResult, Pool, Result};
use error_stack::Report;

// ===========================================================================
// Entities
// ===========================================================================

#[derive(Debug, Clone, PartialEq, Entity)]
#[dao(table = "widgets")]
pub struct Widget {
    #[dao(pk)]
    pub id: i64,
    pub name: String,
}

/// Child row with FK ON DELETE CASCADE — used to verify upsert does NOT fire
/// DELETE triggers (REPLACE/INSERT OR REPLACE would cascade-delete these).
#[derive(Debug, Clone, PartialEq, Entity)]
#[dao(table = "children")]
pub struct Child {
    #[dao(pk)]
    pub id: i64,
    pub parent_id: i64,
}

/// All-PK junction table — exercises the DO NOTHING upsert fallback.
#[derive(Debug, Clone, PartialEq, Entity)]
#[dao(table = "junctions")]
pub struct Junction {
    #[dao(pk)]
    pub a: i64,
    #[dao(pk)]
    pub b: i64,
}

/// Points at a table that does not exist so entity-generated SQL fails at runtime.
#[derive(Debug, Clone, PartialEq, Entity)]
#[dao(table = "nonexistent_table")]
pub struct BadRow {
    #[dao(pk)]
    pub id: i64,
    pub name: String,
}

// ===========================================================================
// Unit error for Report<C> coverage
// ===========================================================================

#[derive(Debug)]
pub struct StoreError;
impl std::error::Error for StoreError {}
impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "store error")
    }
}

// ===========================================================================
// DAO
// ===========================================================================

#[dao]
#[async_trait]
trait WidgetDao {
    #[query("SELECT id, name FROM widgets WHERE id = ?")]
    async fn get(&self, id: i64) -> Result<Option<Widget>>;

    #[insert]
    async fn insert(&self, widget: Widget) -> Result<ExecuteResult>;

    #[upsert]
    async fn upsert(&self, widget: Widget) -> Result<ExecuteResult>;

    /// Upsert path with a `Report<StoreError>` error slot — exercises Phase 1
    /// error conversion on the new method kind.
    #[upsert]
    async fn upsert_reported(
        &self,
        widget: Widget,
    ) -> std::result::Result<ExecuteResult, Report<StoreError>>;

    /// All-PK junction upsert — exercises the DO NOTHING fallback.
    #[upsert]
    async fn upsert_junction(&self, j: Junction) -> Result<ExecuteResult>;

    #[query("SELECT COUNT(*) FROM children WHERE parent_id = ?")]
    async fn count_children(&self, parent_id: i64) -> Result<i64>;
}

#[dao]
#[async_trait]
trait BadTableDao {
    #[upsert]
    async fn upsert(&self, row: BadRow) -> std::result::Result<ExecuteResult, Report<StoreError>>;
}

// ===========================================================================
// Helpers
// ===========================================================================

async fn fresh() -> (Pool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();
    pool.execute(
        "CREATE TABLE widgets (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
        vec![],
    )
    .await
    .unwrap();
    pool.execute(
        "CREATE TABLE children (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER NOT NULL,
            FOREIGN KEY (parent_id) REFERENCES widgets(id) ON DELETE CASCADE
        )",
        vec![],
    )
    .await
    .unwrap();
    pool.execute(
        "CREATE TABLE junctions (a INTEGER NOT NULL, b INTEGER NOT NULL, PRIMARY KEY (a, b))",
        vec![],
    )
    .await
    .unwrap();
    (pool, dir)
}

fn assert_chain_reaches_dao(report: &Report<StoreError>) {
    assert!(
        report.downcast_ref::<StoreError>().is_some(),
        "top context should be StoreError"
    );
    let reaches_dao = report
        .frames()
        .any(|f| f.downcast_ref::<dao::Error>().is_some());
    assert!(
        reaches_dao,
        "frame chain should contain the original dao::Error"
    );
}

// ===========================================================================
// Tests
// ===========================================================================

#[tokio::test]
async fn upsert_inserts_absent_row() {
    let (pool, _dir) = fresh().await;
    let dao = WidgetDao::new(pool);
    let res = dao
        .upsert(Widget {
            id: 1,
            name: "alpha".into(),
        })
        .await
        .unwrap();
    assert_eq!(res.rows_affected, 1, "absent row should be inserted");
    let got = dao.get(1).await.unwrap().unwrap();
    assert_eq!(got.name, "alpha");
}

#[tokio::test]
async fn upsert_updates_existing_pk() {
    let (pool, _dir) = fresh().await;
    let dao = WidgetDao::new(pool);
    dao.insert(Widget {
        id: 1,
        name: "alpha".into(),
    })
    .await
    .unwrap();
    dao.upsert(Widget {
        id: 1,
        name: "beta".into(),
    })
    .await
    .unwrap();
    let got = dao.get(1).await.unwrap().unwrap();
    assert_eq!(got.name, "beta", "non-PK column updated on existing PK");
}

#[tokio::test]
async fn upsert_preserves_identity_under_fk() {
    let (pool, _dir) = fresh().await;
    let dao = WidgetDao::new(pool.clone());
    dao.insert(Widget {
        id: 1,
        name: "alpha".into(),
    })
    .await
    .unwrap();
    pool.execute(
        "INSERT INTO children (id, parent_id) VALUES (?, ?)",
        vec![Box::new(100i64) as dao::Param, Box::new(1i64) as dao::Param],
    )
    .await
    .unwrap();
    assert_eq!(dao.count_children(1).await.unwrap(), 1);

    // Upsert the parent with a new name. With INSERT OR REPLACE this would
    // DELETE the parent (firing the CASCADE) and orphan the child. With
    // ON CONFLICT DO UPDATE the row identity is preserved.
    dao.upsert(Widget {
        id: 1,
        name: "beta".into(),
    })
    .await
    .unwrap();

    assert_eq!(
        dao.count_children(1).await.unwrap(),
        1,
        "child must survive parent upsert (no cascade delete)"
    );
    let got = dao.get(1).await.unwrap().unwrap();
    assert_eq!(got.name, "beta");
}

#[tokio::test]
async fn upsert_in_transaction_rolls_back_on_drop() {
    // A transaction that is begun, written to, then dropped (not committed) must
    // roll back. We assert the upsert did not persist.
    let (pool, _dir) = fresh().await;
    let dao = WidgetDao::new(pool.clone());

    {
        let tx = pool.begin().await.unwrap();
        dao.with(&tx)
            .upsert(Widget {
                id: 1,
                name: "alpha".into(),
            })
            .await
            .unwrap();
        // tx dropped here without commit → rollback
    }

    let got = dao.get(1).await.unwrap();
    assert!(
        got.is_none(),
        "upsert must not persist after an uncommitted (dropped) transaction"
    );
}

#[tokio::test]
async fn upsert_report_error_on_runtime_failure() {
    // BadRow points at a nonexistent table → the upsert SQL fails at runtime,
    // and the macro-generated body must wrap it into Report<StoreError>.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("bad.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();
    let bad = BadTableDao::new(pool);
    let err = bad
        .upsert(BadRow {
            id: 1,
            name: "x".to_string(),
        })
        .await
        .unwrap_err();
    assert_chain_reaches_dao(&err);
}

#[tokio::test]
async fn upsert_reported_success_path() {
    // The Report<C> variant must also succeed cleanly on the happy path.
    let (pool, _dir) = fresh().await;
    let dao = WidgetDao::new(pool);
    let res = dao
        .upsert_reported(Widget {
            id: 1,
            name: "alpha".into(),
        })
        .await
        .unwrap();
    assert_eq!(res.rows_affected, 1);
}

#[tokio::test]
async fn upsert_all_pk_emits_do_nothing_and_is_idempotent() {
    // Verify the generated SQL for an all-PK entity contains DO NOTHING.
    let sql = <Junction as EntityMeta>::upsert_sql();
    assert!(
        sql.contains("ON CONFLICT(a, b) DO NOTHING"),
        "all-PK upsert SQL should be DO NOTHING, got: {}",
        sql
    );

    let (pool, _dir) = fresh().await;
    let dao = WidgetDao::new(pool);

    // First upsert inserts the junction row.
    let r1 = dao.upsert_junction(Junction { a: 1, b: 2 }).await.unwrap();
    assert_eq!(r1.rows_affected, 1, "first upsert inserts");

    // Second upsert with same PK pair must be a no-op (DO NOTHING).
    let r2 = dao.upsert_junction(Junction { a: 1, b: 2 }).await.unwrap();
    assert_eq!(
        r2.rows_affected, 0,
        "second upsert on same PK must be a no-op (DO NOTHING)"
    );
}

#[tokio::test]
async fn upsert_sql_shape_is_correct() {
    let sql = <Widget as EntityMeta>::upsert_sql();
    assert!(
        sql.contains("INSERT INTO widgets"),
        "missing INSERT INTO widgets: {}",
        sql
    );
    assert!(
        sql.contains("ON CONFLICT(id) DO UPDATE SET name = excluded.name"),
        "missing ON CONFLICT DO UPDATE SET clause: {}",
        sql
    );
}
