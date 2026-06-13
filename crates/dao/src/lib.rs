pub mod conn;
pub mod entity_meta;
pub mod error;
pub mod from_row;
pub mod from_sql;
pub mod pool;
pub mod row;
pub mod to_row;
pub mod to_sql;

pub use dao_macros::{dao, Entity};

pub use conn::Conn;
pub use entity_meta::{EntityMeta, ExecuteResult};
pub use error::Error;
pub use error::Result;
pub use from_row::FromRow;
pub use from_sql::FromSqlColumn;
pub use pool::{Param, Pool, PoolBuilder, Transaction};
pub use row::{ColumnValue, Row};
pub use to_row::ToRow;
pub use to_sql::ToSqlColumn;

pub use async_trait::async_trait;

#[cfg(test)]
mod tests {
    use crate::error::Error;
    use crate::pool::Pool;
    use crate::Param;
    use crate::ToRow;
    use crate::ToSqlColumn;

    /// Test that rusqlite errors convert to our Error::Database variant.
    #[test]
    fn error_from_rusqlite() {
        let err = rusqlite::Error::InvalidColumnIndex(999);
        let our_err: Error = err.into();

        match our_err {
            Error::Database(_) => {} // expected
            other => panic!("expected Database variant, got: {other:?}"),
        }

        assert!(our_err.to_string().contains("database error"));
    }

