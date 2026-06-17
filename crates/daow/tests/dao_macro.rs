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

use daow::error::Error;
use daow::row::ColumnValue;
use daow::{async_trait, dao, Entity, FromSqlColumn, Pool, Result};

/// A simple entity for DAO tests.
#[derive(Debug, PartialEq, Entity)]
struct RecallEntity {
    id: i64,
    name: String,
}

/// DAO trait with multiple query methods.
#[dao]
#[async_trait]
trait RecallDao {
    #[query("SELECT id, name FROM recalls WHERE id = ?")]
    async fn get_by_id(&self, id: i64) -> Result<Option<RecallEntity>>;

    #[query("SELECT id, name FROM recalls ORDER BY id")]
    async fn get_all(&self) -> Result<Vec<RecallEntity>>;

    #[query("SELECT COUNT(*) FROM recalls")]
    async fn count(&self) -> Result<i64>;
}

/// Helper: create a test database with schema and sample data.
async fn setup_test_db() -> (Pool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

    pool.execute(
        "CREATE TABLE recalls (id INTEGER PRIMARY KEY, name TEXT)",
        vec![],
    )
    .await
    .unwrap();

    pool.execute(
        "INSERT INTO recalls (id, name) VALUES (?, ?)",
        vec![Box::new(1i64), Box::new("recall_a".to_string())],
    )
    .await
    .unwrap();

    pool.execute(
        "INSERT INTO recalls (id, name) VALUES (?, ?)",
        vec![Box::new(2i64), Box::new("recall_b".to_string())],
    )
    .await
    .unwrap();

    (pool, dir)
}

/// Test: get_by_id returns the correct entity.
#[tokio::test]
async fn dao_get_by_id() {
    let (pool, _dir) = setup_test_db().await;
    let recall_dao = RecallDao::new(pool);
    let result = recall_dao.get_by_id(1).await.unwrap();
    assert_eq!(
        result,
        Some(RecallEntity {
            id: 1,
            name: "recall_a".to_string(),
        })
    );
}

/// Test: get_by_id returns None for missing ID.
#[tokio::test]
async fn dao_get_by_id_missing() {
    let (pool, _dir) = setup_test_db().await;
    let recall_dao = RecallDao::new(pool);
    let result = recall_dao.get_by_id(999).await.unwrap();
    assert_eq!(result, None);
}

/// Test: get_all returns all entities.
#[tokio::test]
async fn dao_get_all() {
    let (pool, _dir) = setup_test_db().await;
    let recall_dao = RecallDao::new(pool);
    let results = recall_dao.get_all().await.unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].name, "recall_a");
    assert_eq!(results[1].name, "recall_b");
}

/// Test: count returns scalar value.
#[tokio::test]
async fn dao_count() {
    let (pool, _dir) = setup_test_db().await;
    let recall_dao = RecallDao::new(pool);
    let count = recall_dao.count().await.unwrap();
    assert_eq!(count, 2);
}

/// Test: DAO method with multiple parameters.
#[derive(Debug, PartialEq, Entity)]
struct NamedRecall {
    id: i64,
    name: String,
}

#[dao]
#[async_trait]
trait NamedRecallDao {
    #[query("SELECT id, name FROM recalls WHERE name = ? AND id > ?")]
    async fn find_by_name_and_min_id(
        &self,
        name: String,
        min_id: i64,
    ) -> Result<Option<NamedRecall>>;
}

#[tokio::test]
async fn dao_multi_param() {
    let (pool, _dir) = setup_test_db().await;
    let named_dao = NamedRecallDao::new(pool);
    let result = named_dao
        .find_by_name_and_min_id("recall_a".to_string(), 0)
        .await
        .unwrap();
    assert_eq!(
        result,
        Some(NamedRecall {
            id: 1,
            name: "recall_a".to_string(),
        })
    );

    // Should return None because id=1 is not > 1
    let result = named_dao
        .find_by_name_and_min_id("recall_a".to_string(), 1)
        .await
        .unwrap();
    assert_eq!(result, None);
}

/// Custom newtype for DAO tests.
#[derive(Debug, PartialEq)]
struct Email(String);

impl FromSqlColumn for Email {
    fn from_column(value: &ColumnValue) -> Result<Self> {
        let s = String::from_column(value)?;
        if s.contains('@') {
            Ok(Email(s))
        } else {
            Err(Error::custom(format!("invalid email: {s}")))
        }
    }
}

#[derive(Debug, PartialEq, Entity)]
struct UserEntity {
    id: i64,
    email: Email,
}

#[dao]
#[async_trait]
trait UserDao {
    #[query("SELECT id, email FROM users WHERE id = ?")]
    async fn get_by_id(&self, id: i64) -> Result<Option<UserEntity>>;
}

#[tokio::test]
async fn dao_custom_type() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

    pool.execute(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT)",
        vec![],
    )
    .await
    .unwrap();

    pool.execute(
        "INSERT INTO users (id, email) VALUES (?, ?)",
        vec![Box::new(1i64), Box::new("test@example.com".to_string())],
    )
    .await
    .unwrap();

    let user_dao = UserDao::new(pool);
    let result = user_dao.get_by_id(1).await.unwrap();
    assert_eq!(
        result,
        Some(UserEntity {
            id: 1,
            email: Email("test@example.com".to_string()),
        })
    );
}

/// Test: error propagation — query on nonexistent table returns Error::Database.
#[tokio::test]
async fn dao_database_error() {
    let (pool, _dir) = setup_test_db().await;

    let result: std::result::Result<Option<RecallEntity>, Error> = pool
        .query_one("SELECT id, name FROM nonexistent_table", vec![])
        .await;
    assert!(matches!(result, Err(Error::Database(_))));
}

/// Test: concurrent DAO calls via spawn_blocking don't deadlock.
#[tokio::test]
async fn dao_concurrent() {
    let (pool, _dir) = setup_test_db().await;

    let dao1 = RecallDao::new(pool.clone());
    let dao2 = RecallDao::new(pool);

    let h1 = tokio::spawn(async move {
        let r: Option<RecallEntity> = dao1.get_by_id(1).await.unwrap();
        r
    });
    let h2 = tokio::spawn(async move {
        let r: Option<RecallEntity> = dao2.get_by_id(2).await.unwrap();
        r
    });

    let r1 = h1.await.unwrap();
    let r2 = h2.await.unwrap();
    assert_eq!(r1.unwrap().name, "recall_a");
    assert_eq!(r2.unwrap().name, "recall_b");
}
