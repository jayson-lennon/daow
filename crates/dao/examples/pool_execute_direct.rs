//! Direct Pool::execute and ToRow: using the write path without the #[dao] macro.
//!
//! For cases where the #[dao] macro doesn't fit (dynamic SQL, bulk inserts,
//! or ad-hoc writes), you can call pool.execute() directly with ToRow.
//!
//! Uses a TodoId newtype for strongly-typed IDs.
//!
//! Run with: cargo run --example pool_execute_direct

use dao::{ row::ColumnValue, Entity, FromSqlColumn, Pool, Result, ToRow, ToSqlColumn };

async fn setup_db() -> Pool {
    let pool = Pool::open(":memory:").unwrap();
    pool.execute(
        "CREATE TABLE todos (id INTEGER PRIMARY KEY, text TEXT, done INTEGER)",
        vec![],
    )
    .await
    .unwrap();
    pool
}

/// Strongly-typed todo ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TodoId(i64);

impl FromSqlColumn for TodoId {
    fn from_column(value: &ColumnValue) -> Result<Self> {
        Ok(TodoId(i64::from_column(value)?))
    }
}

impl ToSqlColumn for TodoId {
    fn to_column(&self) -> Result<dao::Param> {
        self.0.to_column()
    }
}

#[derive(Debug, Clone, Entity)]
#[dao(table = "todos")]
struct Todo {
    #[dao(pk)]
    id: TodoId,
    text: String,
    done: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let pool = setup_db().await;

    // --- Insert via ToRow ---
    let todo = Todo {
        id: TodoId(1),
        text: "Write examples".to_string(),
        done: false,
    };
    let result = pool
        .execute(
            "INSERT INTO todos (id, text, done) VALUES (?, ?, ?)",
            todo.to_insert_params()?,
        )
        .await?;
    println!(
        "Insert: rows_affected={}, last_insert_rowid={}",
        result.rows_affected, result.last_insert_rowid
    );

    // --- Update via ToRow ---
    let updated = Todo {
        id: TodoId(1),
        text: "Write examples".to_string(),
        done: true,
    };
    let result = pool
        .execute(
            "UPDATE todos SET text = ?, done = ? WHERE id = ?",
            updated.to_update_params()?,
        )
        .await?;
    println!("Update: rows_affected={}", result.rows_affected);

    // --- Delete via ToRow ---
    let result = pool
        .execute("DELETE FROM todos WHERE id = ?", updated.to_delete_params()?)
        .await?;
    println!("Delete: rows_affected={}", result.rows_affected);

    // --- Arbitrary SQL (no entity) ---
    pool.execute(
        "INSERT INTO todos (id, text, done) VALUES (?, ?, ?)",
        vec![
            Box::new(2i64),
            Box::new("Review PR".to_string()),
            Box::new(false),
        ],
    )
    .await?;

    pool.execute(
        "INSERT INTO todos (id, text, done) VALUES (?, ?, ?)",
        vec![
            Box::new(3i64),
            Box::new("Merge".to_string()),
            Box::new(false),
        ],
    )
    .await?;

    // Bulk update
    let result = pool
        .execute("UPDATE todos SET done = ? WHERE done = ?", vec![Box::new(true), Box::new(false)])
        .await?;
    println!("\nMarked {} todos as done", result.rows_affected);
    assert_eq!(result.rows_affected, 2);

    // Count
    let count: i64 = pool
        .query_one("SELECT COUNT(*) FROM todos", vec![])
        .await?
        .unwrap();
    println!("Total todos: {}", count);
    assert_eq!(count, 2);

    println!("\nAll checks passed!");
    Ok(())
}
