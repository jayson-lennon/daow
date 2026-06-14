//! Tests for error_stack `Report<E>` return types on `#[dao]`-generated methods.
//!
//! Verifies that when a consumer declares `Result<T, Report<UnitErr>>` (or
//! `Result<T, Report<dao::Error>>`), the macro-emitted bodies correctly convert
//! `dao::Error` into the declared error type, preserving the original error in
//! the `Report` frame chain.

use dao::{async_trait, dao, ExecuteResult, Pool};
use error_stack::Report;

// --- Consumer-defined unit error type ---
#[derive(Debug)]
struct StoreError;
impl std::error::Error for StoreError {}
impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "store error")
    }
}

// --- Entity ---
#[derive(Debug, Clone, PartialEq, dao::Entity)]
#[dao(table = "widgets")]
struct Widget {
    #[dao(pk)]
    id: i64,
    name: String,
}

/// Returns `(pool, _tempdir)`. Creates a `widgets` table but NO `nonexistent_table`.
async fn setup_db() -> (Pool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();
    pool.execute(
        "CREATE TABLE widgets (id INTEGER PRIMARY KEY, name TEXT)",
        vec![],
    )
    .await
    .unwrap();
    (pool, dir)
}

// ===========================================================================
// DAO whose every method returns Report<StoreError> (unit error).
// ===========================================================================
#[dao]
#[async_trait]
#[allow(dead_code)] // update/delete exist to verify macro expansion compiles
trait WidgetDao {
    #[query("SELECT id, name FROM widgets WHERE id = ?")]
    async fn get_by_id(&self, id: i64) -> Result<Option<Widget>, Report<StoreError>>;

    // Bare query: query_one returns Error::custom when no rows — exercises wrap_tail on a
    // runtime error path for queries (compile-time SQL validation precludes nonexistent tables).
    #[query("SELECT id, name FROM widgets WHERE id = ?")]
    async fn get_required(&self, id: i64) -> Result<Widget, Report<StoreError>>;

    // Selects ONLY `id` but maps to Widget — from_row fails at runtime reading
    // `name`. Forces a dao::Error through the Option<T> query tail.
    #[query("SELECT id FROM widgets WHERE id = ?")]
    async fn get_id_only(&self, id: i64) -> Result<Option<Widget>, Report<StoreError>>;

    #[insert]
    async fn insert(&self, widget: Widget) -> Result<ExecuteResult, Report<StoreError>>;

    #[update]
    async fn update(&self, widget: Widget) -> Result<ExecuteResult, Report<StoreError>>;

    #[delete]
    async fn delete(&self, widget: Widget) -> Result<ExecuteResult, Report<StoreError>>;

    // Duplicate-PK insert to force a runtime #[execute] failure on a valid table.
    #[execute("INSERT INTO widgets (id, name) VALUES (?, ?)")]
    async fn insert_dup(&self, widget: Widget) -> Result<ExecuteResult, Report<StoreError>>;
}

// ===========================================================================
// DAO whose methods return Report<dao::Error> (the special-case slot).
// ===========================================================================
#[dao]
#[async_trait]
trait WidgetDaoDaoErr {
    #[insert]
    async fn insert(&self, widget: Widget) -> Result<ExecuteResult, Report<dao::Error>>;

    #[execute("INSERT INTO widgets (id, name) VALUES (?, ?)")]
    async fn insert_dup(&self, widget: Widget) -> Result<ExecuteResult, Report<dao::Error>>;
}

// ===========================================================================
// DAO whose methods return plain dao::Result (regression — today's behavior).
// ===========================================================================
#[dao]
#[async_trait]
trait WidgetDaoPlain {
    #[insert]
    async fn insert(&self, widget: Widget) -> dao::Result<ExecuteResult>;

    #[query("SELECT id, name FROM widgets WHERE id = ?")]
    async fn get_by_id(&self, id: i64) -> dao::Result<Option<Widget>>;
}

// --- BadRow: entity whose table does not exist. Used to force insert/update/delete
//     runtime failures. NOT subject to compile-time SQL validation (those methods use
//     entity-generated SQL), so it compiles and fails at runtime. ---
#[derive(Debug, Clone, PartialEq, dao::Entity)]
#[dao(table = "nonexistent_table")]
struct BadRow {
    #[dao(pk)]
    id: i64,
    name: String,
}

#[dao]
#[async_trait]
trait BadTableDao {
    #[insert]
    async fn insert(&self, row: BadRow) -> Result<ExecuteResult, Report<StoreError>>;

    #[update]
    async fn update(&self, row: BadRow) -> Result<ExecuteResult, Report<StoreError>>;

    #[delete]
    async fn delete(&self, row: BadRow) -> Result<ExecuteResult, Report<StoreError>>;
}

