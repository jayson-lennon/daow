//! Integration tests for Phase 2: `Pool::with_conn` and `Pool::begin` / `Transaction`.
//!
//! Covers: held-connection primitives, commit persistence, rollback-on-drop, mid-tx
//! failure atomicity, and that a failed closure does not corrupt subsequent pool checkouts.

use dao::{ExecuteResult, Pool};

/// Schema with a small table and a uniqueness constraint so we can force a mid-tx failure.
const SCHEMA: &str = "
CREATE TABLE accounts (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);
";

async fn fresh_pool(test: &str) -> Pool {
    // Use a temp file so each test is isolated. The `test` name keeps them apart.
    let dir = std::env::temp_dir().join(format!("dao_tx_{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("db.sqlite").to_string_lossy().to_string();
    let pool = Pool::builder()
        .path(&path)
        .max_size(4)
        .pragma("foreign_keys", "ON")
        .build()
        .unwrap();
    pool.execute(SCHEMA, vec![]).await.unwrap();
    pool
}

#[tokio::test]
async fn with_conn_runs_multi_statement_block_on_one_connection() {
    let pool = fresh_pool("with_conn_block").await;
    // Two inserts inside a manual transaction on the held connection.
    pool.with_conn(|conn| {
        let tx = conn.transaction()?;
        tx.execute("INSERT INTO accounts (id, name) VALUES (1, 'alice')", [])?;
        tx.execute("INSERT INTO accounts (id, name) VALUES (2, 'bob')", [])?;
        tx.commit()?;
        Ok::<_, dao::Error>(())
    })
    .await
    .unwrap();

    let count: Option<i64> = pool
        .query_one("SELECT COUNT(*) FROM accounts", vec![])
        .await
        .unwrap();
    assert_eq!(count, Some(2));
}

#[tokio::test]
async fn with_conn_closure_failure_does_not_corrupt_pool() {
    let pool = fresh_pool("with_conn_err").await;
    // Closure returns Err; the connection must still be returned to the pool cleanly.
    let err = pool
        .with_conn(|_conn| {
            Err::<(), dao::Error>(dao::Error::custom("synthetic closure failure"))
        })
        .await
        .unwrap_err();
    assert!(err.to_string().contains("synthetic closure failure"));

    // Pool must still work.
    pool.execute("INSERT INTO accounts (id, name) VALUES (1, 'alice')", vec![])
        .await
        .unwrap();
    let got: Option<String> = pool
        .query_one("SELECT name FROM accounts WHERE id = 1", vec![])
        .await
        .unwrap();
    assert_eq!(got.as_deref(), Some("alice"));
}

#[tokio::test]
async fn begin_commit_persists_rows() {
    let pool = fresh_pool("commit").await;
    let tx = pool.begin().await.unwrap();
    let res: ExecuteResult = tx
        .execute(
            "INSERT INTO accounts (id, name) VALUES (1, 'alice')",
            vec![],
        )
        .await
        .unwrap();
    assert_eq!(res.rows_affected, 1);
    tx.commit().await.unwrap();

    let got: Option<String> = pool
        .query_one("SELECT name FROM accounts WHERE id = 1", vec![])
        .await
        .unwrap();
    assert_eq!(got.as_deref(), Some("alice"));
}

#[tokio::test]
async fn begin_drop_without_commit_reverts_row() {
    let pool = fresh_pool("rollback_drop").await;
    {
        let tx = pool.begin().await.unwrap();
        tx.execute(
            "INSERT INTO accounts (id, name) VALUES (1, 'alice')",
            vec![],
        )
        .await
        .unwrap();
        // Drop without commit.
    }

    let count: Option<i64> = pool
        .query_one("SELECT COUNT(*) FROM accounts", vec![])
        .await
        .unwrap();
    assert_eq!(count, Some(0));
}

#[tokio::test]
async fn begin_drop_without_commit_reverts_row_to_idle_clean() {
    // Follow-up: the rolled-back connection must be reusable (no dangling BEGIN).
    let pool = fresh_pool("rollback_reuse").await;
    {
        let tx = pool.begin().await.unwrap();
        tx.execute(
            "INSERT INTO accounts (id, name) VALUES (1, 'alice')",
            vec![],
        )
        .await
        .unwrap();
    }

    // Force a second checkout. With max_size=1 this reuses the rolled-back conn.
    // We need a 1-size pool for this to be deterministic.
    let dir = std::env::temp_dir().join("dao_tx_rollback_reuse1");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("db.sqlite").to_string_lossy().to_string();
    let pool1 = Pool::builder().path(&path).max_size(1).build().unwrap();
    pool1.execute(SCHEMA, vec![]).await.unwrap();
    {
        let tx = pool1.begin().await.unwrap();
        tx.execute(
            "INSERT INTO accounts (id, name) VALUES (1, 'alice')",
            vec![],
        )
        .await
        .unwrap();
    }
    // Reuse the same connection: it must not be inside a transaction.
    let res: ExecuteResult = pool1
        .execute(
            "INSERT INTO accounts (id, name) VALUES (2, 'bob')",
            vec![],
        )
        .await
        .unwrap();
    assert_eq!(res.rows_affected, 1);
    let count: Option<i64> = pool1
        .query_one("SELECT COUNT(*) FROM accounts", vec![])
        .await
        .unwrap();
    // alice was rolled back; only bob remains.
    assert_eq!(count, Some(1));
}

#[tokio::test]
async fn mid_tx_failure_reverts_all_changes() {
    let pool = fresh_pool("mid_fail").await;
    // Seed one row.
    pool.execute(
        "INSERT INTO accounts (id, name) VALUES (1, 'alice')",
        vec![],
    )
    .await
    .unwrap();

    let tx = pool.begin().await.unwrap();
    tx.execute(
        "INSERT INTO accounts (id, name) VALUES (2, 'bob')",
        vec![],
    )
    .await
    .unwrap();
    // Duplicate name 'alice' violates the UNIQUE constraint -> statement error.
    let err = tx
        .execute(
            "INSERT INTO accounts (id, name) VALUES (3, 'alice')",
            vec![],
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("UNIQUE") || err.to_string().contains("unique"));
    // Drop without commit -> both inserts revert.
    drop(tx);

    let count: Option<i64> = pool
        .query_one("SELECT COUNT(*) FROM accounts", vec![])
        .await
        .unwrap();
    // Only the seeded alice remains.
    assert_eq!(count, Some(1));
}

#[tokio::test]
async fn interleaved_read_write_in_one_tx() {
    // The orphan-reaping pattern from the diesel store: write, then read on the
    // same connection within the same transaction.
    let pool = fresh_pool("interleaved").await;
    let tx = pool.begin().await.unwrap();
    tx.execute(
        "INSERT INTO accounts (id, name) VALUES (1, 'alice')",
        vec![],
    )
    .await
    .unwrap();
    tx.execute(
        "INSERT INTO accounts (id, name) VALUES (2, 'bob')",
        vec![],
    )
    .await
    .unwrap();

    // Interleaved read on the same conn, same tx.
    let count: Option<i64> = tx
        .query_one("SELECT COUNT(*) FROM accounts", vec![])
        .await
        .unwrap();
    assert_eq!(count, Some(2));

    tx.commit().await.unwrap();
}
