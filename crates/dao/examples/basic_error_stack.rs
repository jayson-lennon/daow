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

//! Same as `basic.rs`, but DAO methods return `error_stack::Report<E>`
//! instead of the `dao::Result` alias.
//!
//! Define a unit error type, use it in each method's return type, and
//! propagate errors with `?` exactly as before — `change_context` is
//! emitted for you.
//!
//! Run with: cargo run --example basic_error_stack

use dao::{
    async_trait, dao, row::ColumnValue, Entity, ExecuteResult, FromSqlColumn, Pool, ToSqlColumn,
};
use error_stack::{Report, ResultExt};

/// A consumer-defined unit error. `?` on any DAO method below lifts the
/// underlying `dao::Error` into this type via `change_context`.
#[derive(Debug)]
struct UserError;

impl std::fmt::Display for UserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("user operation failed")
    }
}

impl std::error::Error for UserError {}

/// Set up an in-memory database with schema.
async fn setup_db() -> Result<Pool, dao::Error> {
    let pool = Pool::open(":memory:")?;

    pool.execute(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)",
        vec![],
    )
    .await?;

    Ok(pool)
}

/// Strongly-typed user ID — prevents mixing up with other i64 values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UserId(i64);

impl FromSqlColumn for UserId {
    fn from_column(value: &ColumnValue) -> Result<Self, dao::Error> {
        Ok(UserId(i64::from_column(value)?))
    }
}

impl ToSqlColumn for UserId {
    fn to_column(&self) -> Result<dao::Param, dao::Error> {
        self.0.to_column()
    }
}

#[derive(Debug, Clone, Entity)]
#[dao(table = "users")]
struct User {
    #[dao(pk)]
    id: UserId,
    name: String,
    email: String,
}

#[dao]
#[async_trait]
trait UserDao {
    #[query("SELECT id, name, email FROM users WHERE id = ?")]
    async fn get_by_id(&self, id: UserId) -> Result<Option<User>, Report<UserError>>;

    #[query("SELECT id, name, email FROM users ORDER BY id")]
    async fn get_all(&self) -> Result<Vec<User>, Report<UserError>>;

    #[query("SELECT COUNT(*) FROM users")]
    async fn count(&self) -> Result<i64, Report<UserError>>;

    #[insert]
    async fn insert(&self, user: User) -> Result<ExecuteResult, Report<UserError>>;

    #[upsert]
    async fn upsert(&self, user: User) -> Result<ExecuteResult, Report<UserError>>;

    #[update]
    async fn update(&self, user: User) -> Result<ExecuteResult, Report<UserError>>;

    #[delete]
    async fn delete(&self, user: User) -> Result<ExecuteResult, Report<UserError>>;
}

#[tokio::main]
async fn main() -> Result<(), Report<UserError>> {
    // `setup_db` returns `dao::Result`, so bridge it into our domain error
    // with `change_context`.
    let pool = setup_db().await.change_context(UserError)?;
    let user_dao = UserDao::new(pool);

    // Insert users via the DAO.
    user_dao
        .insert(User {
            id: UserId(1),
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
        })
        .await?;

    user_dao
        .insert(User {
            id: UserId(2),
            name: "Bob".to_string(),
            email: "bob@example.com".to_string(),
        })
        .await?;

    // Upsert: insert a new user (id=3 is absent, so it inserts).
    user_dao
        .upsert(User {
            id: UserId(3),
            name: "Carol".to_string(),
            email: "carol@example.com".to_string(),
        })
        .await?;

    // Get by ID — note the strongly-typed UserId.
    let user = user_dao.get_by_id(UserId(1)).await?;
    println!("Found user: {:?}", user);

    // Get all.
    let users = user_dao.get_all().await?;
    println!("All users:");
    for u in &users {
        println!("  {} @{}", u.name, u.email);
    }

    // Count.
    let count = user_dao.count().await?;
    println!("Total users: {}", count);

    // Update a user.
    user_dao
        .update(User {
            id: UserId(1),
            name: "Alice Updated".to_string(),
            email: "alice_new@example.com".to_string(),
        })
        .await?;

    // Upsert: update an existing user (id=3 now exists, so non-PK columns update).
    user_dao
        .upsert(User {
            id: UserId(3),
            name: "Carol Renamed".to_string(),
            email: "carol_new@example.com".to_string(),
        })
        .await?;

    let upserted = user_dao.get_by_id(UserId(3)).await?.unwrap();
    println!("Upserted user: {:?}", upserted);
    assert_eq!(upserted.name, "Carol Renamed");

    let updated = user_dao.get_by_id(UserId(1)).await?.unwrap();
    println!("Updated user: {:?}", updated);
    assert_eq!(updated.name, "Alice Updated");

    // Delete a user.
    user_dao
        .delete(User {
            id: UserId(2),
            name: "Bob".to_string(),
            email: "bob@example.com".to_string(),
        })
        .await?;

    let remaining = user_dao.get_all().await?;
    assert_eq!(remaining.len(), 2);

    // Missing user returns None.
    let missing = user_dao.get_by_id(UserId(999)).await?;
    assert!(missing.is_none());

    println!("\nAll checks passed!");
    Ok(())
}
