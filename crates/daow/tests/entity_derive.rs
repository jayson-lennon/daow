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
use daow::{FromSqlColumn, Pool};

/// A simple struct for testing Entity derive with primitive types.
#[derive(Debug, PartialEq, daow::Entity)]
struct Item {
    id: i64,
    name: String,
    price: f64,
}

/// Test basic Entity derive: multi-field struct round-trips through Pool.
#[tokio::test]
async fn entity_basic_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

    // Create table
    pool.query_all::<i64>(
        "CREATE TABLE items (id INTEGER, name TEXT, price REAL)",
        vec![],
    )
    .await
    .ok();

    // Insert
    pool.query_all::<i64>(
        "INSERT INTO items (id, name, price) VALUES (?, ?, ?)",
        vec![
            Box::new(1i64),
            Box::new("widget".to_string()),
            Box::new(9.99f64),
        ],
    )
    .await
    .ok();

    // Query one
    let result: Option<Item> = pool
        .query_one(
            "SELECT id, name, price FROM items WHERE id = ?",
            vec![Box::new(1i64)],
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        Some(Item {
            id: 1,
            name: "widget".to_string(),
            price: 9.99,
        })
    );

    // Query all
    let results: Vec<Item> = pool
        .query_all("SELECT id, name, price FROM items ORDER BY id", vec![])
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, 1);
}

/// Test Entity derive with nullable fields (Option<T>).
#[derive(Debug, PartialEq, daow::Entity)]
struct OptionalItem {
    id: i64,
    name: Option<String>,
    price: Option<f64>,
}

#[tokio::test]
async fn entity_option_fields() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

    pool.query_all::<i64>(
        "CREATE TABLE opt_items (id INTEGER, name TEXT, price REAL)",
        vec![],
    )
    .await
    .ok();

    // Insert with some NULLs
    pool.query_all::<i64>(
        "INSERT INTO opt_items (id, name, price) VALUES (?, NULL, ?)",
        vec![Box::new(1i64), Box::new(4.99f64)],
    )
    .await
    .ok();

    pool.query_all::<i64>(
        "INSERT INTO opt_items (id, name, price) VALUES (?, ?, NULL)",
        vec![Box::new(2i64), Box::new("gadget".to_string())],
    )
    .await
    .ok();

    // Row with NULL name
    let result: OptionalItem = pool
        .query_one(
            "SELECT id, name, price FROM opt_items WHERE id = ?",
            vec![Box::new(1i64)],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.name, None);
    assert_eq!(result.price, Some(4.99));

    // Row with NULL price
    let result: OptionalItem = pool
        .query_one(
            "SELECT id, name, price FROM opt_items WHERE id = ?",
            vec![Box::new(2i64)],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.name, Some("gadget".to_string()));
    assert_eq!(result.price, None);
}

/// Test Entity derive with column rename via #[dao(column = "...")].
#[derive(Debug, PartialEq, daow::Entity)]
struct RenamedItem {
    id: i64,
    #[dao(column = "item_name")]
    name: String,
}

#[tokio::test]
async fn entity_column_rename() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

    pool.query_all::<i64>("CREATE TABLE renamed (id INTEGER, item_name TEXT)", vec![])
        .await
        .ok();

    pool.query_all::<i64>(
        "INSERT INTO renamed (id, item_name) VALUES (?, ?)",
        vec![Box::new(1i64), Box::new("renamed_widget".to_string())],
    )
    .await
    .ok();

    let result: RenamedItem = pool
        .query_one(
            "SELECT id, item_name FROM renamed WHERE id = ?",
            vec![Box::new(1i64)],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.id, 1);
    assert_eq!(result.name, "renamed_widget");
}

/// Custom newtype to test user-implemented FromSqlColumn.
#[derive(Debug, PartialEq)]
struct Email(String);

impl FromSqlColumn for Email {
    fn from_column(value: &ColumnValue) -> Result<Self, Error> {
        let s = String::from_column(value)?;
        if s.contains('@') {
            Ok(Email(s))
        } else {
            Err(Error::custom(format!("invalid email: {s}")))
        }
    }
}

/// A struct using the custom FromSqlColumn type.
#[derive(Debug, PartialEq, daow::Entity)]
struct User {
    id: i64,
    email: Email,
}

#[tokio::test]
async fn entity_custom_from_sql_column() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

    pool.query_all::<i64>("CREATE TABLE users (id INTEGER, email TEXT)", vec![])
        .await
        .ok();

    pool.query_all::<i64>(
        "INSERT INTO users (id, email) VALUES (?, ?)",
        vec![Box::new(1i64), Box::new("test@example.com".to_string())],
    )
    .await
    .ok();

    let result: User = pool
        .query_one(
            "SELECT id, email FROM users WHERE id = ?",
            vec![Box::new(1i64)],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.id, 1);
    assert_eq!(result.email, Email("test@example.com".to_string()));
}

