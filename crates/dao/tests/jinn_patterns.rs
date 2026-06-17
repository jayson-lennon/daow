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

//! Verification that jinn's two caller-owned patterns — the FK-toggle migrator
//! and the WAL checkpoint shutdown — are expressible using only dao's public API.
//!
//! These are deliberately **out of scope** for dao's own surface (see spec's
//! "Why migration runner and shutdown stay caller-owned"). This test exists to
//! prove that claim: no new dao method is required to port either pattern.

use dao::{FromRow, Pool, Result, Row};
use tempfile::tempdir;

// ===========================================================================
// #6 — jinn migrator pattern (migrator.rs)
//
// jinn's `run_migrations` does:
//   1. PRAGMA foreign_keys=OFF           (per-connection; FK off for the run)
//   2. run DDL (CREATE/ALTER)            (FK off avoids ordering constraints)
//   3. PRAGMA foreign_keys=ON            (re-enable)
//   4. PRAGMA foreign_key_check          (returns 0 rows = clean)
//
// dao's `with_conn` hands out `&mut rusqlite::Connection` directly, so this
// entire sequence is expressible without any new dao method.
// ===========================================================================

/// Mirrors jinn's migrator: disable FK, run schema DDL, re-enable FK, verify
/// integrity via `foreign_key_check`. Returns the number of FK violations
/// (0 = clean).
async fn run_migrations_jinn_style(pool: &Pool, ddl: &str) -> Result<usize> {
    let ddl = ddl.to_owned();
    pool.with_conn(move |conn| {
        // 1. FK off for the migration run.
        conn.pragma_update(None, "foreign_keys", "OFF")?;

        // 2. Run the migration DDL.
        conn.execute_batch(&ddl)?;

        // 3. Re-enable FK.
        conn.pragma_update(None, "foreign_keys", "ON")?;

        // 4. foreign_key_check returns one row per violation; count them.
        let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
        let violations = stmt.query_map([], |_| Ok(()))?.count();
        Ok(violations)
    })
    .await
}

#[tokio::test]
async fn migrator_pattern_runs_via_with_conn() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("mig.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

    // Two tables with a parent→child FK. Created in one DDL batch with FK off,
    // matching jinn's behaviour where DDL ordering is unconstrained.
    let ddl = "CREATE TABLE parent (id INTEGER PRIMARY KEY);
               CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent(id));";

    let violations = run_migrations_jinn_style(&pool, ddl).await.unwrap();
    assert_eq!(violations, 0, "clean schema should report 0 FK violations");
}

#[tokio::test]
async fn migrator_pattern_detects_fk_violations() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("mig2.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

    // Insert a child row pointing at a non-existent parent, then run the FK
    // check. FK is OFF at insert time (inside with_conn), so the insert
    // succeeds; foreign_key_check then catches the dangling reference.
    let ddl = "CREATE TABLE parent (id INTEGER PRIMARY KEY);
               CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent(id));
               INSERT INTO child (id, parent_id) VALUES (1, 999);";

    let violations = run_migrations_jinn_style(&pool, ddl).await.unwrap();
    assert!(
        violations > 0,
        "dangling FK should be detected by foreign_key_check"
    );
}

// ===========================================================================
// #7 — jinn shutdown checkpoint pattern (sqlite.rs)
//
// jinn's `shutdown_blocking` runs `PRAGMA wal_checkpoint(TRUNCATE)` and reads
// the result row by named columns (busy, log, checkpointed). dao's `query_one`
// supports named-column extraction via a manual `FromRow` impl — no new dao
// method required.
// ===========================================================================

/// Mirrors jinn's `CheckpointResult` struct and its named-column mapping.
struct CheckpointResult {
    busy: i64,
    log: i64,
    checkpointed: i64,
}

impl FromRow for CheckpointResult {
    fn from_row(row: &Row) -> Result<Self> {
        Ok(Self {
            busy: row.get("busy")?,
            log: row.get("log")?,
            checkpointed: row.get("checkpointed")?,
        })
    }
}

#[tokio::test]
async fn shutdown_checkpoint_readable_by_named_columns() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("chk.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

    // Write something so the WAL has frames to checkpoint.
    pool.execute("CREATE TABLE t (id INTEGER PRIMARY KEY)", vec![])
        .await
        .unwrap();
    pool.execute("INSERT INTO t (id) VALUES (1), (2), (3)", vec![])
        .await
        .unwrap();

    // Run the checkpoint and read the result row by named columns — the exact
    // pattern jinn's `shutdown_blocking` uses.
    let result: Option<CheckpointResult> = pool
        .query_one("PRAGMA wal_checkpoint(TRUNCATE)", vec![])
        .await
        .unwrap();

    let result = result.expect("wal_checkpoint should return a result row");
    // `busy` should be 0 on an idle, single-connection test DB.
    assert_eq!(result.busy, 0, "checkpoint should not be busy on idle DB");
    // `log` and `checkpointed` are non-negative; exact values depend on WAL
    // state. Just assert they're sane (>= 0).
    assert!(result.log >= 0, "log frames should be non-negative");
    assert!(
        result.checkpointed >= 0,
        "checkpointed frames should be non-negative"
    );
}