// --- helper: assert a Report<StoreError> carries a dao::Error in its frame chain ---
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

#[tokio::test]
async fn report_unit_error_query_failure() {
    let (pool, _dir) = setup_db().await;
    let dao = WidgetDao::new(pool);
    // Bare query for a non-existent row → query_one returns None → Error::custom at runtime.
    let err = dao.get_required(999).await.unwrap_err();
    assert_chain_reaches_dao(&err);
}

#[tokio::test]
async fn report_unit_error_option_query_failure() {
    // The Option<T> query path (ReturnKind::Option) must also wrap dao::Error into
    // Report<StoreError>. `get_id_only` selects only `id`; Widget::from_row then
    // fails at runtime reading `name` (Error::ColumnNotFound). This exercises the
    // Option<T> query tail through wrap_tail's Report(C) branch — the one combination
    // not previously covered on a failure path.
    let (pool, _dir) = setup_db().await;
    let dao = WidgetDao::new(pool);
    // Insert a row so the query returns Some(row) and from_row actually runs.
    dao.insert(Widget {
        id: 1,
        name: "x".to_string(),
    })
    .await
    .unwrap();
    let err = dao.get_id_only(1).await.unwrap_err();
    assert_chain_reaches_dao(&err);
}

#[tokio::test]
async fn report_unit_error_insert_failure() {
    let (pool, _dir) = setup_db().await;
    let bad = BadTableDao::new(pool);
    let err = bad
        .insert(BadRow {
            id: 1,
            name: "x".to_string(),
        })
        .await
        .unwrap_err();
    assert_chain_reaches_dao(&err);
}

#[tokio::test]
async fn report_unit_error_update_failure() {
    let (pool, _dir) = setup_db().await;
    let bad = BadTableDao::new(pool);
    let err = bad
        .update(BadRow {
            id: 1,
            name: "x".to_string(),
        })
        .await
        .unwrap_err();
    assert_chain_reaches_dao(&err);
}

#[tokio::test]
async fn report_unit_error_delete_failure() {
    let (pool, _dir) = setup_db().await;
    let bad = BadTableDao::new(pool);
    let err = bad
        .delete(BadRow {
            id: 1,
            name: "x".to_string(),
        })
        .await
        .unwrap_err();
    assert_chain_reaches_dao(&err);
}

#[tokio::test]
async fn report_unit_error_execute_failure() {
    let (pool, _dir) = setup_db().await;
    let dao = WidgetDao::new(pool);
    // First insert succeeds.
    dao.insert(Widget {
        id: 1,
        name: "first".to_string(),
    })
    .await
    .unwrap();
    // Second insert with same PK → uniqueness violation at runtime.
    let err = dao
        .insert_dup(Widget {
            id: 1,
            name: "dup".to_string(),
        })
        .await
        .unwrap_err();
    assert_chain_reaches_dao(&err);
}

// ===========================================================================
// Report<dao::Error> slot — special-case path (.map_err(Report::new)).
// ===========================================================================
#[tokio::test]
async fn report_dao_error_execute_failure() {
    let (pool, _dir) = setup_db().await;
    let dao = WidgetDaoDaoErr::new(pool);
    dao.insert(Widget {
        id: 1,
        name: "first".to_string(),
    })
    .await
    .unwrap();
    let err = dao
        .insert_dup(Widget {
            id: 1,
            name: "dup".to_string(),
        })
        .await
        .unwrap_err();
    assert!(
        err.downcast_ref::<dao::Error>().is_some(),
        "Report<dao::Error> top context should be dao::Error"
    );
}

// ===========================================================================
// Plain dao::Result regression — today's behavior unchanged.
// ===========================================================================
#[tokio::test]
async fn plain_dao_result_still_works() {
    let (pool, _dir) = setup_db().await;
    let dao = WidgetDaoPlain::new(pool);
    let widget = Widget {
        id: 1,
        name: "widget".to_string(),
    };
    let res = dao.insert(widget.clone()).await.unwrap();
    assert_eq!(res.rows_affected, 1);
    let fetched = dao.get_by_id(1).await.unwrap();
    assert_eq!(fetched, Some(widget));
}

// ===========================================================================
// Positive: Report<UnitErr> methods succeed when no error occurs.
// ===========================================================================
#[tokio::test]
async fn report_unit_error_success_path() {
    let (pool, _dir) = setup_db().await;
    let dao = WidgetDao::new(pool);
    let widget = Widget {
        id: 1,
        name: "widget".to_string(),
    };
    let res = dao.insert(widget).await.unwrap();
    assert_eq!(res.rows_affected, 1);
    let fetched = dao.get_by_id(1).await.unwrap();
    assert!(fetched.is_some());
}