// --- Write support tests ---

use daow::{EntityMeta, ToRow};

/// A struct with table + pk for testing write support.
#[derive(Debug, PartialEq, daow::Entity)]
#[dao(table = "products")]
struct Product {
    #[dao(pk)]
    id: i64,
    name: String,
    price: f64,
}

#[tokio::test]
async fn entity_meta_sql_generation() {
    assert_eq!(Product::TABLE_NAME, "products");
    assert_eq!(Product::FIELD_COUNT, 3);
    assert_eq!(Product::PK_INDICES, &[0]);
    assert_eq!(
        Product::insert_sql(),
        "INSERT INTO products (id, name, price) VALUES (?, ?, ?)"
    );
    assert_eq!(
        Product::update_sql(),
        "UPDATE products SET name = ?, price = ? WHERE id = ?"
    );
    assert_eq!(Product::delete_sql(), "DELETE FROM products WHERE id = ?");
}

#[tokio::test]
async fn entity_to_row_insert() {
    let product = Product {
        id: 1,
        name: "widget".to_string(),
        price: 9.99,
    };
    let params = product.to_insert_params().unwrap();
    assert_eq!(params.len(), 3);

    // Verify by inserting into a real DB
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();
    pool.execute(
        "CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT, price REAL)",
        vec![],
    )
    .await
    .unwrap();

    let result = pool.execute(Product::insert_sql(), params).await.unwrap();
    assert_eq!(result.rows_affected, 1);
    assert_eq!(result.last_insert_rowid, 1);

    // Verify the row
    let fetched: Option<Product> = pool
        .query_one(
            "SELECT id, name, price FROM products WHERE id = ?",
            vec![Box::new(1i64)],
        )
        .await
        .unwrap();
    assert_eq!(
        fetched,
        Some(Product {
            id: 1,
            name: "widget".to_string(),
            price: 9.99
        })
    );
}

#[tokio::test]
async fn entity_to_row_update() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();
    pool.execute(
        "CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT, price REAL)",
        vec![],
    )
    .await
    .unwrap();

    // Insert initial
    let product = Product {
        id: 1,
        name: "widget".to_string(),
        price: 9.99,
    };
    pool.execute(Product::insert_sql(), product.to_insert_params().unwrap())
        .await
        .unwrap();

    // Update
    let updated = Product {
        id: 1,
        name: "gadget".to_string(),
        price: 19.99,
    };
    let result = pool
        .execute(Product::update_sql(), updated.to_update_params().unwrap())
        .await
        .unwrap();
    assert_eq!(result.rows_affected, 1);

    // Verify
    let fetched: Option<Product> = pool
        .query_one(
            "SELECT id, name, price FROM products WHERE id = ?",
            vec![Box::new(1i64)],
        )
        .await
        .unwrap();
    assert_eq!(
        fetched,
        Some(Product {
            id: 1,
            name: "gadget".to_string(),
            price: 19.99
        })
    );
}

#[tokio::test]
async fn entity_to_row_delete() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();
    pool.execute(
        "CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT, price REAL)",
        vec![],
    )
    .await
    .unwrap();

    // Insert then delete
    let product = Product {
        id: 1,
        name: "widget".to_string(),
        price: 9.99,
    };
    pool.execute(Product::insert_sql(), product.to_insert_params().unwrap())
        .await
        .unwrap();

    let result = pool
        .execute(Product::delete_sql(), product.to_delete_params().unwrap())
        .await
        .unwrap();
    assert_eq!(result.rows_affected, 1);

    // Verify gone
    let fetched: Option<Product> = pool
        .query_one(
            "SELECT id, name, price FROM products WHERE id = ?",
            vec![Box::new(1i64)],
        )
        .await
        .unwrap();
    assert_eq!(fetched, None);
}

// Verify that an Entity without table attribute still works (read-only)
#[derive(Debug, PartialEq, daow::Entity)]
struct ReadOnlyUser {
    id: i64,
    name: String,
}

#[tokio::test]
async fn entity_read_only_no_table() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();
    pool.execute(
        "CREATE TABLE ro_users (id INTEGER PRIMARY KEY, name TEXT)",
        vec![],
    )
    .await
    .unwrap();
    pool.execute(
        "INSERT INTO ro_users (id, name) VALUES (1, 'alice')",
        vec![],
    )
    .await
    .unwrap();

    let fetched: Option<ReadOnlyUser> = pool
        .query_one(
            "SELECT id, name FROM ro_users WHERE id = ?",
            vec![Box::new(1i64)],
        )
        .await
        .unwrap();
    assert_eq!(
        fetched,
        Some(ReadOnlyUser {
            id: 1,
            name: "alice".to_string()
        })
    );
}
