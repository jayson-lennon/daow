//! Basic DAO example: demonstrates Entity derive, DAO trait, and query methods.
//!
//! Run with: cargo run --example basic

use dao::{async_trait, dao, Entity, Pool, Result};

/// Set up an in-memory database with schema and sample data.
async fn setup_db() -> Pool {
    let pool = Pool::open(":memory:").unwrap();

    pool.query_all::<i64>(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)",
        vec![],
    )
    .await
    .unwrap();

    pool.query_all::<i64>(
        "INSERT INTO users (id, name, email) VALUES (?, ?, ?)",
        vec![
            Box::new(1i64),
            Box::new("Alice".to_string()),
            Box::new("alice@example.com".to_string()),
        ],
    )
    .await
    .unwrap();

    pool.query_all::<i64>(
        "INSERT INTO users (id, name, email) VALUES (?, ?, ?)",
        vec![
            Box::new(2i64),
            Box::new("Bob".to_string()),
            Box::new("bob@example.com".to_string()),
        ],
    )
    .await
    .unwrap();

    pool
}

#[derive(Debug, Entity)]
struct User {
    id: i64,
    name: String,
    email: String,
}

#[dao]
#[async_trait]
trait UserDao {
    #[query("SELECT id, name, email FROM users WHERE id = ?")]
    async fn get_by_id(&self, id: i64) -> Result<Option<User>>;

    #[query("SELECT id, name, email FROM users ORDER BY id")]
    async fn get_all(&self) -> Result<Vec<User>>;

    #[query("SELECT COUNT(*) FROM users")]
    async fn count(&self) -> Result<i64>;
}

#[tokio::main]
async fn main() {
    let pool = setup_db().await;
    let user_dao = UserDao::new(pool);

    // Get by ID.
    let user = user_dao.get_by_id(1).await.unwrap();
    println!("Found user: {:?}", user);

    // Get all.
    let users = user_dao.get_all().await.unwrap();
    println!("All users:");
    for u in &users {
        println!("  {} @{}", u.name, u.email);
    }

    // Count.
    let count = user_dao.count().await.unwrap();
    println!("Total users: {}", count);

    // Missing user returns None.
    let missing = user_dao.get_by_id(999).await.unwrap();
    println!("Missing user: {:?}", missing);

    assert_eq!(users.len(), 2);
    assert_eq!(count, 2);
    assert!(missing.is_none());

    println!("\nAll checks passed!");
}
