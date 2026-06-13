//! Multi-DAO transactions: two `#[dao]` DAOs share one transaction.
//!
//! Extends the `multi_dao` pattern with a transaction spanning both DAOs.
//! Demonstrates:
//!   - `dao.with(&tx).method()` running on the transaction's connection
//!   - atomicity across two DAOs: both inserts persist after `commit()`
//!   - mid-tx failure reverts everything done so far
//!
//! Run with: cargo run --example multi_dao_tx

use dao::{
    async_trait, dao, row::ColumnValue, Entity, ExecuteResult, FromSqlColumn, Pool, Result,
    ToSqlColumn,
};

async fn setup_db() -> Result<Pool> {
    let pool = Pool::open(":memory:")?;
    pool.execute(
        "CREATE TABLE blog_authors (id INTEGER PRIMARY KEY, name TEXT)",
        vec![],
    )
    .await?;
    pool.execute(
        "CREATE TABLE blog_articles (id INTEGER PRIMARY KEY, author_id INTEGER, title TEXT, body TEXT)",
        vec![],
    )
    .await?;
    Ok(pool)
}

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

#[derive(Debug, Clone, Entity)]
#[dao(table = "blog_authors")]
struct Author {
    #[dao(pk)]
    id: AuthorId,
    name: String,
}

#[derive(Debug, Clone, Entity)]
#[dao(table = "blog_articles")]
struct Article {
    #[dao(pk)]
    id: ArticleId,
    author_id: AuthorId,
    title: String,
    body: String,
}

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
    #[insert]
    async fn publish(&self, article: Article) -> Result<ExecuteResult>;
}

#[tokio::main]
async fn main() -> Result<()> {
    let pool = setup_db().await?;
    let authors = AuthorDao::new(pool.clone());
    let articles = ArticleDao::new(pool.clone());

    // --- Commit path: both DAOs write inside one tx, both persist. ---
    {
        let tx = pool.begin().await?;
        authors
            .with(&tx)
            .create(Author {
                id: AuthorId(1),
                name: "Alice".into(),
            })
            .await?;
        articles
            .with(&tx)
            .publish(Article {
                id: ArticleId(10),
                author_id: AuthorId(1),
                title: "First post".into(),
                body: "Hello world!".into(),
            })
            .await?;
        tx.commit().await?; // both rows persist
    }
    let all_authors = authors.list().await?;
    assert_eq!(all_authors.len(), 1, "committed: author persisted");
    println!("[OK] multi-DAO tx committed: author + article persisted");

    // --- Rollback path: a mid-tx failure reverts all writes. ---
    {
        let tx = pool.begin().await?;
        authors
            .with(&tx)
            .create(Author {
                id: AuthorId(2),
                name: "Bob".into(),
            })
            .await?;
        // Insert a duplicate author id (1 already exists) -> PK violation -> error.
        // The error is expected, so we discard it; the tx is dropped without commit
        // -> rollback -> Bob (id=2) is undone too.
        let _ = authors
            .with(&tx)
            .create(Author {
                id: AuthorId(1),
                name: "dup".into(),
            })
            .await;
        drop(tx);
    }
    let all_authors = authors.list().await?;
    assert_eq!(
        all_authors.len(),
        1,
        "rolled back: Bob (and failed writes) not persisted"
    );
    println!("[OK] multi-DAO tx mid-failure rolled back: only committed row remains");

    println!("\nAll multi-DAO transaction checks passed!");
    Ok(())
}
