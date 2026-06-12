pub mod error;
pub mod from_row;
pub mod from_sql;
pub mod pool;
pub mod row;

pub use dao_macros::{dao, Entity};

pub use error::Error;
pub use error::Result;
pub use from_row::FromRow;
pub use from_sql::FromSqlColumn;
pub use pool::{Param, Pool};
pub use row::{ColumnValue, Row};

pub use async_trait::async_trait;

#[cfg(test)]
mod tests {
    use crate::error::Error;
    use crate::pool::Pool;

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

        pool.query_all::<i64>(
            "CREATE TABLE items (id INTEGER, name TEXT)",
            vec![],
        )
        .await
        .ok();

        pool.query_all::<i64>(
            "INSERT INTO items (id, name) VALUES (?, ?)",
            vec![Box::new(1i64), Box::new("widget".to_string())],
        )
        .await
        .ok();

        let result: Option<String> = pool
            .query_one(
                "SELECT name FROM items WHERE id = ?",
                vec![Box::new(1i64)],
            )
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
}