    /// Test that custom() and custom_boxed() constructors work.
    #[test]
    fn error_custom_constructors() {
        let e1 = Error::custom("something went wrong");
        assert!(matches!(e1, Error::Custom(ref s) if s == "something went wrong"));

        let e2 = Error::custom_boxed(Box::new(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "pipe broke",
        )));
        assert!(matches!(e2, Error::CustomBoxed(_)));
        assert!(e2.to_string().contains("pipe broke"));
    }

    /// Test Error source chains work for Database and CustomBoxed.
    #[test]
    fn error_source_chain() {
        use std::error::Error as StdError;

        let db_err = Error::Database(rusqlite::Error::InvalidColumnIndex(0));
        assert!(db_err.source().is_some());

        let boxed = Error::custom_boxed(Box::new(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "pipe broke",
        )));
        assert!(boxed.source().is_some());

        let custom = Error::custom("msg");
        assert!(custom.source().is_none());
    }

    /// Test Pool::open() creates a database file.
    #[test]
    fn pool_open_creates_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let path_str = db_path.to_str().unwrap();

        assert!(!db_path.exists());
        let pool = Pool::open(path_str).unwrap();
        assert!(db_path.exists());

        // Pool can be cloned (Arc increment)
        let _pool2 = pool.clone();
    }

    /// Test Pool::open() fails on invalid path.
    #[test]
    fn pool_open_invalid_path() {
        let result = Pool::open("/nonexistent/deeply/nested/dir/test.db");
        assert!(result.is_err());
    }

    /// Test scalar query via Pool — SELECT of a single value.
    #[tokio::test]
    async fn pool_scalar_query() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

        let result: Option<i64> = pool.query_one("SELECT 42", vec![]).await.unwrap();
        assert_eq!(result, Some(42));

        let result: Option<String> = pool
            .query_one("SELECT 'hello world'", vec![])
            .await
            .unwrap();
        assert_eq!(result, Some("hello world".to_string()));
    }

    /// Test query_one returns None for empty result.
    #[tokio::test]
    async fn pool_query_one_empty() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

        pool.query_all::<i64>("CREATE TABLE empty_test (id INTEGER PRIMARY KEY)", vec![])
            .await
            .ok();

        let result: Option<i64> = pool
            .query_one("SELECT id FROM empty_test", vec![])
            .await
            .unwrap();
        assert_eq!(result, None);
    }

    /// Test query_all returns multiple rows.
    #[tokio::test]
    async fn pool_query_all() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

        let result: Vec<i64> = pool
            .query_all("VALUES (1), (2), (3)", vec![])
            .await
            .unwrap();
        assert_eq!(result, vec![1, 2, 3]);
    }

    /// Test query with positional parameters.
    #[tokio::test]
    async fn pool_query_with_params() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

        pool.query_all::<i64>("CREATE TABLE items (id INTEGER, name TEXT)", vec![])
            .await
            .ok();

        pool.query_all::<i64>(
            "INSERT INTO items (id, name) VALUES (?, ?)",
            vec![Box::new(1i64), Box::new("widget".to_string())],
        )
        .await
        .ok();

        let result: Option<String> = pool
            .query_one("SELECT name FROM items WHERE id = ?", vec![Box::new(1i64)])
            .await
            .unwrap();
        assert_eq!(result, Some("widget".to_string()));
    }

    /// Test concurrent queries via spawn_blocking don't deadlock.
    #[tokio::test]
    async fn pool_concurrent_queries() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

        let h1 = tokio::spawn({
            let pool = pool.clone();
            async move {
                let val: i64 = pool.query_one("SELECT 1", vec![]).await.unwrap().unwrap();
                val
            }
        });

        let h2 = tokio::spawn({
            let pool = pool.clone();
            async move {
                let val: i64 = pool.query_one("SELECT 2", vec![]).await.unwrap().unwrap();
                val
            }
        });

        assert_eq!(h1.await.unwrap(), 1);
        assert_eq!(h2.await.unwrap(), 2);
    }

    /// Test that a database error maps to our Error::Database variant.
    #[tokio::test]
    async fn pool_database_error() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

        let result: Result<Option<i64>, Error> = pool
            .query_one("SELECT * FROM nonexistent_table", vec![])
            .await;

        match result {
            Err(Error::Database(_)) => {} // expected
            Err(other) => panic!("expected Database error, got: {other:?}"),
            Ok(_) => panic!("expected error, got Ok"),
        }
    }

    // --- ToSqlColumn unit tests ---

    /// Test that ToSqlColumn blanket impl produces a Param that rusqlite can bind.
    #[test]
    fn to_sql_column_i64() {
        use crate::to_sql::ToSqlColumn;
        let param = 42i64.to_column().unwrap();
        // Verify round-trip through rusqlite directly
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE t (v INTEGER)", []).unwrap();
        conn.execute(
            "INSERT INTO t (v) VALUES (?)",
            rusqlite::params_from_iter(std::iter::once(param.as_ref())),
        )
        .unwrap();
        let val: i64 = conn.query_row("SELECT v FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(val, 42);
    }

    #[test]
    fn to_sql_column_string() {
        use crate::to_sql::ToSqlColumn;
        let param = "hello".to_string().to_column().unwrap();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE t (v TEXT)", []).unwrap();
        conn.execute(
            "INSERT INTO t (v) VALUES (?)",
            rusqlite::params_from_iter(std::iter::once(param.as_ref())),
        )
        .unwrap();
        let val: String = conn.query_row("SELECT v FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(val, "hello");
    }

    #[test]
    fn to_sql_column_f64() {
        use crate::to_sql::ToSqlColumn;
        let param = std::f64::consts::PI.to_column().unwrap();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE t (v REAL)", []).unwrap();
        conn.execute(
            "INSERT INTO t (v) VALUES (?)",
            rusqlite::params_from_iter(std::iter::once(param.as_ref())),
        )
        .unwrap();
        let val: f64 = conn.query_row("SELECT v FROM t", [], |r| r.get(0)).unwrap();
        assert!((val - std::f64::consts::PI).abs() < f64::EPSILON);
    }

    #[test]
    fn to_sql_column_bool() {
        use crate::to_sql::ToSqlColumn;
        let param = true.to_column().unwrap();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE t (v INTEGER)", []).unwrap();
        conn.execute(
            "INSERT INTO t (v) VALUES (?)",
            rusqlite::params_from_iter(std::iter::once(param.as_ref())),
        )
        .unwrap();
        let val: bool = conn.query_row("SELECT v FROM t", [], |r| r.get(0)).unwrap();
        assert!(val);
    }

    #[test]
    fn to_sql_column_option_some() {
        use crate::to_sql::ToSqlColumn;
        let param = Some(42i64).to_column().unwrap();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE t (v INTEGER)", []).unwrap();
        conn.execute(
            "INSERT INTO t (v) VALUES (?)",
            rusqlite::params_from_iter(std::iter::once(param.as_ref())),
        )
        .unwrap();
        let val: Option<i64> = conn.query_row("SELECT v FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(val, Some(42));
    }

    #[test]
    fn to_sql_column_option_none() {
        use crate::to_sql::ToSqlColumn;
        let param: Option<i64> = None;
        let param = param.to_column().unwrap();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE t (v INTEGER)", []).unwrap();
        conn.execute(
            "INSERT INTO t (v) VALUES (?)",
            rusqlite::params_from_iter(std::iter::once(param.as_ref())),
        )
        .unwrap();
        let val: Option<i64> = conn.query_row("SELECT v FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(val, None);
    }

    #[test]
    fn to_sql_column_vec_u8() {
        use crate::to_sql::ToSqlColumn;
        let data = vec![1u8, 2, 3];
        let param = data.to_column().unwrap();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE t (v BLOB)", []).unwrap();
        conn.execute(
            "INSERT INTO t (v) VALUES (?)",
            rusqlite::params_from_iter(std::iter::once(param.as_ref())),
        )
        .unwrap();
        let val: Vec<u8> = conn.query_row("SELECT v FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(val, vec![1, 2, 3]);
    }

    /// Test custom newtype ToSqlColumn + FromSqlColumn round-trip.
    #[test]
    fn to_sql_column_custom_newtype_roundtrip() {
        use crate::from_sql::FromSqlColumn;
        use crate::row::ColumnValue;
        use crate::to_sql::ToSqlColumn;

        #[derive(Debug, PartialEq)]
        struct Slug(String);

        impl FromSqlColumn for Slug {
            fn from_column(value: &ColumnValue) -> crate::error::Result<Self> {
                let s = String::from_column(value)?;
                if s.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-' || c.is_ascii_digit())
                {
                    Ok(Slug(s))
                } else {
                    Err(crate::error::Error::custom(format!("invalid slug: {s}")))
                }
            }
        }

        impl ToSqlColumn for Slug {
            fn to_column(&self) -> crate::error::Result<crate::pool::Param> {
                self.0.to_column()
            }
        }

        let slug = Slug("hello-world".to_string());
        let param = slug.to_column().unwrap();

        // Round-trip through SQLite
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE t (v TEXT)", []).unwrap();
        conn.execute(
            "INSERT INTO t (v) VALUES (?)",
            rusqlite::params_from_iter(std::iter::once(param.as_ref())),
        )
        .unwrap();

        let val: Slug = conn
            .query_row("SELECT v FROM t", [], |row| {
                let vref = row.get_ref(0)?;
                Slug::from_column(&ColumnValue::new(vref))
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
            })
            .unwrap();
        assert_eq!(val, Slug("hello-world".to_string()));
    }

    /// Manual ToRow implementation for testing param ordering.
    /// Fields: id (PK), name, email. Table: test_users.
    struct TestUser {
        id: i64,
        name: String,
        email: String,
    }

    impl ToRow for TestUser {
        fn to_insert_params(&self) -> crate::error::Result<Vec<Param>> {
            Ok(vec![
                ToSqlColumn::to_column(&self.id)?,
                ToSqlColumn::to_column(&self.name)?,
                ToSqlColumn::to_column(&self.email)?,
            ])
        }

        fn to_update_params(&self) -> crate::error::Result<Vec<Param>> {
            // Non-PK first (SET), then PK (WHERE)
            Ok(vec![
                ToSqlColumn::to_column(&self.name)?,
                ToSqlColumn::to_column(&self.email)?,
                ToSqlColumn::to_column(&self.id)?,
            ])
        }

        fn to_delete_params(&self) -> crate::error::Result<Vec<Param>> {
            Ok(vec![ToSqlColumn::to_column(&self.id)?])
        }
    }

    fn test_user() -> TestUser {
        TestUser {
            id: 1,
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
        }
    }

    fn setup_test_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE test_users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn to_row_insert_params() {
        let conn = setup_test_db();
        let user = test_user();
        let params = user.to_insert_params().unwrap();

        conn.execute(
            "INSERT INTO test_users (id, name, email) VALUES (?, ?, ?)",
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
        )
        .unwrap();

        let (name, email): (String, String) = conn
            .query_row("SELECT name, email FROM test_users WHERE id = 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(name, "Alice");
        assert_eq!(email, "alice@example.com");
    }

    #[test]
    fn to_row_update_params() {
        let conn = setup_test_db();
        let user = test_user();

        // Insert first
        let insert_params = user.to_insert_params().unwrap();
        conn.execute(
            "INSERT INTO test_users (id, name, email) VALUES (?, ?, ?)",
            rusqlite::params_from_iter(insert_params.iter().map(|p| p.as_ref())),
        )
        .unwrap();

        // Update
        let updated = TestUser {
            name: "Bob".to_string(),
            ..user
        };
        let params = updated.to_update_params().unwrap();
        conn.execute(
            "UPDATE test_users SET name = ?, email = ? WHERE id = ?",
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
        )
        .unwrap();

        let name: String = conn
            .query_row("SELECT name FROM test_users WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "Bob");
    }

    #[test]
    fn to_row_delete_params() {
        let conn = setup_test_db();
        let user = test_user();

        // Insert first
        let insert_params = user.to_insert_params().unwrap();
        conn.execute(
            "INSERT INTO test_users (id, name, email) VALUES (?, ?, ?)",
            rusqlite::params_from_iter(insert_params.iter().map(|p| p.as_ref())),
        )
        .unwrap();

        // Delete
        let params = user.to_delete_params().unwrap();
        conn.execute(
            "DELETE FROM test_users WHERE id = ?",
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
        )
        .unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM test_users", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    // ====================
    // Pool::execute tests
    // ====================

    #[tokio::test]
    async fn pool_execute_insert() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

        pool.execute(
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)",
            vec![],
        )
        .await
        .unwrap();

        let result = pool
            .execute(
                "INSERT INTO items (name) VALUES (?)",
                vec![Box::new("widget".to_string())],
            )
            .await
            .unwrap();
        assert_eq!(result.rows_affected, 1);
        assert!(result.last_insert_rowid > 0);

        // Verify the row is actually there
        let name: Option<String> = pool
            .query_one(
                "SELECT name FROM items WHERE id = ?",
                vec![Box::new(result.last_insert_rowid)],
            )
            .await
            .unwrap();
        assert_eq!(name, Some("widget".to_string()));
    }

    #[tokio::test]
    async fn pool_execute_update() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

        pool.execute(
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)",
            vec![],
        )
        .await
        .unwrap();
        pool.execute(
            "INSERT INTO items (name) VALUES (?)",
            vec![Box::new("widget".to_string())],
        )
        .await
        .unwrap();

        let result = pool
            .execute(
                "UPDATE items SET name = ? WHERE name = ?",
                vec![
                    Box::new("gadget".to_string()),
                    Box::new("widget".to_string()),
                ],
            )
            .await
            .unwrap();
        assert_eq!(result.rows_affected, 1);
        // last_insert_rowid is from the prior INSERT, not reset by UPDATE
        // (it's a connection-level value — we just verify rows_affected is correct)
        assert!(result.last_insert_rowid >= 0);
    }

    #[tokio::test]
    async fn pool_execute_delete() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

        pool.execute(
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)",
            vec![],
        )
        .await
        .unwrap();
        pool.execute(
            "INSERT INTO items (name) VALUES (?)",
            vec![Box::new("widget".to_string())],
        )
        .await
        .unwrap();

        let result = pool
            .execute(
                "DELETE FROM items WHERE name = ?",
                vec![Box::new("widget".to_string())],
            )
            .await
            .unwrap();
        assert_eq!(result.rows_affected, 1);

        // Verify row is gone
        let count: Option<i64> = pool
            .query_one("SELECT COUNT(*) FROM items", vec![])
            .await
            .unwrap();
        assert_eq!(count, Some(0));
    }

    #[tokio::test]
    async fn pool_execute_noop_update() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

        pool.execute(
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)",
            vec![],
        )
        .await
        .unwrap();

        // UPDATE that matches no rows
        let result = pool
            .execute(
                "UPDATE items SET name = ? WHERE id = ?",
                vec![Box::new("nope".to_string()), Box::new(999i64)],
            )
            .await
            .unwrap();
        assert_eq!(result.rows_affected, 0);
    }

    /// Builder-applied pragmas must be observable on every checked-out connection.
    /// This is the gap-#3 fix: `PRAGMA foreign_keys` is off-by-default per connection,
    /// which silently breaks `ON DELETE CASCADE` unless the pool sets it.
    #[tokio::test]
    async fn pool_builder_pragma_applied() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("pragma.db");
        let pool = Pool::builder()
            .path(db_path.to_str().unwrap())
            .max_size(4)
            .pragma("foreign_keys", "ON")
            .build()
            .unwrap();

        // Each query_one checks out a fresh-or-idle connection and runs against it.
        // If the pragma were not applied, foreign_keys would be 0 here.
        for i in 0..8 {
            let fk: Option<i64> = pool.query_one("PRAGMA foreign_keys", vec![]).await.unwrap();
            assert_eq!(
                fk, Some(1),
                "iteration {i}: foreign_keys pragma not applied to checked-out connection"
            );
        }
    }
}
