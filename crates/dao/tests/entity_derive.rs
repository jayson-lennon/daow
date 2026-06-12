use dao::{FromSqlColumn, Pool};
use dao::row::ColumnValue;
use dao::error::Error;

/// A simple struct for testing Entity derive with primitive types.
#[derive(Debug, PartialEq, dao::Entity)]
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
        .query_one("SELECT id, name, price FROM items WHERE id = ?", vec![Box::new(1i64)])
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
#[derive(Debug, PartialEq, dao::Entity)]
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
        .query_one("SELECT id, name, price FROM opt_items WHERE id = ?", vec![Box::new(1i64)])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.name, None);
    assert_eq!(result.price, Some(4.99));

    // Row with NULL price
    let result: OptionalItem = pool
        .query_one("SELECT id, name, price FROM opt_items WHERE id = ?", vec![Box::new(2i64)])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.name, Some("gadget".to_string()));
    assert_eq!(result.price, None);
}

/// Test Entity derive with column rename via #[dao(column = "...")].
#[derive(Debug, PartialEq, dao::Entity)]
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

    pool.query_all::<i64>(
        "CREATE TABLE renamed (id INTEGER, item_name TEXT)",
        vec![],
    )
    .await
    .ok();

    pool.query_all::<i64>(
        "INSERT INTO renamed (id, item_name) VALUES (?, ?)",
        vec![Box::new(1i64), Box::new("renamed_widget".to_string())],
    )
    .await
    .ok();

    let result: RenamedItem = pool
        .query_one("SELECT id, item_name FROM renamed WHERE id = ?", vec![Box::new(1i64)])
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
#[derive(Debug, PartialEq, dao::Entity)]
struct User {
    id: i64,
    email: Email,
}

#[tokio::test]
async fn entity_custom_from_sql_column() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

    pool.query_all::<i64>(
        "CREATE TABLE users (id INTEGER, email TEXT)",
        vec![],
    )
    .await
    .ok();

    pool.query_all::<i64>(
        "INSERT INTO users (id, email) VALUES (?, ?)",
        vec![Box::new(1i64), Box::new("test@example.com".to_string())],
    )
    .await
    .ok();

    let result: User = pool
        .query_one("SELECT id, email FROM users WHERE id = ?", vec![Box::new(1i64)])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.id, 1);
    assert_eq!(result.email, Email("test@example.com".to_string()));
}
