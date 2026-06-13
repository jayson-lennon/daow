//! `Pool::builder()` — configuring pragmas and pool bounds.
//!
//! Shows how to build a pool that applies per-connection pragmas (required for
//! `ON DELETE CASCADE`, which silently no-ops when `foreign_keys` is off — the default)
//! and how to bound concurrency with `max_size`.
//!
//! Run with: cargo run --example pool_builder

use dao::{Pool, Result};

/// Build a pool with the pragmas we need (foreign keys + in-memory journal),
/// then create a parent/child schema that relies on `ON DELETE CASCADE`.
async fn setup_db() -> Result<Pool> {
    let pool = Pool::builder()
        .path(":memory:")
        .max_size(4)
        .pragma("foreign_keys", "ON")
        .pragma("journal_mode", "MEMORY")
        .build()?;

    // foreign_keys is observable on a checked-out connection (1 = ON).
    let fk: i64 = pool.query_one("PRAGMA foreign_keys", vec![]).await?.unwrap();
    assert_eq!(fk, 1);
    println!("foreign_keys = {fk} (ON — cascades will fire)");

    pool.execute(
        "CREATE TABLE parents (id INTEGER PRIMARY KEY)",
        vec![],
    )
    .await?;
    pool.execute(
        "CREATE TABLE children (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER REFERENCES parents(id) ON DELETE CASCADE
        )",
        vec![],
    )
    .await?;

    Ok(pool)
}

#[tokio::main]
async fn main() -> Result<()> {
    let pool = setup_db().await?;

    // Insert then delete the parent; the child should cascade-delete.
    pool.execute("INSERT INTO parents (id) VALUES (1)", vec![]).await?;
    pool.execute("INSERT INTO children (id, parent_id) VALUES (1, 1)", vec![])
        .await?;
    pool.execute("DELETE FROM parents WHERE id = 1", vec![]).await?;

    let orphans: Option<i64> = pool
        .query_one("SELECT COUNT(*) FROM children", vec![])
        .await?;
    assert_eq!(orphans, Some(0), "cascade should have removed the child");
    println!("cascade delete worked: 0 orphaned children after parent deletion");

    // Back-compat: the simple constructor still works (defaults: max_size 4, no pragmas).
    let plain = Pool::open(":memory:")?;
    let one: i64 = plain.query_one("SELECT 1", vec![]).await?.unwrap();
    assert_eq!(one, 1);

    println!("\nAll checks passed!");
    Ok(())
}
