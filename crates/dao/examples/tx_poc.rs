//! Proof-of-concept / living documentation for the `dao` transaction layer.
//!
//! This is the de-risking PoC (originally hand-rolled to validate the lifetime
//! design) ported onto the **real** `dao::Pool` / `dao::Transaction` API. It
//! exercises the runtime scenarios the design depends on:
//!
//!   1. Pool autocommit path works (each call checks out a connection).
//!   2. `with_conn` runs multiple statements on one held connection atomically.
//!   3. `pool.begin()` -> writes -> `commit()` persists.
//!   4. `pool.begin()` -> writes -> drop (no commit) reverts (rollback-on-drop).
//!   5. A mid-transaction failure reverts everything done so far in the tx.
//!
//! The borrow-safety guarantees (E0505: cannot commit while a `.with(&tx)` view
//! is live; E0597: a view cannot escape the tx's scope) are enforced purely by
//! the type signatures and are verified by trybuild compile-fail tests in
//! `crates/dao/tests/ui/` rather than at runtime.
//!
//! Run with: `cargo run --example tx_poc`

use dao::{ExecuteResult, Pool};
use std::time::Duration;

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS accounts (id INTEGER PRIMARY KEY, balance INTEGER NOT NULL)";

fn fresh_pool() -> (Pool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let pool = Pool::builder()
        .path(dir.path().join("poc.sqlite").to_str().unwrap())
        .max_size(2)
        .acquire_timeout(Duration::from_secs(5))
        .pragma("journal_mode", "WAL")
        .pragma("foreign_keys", "ON")
        .build()
        .unwrap();
    (pool, dir)
}

#[tokio::main]
async fn main() {
    println!("dao transaction PoC\n");

    // Scenario 1 — pool autocommit path.
    {
        let (pool, _dir) = fresh_pool();
        pool.execute(SCHEMA, vec![]).await.unwrap();
        pool.execute(
            "INSERT INTO accounts (id, balance) VALUES (1, 100)",
            vec![],
        )
        .await
        .unwrap();
        let bal: Option<i64> = pool
            .query_one("SELECT balance FROM accounts WHERE id = 1", vec![])
            .await
            .unwrap();
        assert_eq!(bal, Some(100), "scenario 1: autocommit insert observable");
        println!("[OK] 1. pool autocommit path writes and reads");
    }

    // Scenario 2 — with_conn runs multiple statements atomically on one held conn.
    {
        let (pool, _dir) = fresh_pool();
        pool.execute(SCHEMA, vec![]).await.unwrap();
        // Debit one account, credit another, in a manual BEGIN...COMMIT via the
        // held connection. If anything between fails, both revert.
        pool.with_conn(|conn| {
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT INTO accounts (id, balance) VALUES (1, 100)",
                [],
            )?;
            tx.execute(
                "INSERT INTO accounts (id, balance) VALUES (2, 0)",
                [],
            )?;
            tx.execute("UPDATE accounts SET balance = balance - 50 WHERE id = 1", [])?;
            tx.execute("UPDATE accounts SET balance = balance + 50 WHERE id = 2", [])?;
            tx.commit()?;
            Ok::<_, dao::Error>(())
        })
        .await
        .unwrap();
        let bal1: Option<i64> = pool
            .query_one("SELECT balance FROM accounts WHERE id = 1", vec![])
            .await
            .unwrap();
        let bal2: Option<i64> = pool
            .query_one("SELECT balance FROM accounts WHERE id = 2", vec![])
            .await
            .unwrap();
        assert_eq!(bal1, Some(50), "scenario 2: debit applied");
        assert_eq!(bal2, Some(50), "scenario 2: credit applied");
        println!("[OK] 2. with_conn atomic multi-statement transfer");
    }

    // Scenario 3 — begin() + writes + commit() persists.
    {
        let (pool, _dir) = fresh_pool();
        pool.execute(SCHEMA, vec![]).await.unwrap();
        {
            let tx = pool.begin().await.unwrap();
            tx.execute(
                "INSERT INTO accounts (id, balance) VALUES (1, 7)",
                vec![],
            )
            .await
            .unwrap();
            tx.execute(
                "INSERT INTO accounts (id, balance) VALUES (2, 9)",
                vec![],
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
        }
        let count: Option<i64> = pool
            .query_one("SELECT COUNT(*) FROM accounts", vec![])
            .await
            .unwrap();
        assert_eq!(count, Some(2), "scenario 3: committed rows persist");
        println!("[OK] 3. begin + commit persists");
    }

    // Scenario 4 — begin() + writes + drop (no commit) reverts.
    {
        let (pool, _dir) = fresh_pool();
        pool.execute(SCHEMA, vec![]).await.unwrap();
        {
            let tx = pool.begin().await.unwrap();
            tx.execute(
                "INSERT INTO accounts (id, balance) VALUES (1, 999)",
                vec![],
            )
            .await
            .unwrap();
            // Deliberately do NOT commit — drop triggers rollback.
        }
        let count: Option<i64> = pool
            .query_one("SELECT COUNT(*) FROM accounts", vec![])
            .await
            .unwrap();
        assert_eq!(count, Some(0), "scenario 4: dropped tx reverts");
        println!("[OK] 4. drop without commit rolls back");
    }

    // Scenario 5 — a mid-tx failure reverts everything done so far in the tx.
    {
        let (pool, _dir) = fresh_pool();
        pool.execute(SCHEMA, vec![]).await.unwrap();
        // Seed id=1 so the second insert (also id=1) fails with a PK violation.
        pool.execute(
            "INSERT INTO accounts (id, balance) VALUES (1, 5)",
            vec![],
        )
        .await
        .unwrap();

        let result = async {
            let tx = pool.begin().await.unwrap();
            // This one would succeed on its own...
            tx.execute(
                "INSERT INTO accounts (id, balance) VALUES (2, 8)",
                vec![],
            )
            .await
            .unwrap();
            // ...but a duplicate PK here fails, and the whole tx must revert.
            let res: Result<ExecuteResult, dao::Error> = tx
                .execute(
                    "INSERT INTO accounts (id, balance) VALUES (1, 11)",
                    vec![],
                )
                .await;
            if res.is_err() {
                // tx drops here -> rollback; id=2's insert is undone.
                return;
            }
            tx.commit().await.unwrap();
        }
        .await;

        let _ = result;
        let count: Option<i64> = pool
            .query_one("SELECT COUNT(*) FROM accounts", vec![])
            .await
            .unwrap();
        assert_eq!(count, Some(1), "scenario 5: mid-tx failure reverts all");
        println!("[OK] 5. mid-tx failure reverts whole transaction");
    }

    println!("\nAll scenarios passed.");
}
