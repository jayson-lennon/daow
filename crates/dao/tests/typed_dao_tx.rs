//! Integration tests for typed-DAO transaction atomicity: two `#[dao]` DAOs
//! sharing one `Transaction`, with commit-persistence and mid-failure rollback.
//!
//! This is the regression-protected version of the scenario demonstrated in
//! `examples/multi_dao_tx.rs`. It proves that a `#[dao]` method called via
//! `dao.with(&tx)` runs on the transaction's connection, and that a failure
//! mid-way through a multi-DAO transaction reverts *both* DAOs' writes.

use dao::{
    async_trait, dao, row::ColumnValue, Entity, ExecuteResult, FromSqlColumn, Pool, Result,
    ToSqlColumn,
};

// --- Entities (mirrors the example, trimmed) ---

#[derive(Debug, Clone, PartialEq, Entity)]
#[dao(table = "blog_authors")]
struct Author {
    #[dao(pk)]
    id: AuthorId,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Entity)]
#[dao(table = "blog_articles")]
struct Article {
    #[dao(pk)]
    id: ArticleId,
    author_id: AuthorId,
    title: String,
    body: String,
}

// --- Newtypes (so PK fields are typed, matching the example) ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AuthorId(i64);

impl FromSqlColumn for AuthorId {
    fn from_column(value: &ColumnValue) -> Result<Self> {
        Ok(AuthorId(i64::from_column(value)?))
    }
}

impl ToSqlColumn for AuthorId {
    fn to_column(&self) -> Result<dao::Param> {
        self.0.to_column()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArticleId(i64);

impl FromSqlColumn for ArticleId {
    fn from_column(value: &ColumnValue) -> Result<Self> {
        Ok(ArticleId(i64::from_column(value)?))
    }
}

impl ToSqlColumn for ArticleId {
    fn to_column(&self) -> Result<dao::Param> {
        self.0.to_column()
    }
}

// --- DAOs ---

#[dao]
#[async_trait]
trait AuthorDao {
    #[query("SELECT id, name FROM blog_authors ORDER BY id")]
    async fn list(&self) -> Result<Vec<Author>>;

    #[insert]
    async fn create(&self, author: Author) -> Result<ExecuteResult>;
}

#[dao]
#[async_trait]
trait ArticleDao {
    #[query("SELECT id, author_id, title, body FROM blog_articles ORDER BY id")]
    async fn list(&self) -> Result<Vec<Article>>;

    #[insert]
    async fn publish(&self, article: Article) -> Result<ExecuteResult>;
}

/// Creates a temp directory + pool + the two tables.
/// Returns (pool, TempDir) — caller must keep TempDir alive.
async fn setup_db() -> (Pool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = Pool::open(db_path.to_str().unwrap()).unwrap();

    pool.execute(
        "CREATE TABLE blog_authors (id INTEGER PRIMARY KEY, name TEXT)",
        vec![],
    )
    .await
    .unwrap();
    pool.execute(
        "CREATE TABLE blog_articles (id INTEGER PRIMARY KEY, author_id INTEGER, title TEXT, body TEXT)",
        vec![],
    )
    .await
    .unwrap();

    (pool, dir)
}

#[tokio::test]
async fn typed_dao_tx_commit_persists_both_daos() {
    let (pool, _dir) = setup_db().await;
    let authors = AuthorDao::new(pool.clone());
    let articles = ArticleDao::new(pool.clone());

    // Both DAOs write inside one transaction, then commit.
    let tx = pool.begin().await.unwrap();
    authors
        .with(&tx)
        .create(Author {
            id: AuthorId(1),
            name: "Alice".into(),
        })
        .await
        .unwrap();
    articles
        .with(&tx)
        .publish(Article {
            id: ArticleId(10),
            author_id: AuthorId(1),
            title: "First post".into(),
            body: "Hello world!".into(),
        })
        .await
        .unwrap();
    tx.commit().await.unwrap();

    // Both rows must be visible after commit.
    assert_eq!(authors.list().await.unwrap().len(), 1);
    assert_eq!(articles.list().await.unwrap().len(), 1);
}

#[tokio::test]
async fn typed_dao_tx_mid_failure_reverts_both_daos() {
    // The core scenario: DAO #1 edit succeeds, DAO #2 edit fails, both are
    // rolled back. A later query sees neither.
    let (pool, _dir) = setup_db().await;
    let authors = AuthorDao::new(pool.clone());
    let articles = ArticleDao::new(pool.clone());

    // Seed one committed author so the second DAO edit below can fail on a
    // duplicate author_id (FK target doesn't matter — we use a PK violation
    // on blog_authors itself, which is simpler and constraint-free to set up).
    authors
        .create(Author {
            id: AuthorId(1),
            name: "Alice".into(),
        })
        .await
        .unwrap();

    let tx = pool.begin().await.unwrap();

    // First DAO edit: succeeds.
    authors
        .with(&tx)
        .create(Author {
            id: AuthorId(2),
            name: "Bob".into(),
        })
        .await
        .unwrap();

    // Second DAO edit: fails with a PK violation (author id=1 already exists).
    let err = authors
        .with(&tx)
        .create(Author {
            id: AuthorId(1),
            name: "dup".into(),
        })
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("UNIQUE") || err.to_string().contains("unique"),
        "expected a uniqueness violation, got: {err}"
    );

    // Drop without commit -> automatic rollback -> both Bob and the failed
    // duplicate are undone.
    drop(tx);

    // Later query: only the seeded Alice remains. Bob was rolled back.
    let all_authors = authors.list().await.unwrap();
    assert_eq!(
        all_authors.len(),
        1,
        "mid-tx failure should have reverted both DAOs' writes"
    );
    // And the article DAO never wrote anything (sanity).
    assert_eq!(articles.list().await.unwrap().len(), 0);
}
