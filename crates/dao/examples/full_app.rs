//! Full app example: multi-entity blog with Users, Posts, writes, concurrent queries, and newtypes.
//!
//! Demonstrates UserId, PostId as strongly-typed IDs, and a Slug newtype with
//! symmetric FromSqlColumn / ToSqlColumn for validated read/write.
//!
//! Run with: cargo run --example full_app

use dao::{
    async_trait, dao, error::Error, row::ColumnValue, Entity, ExecuteResult, FromSqlColumn, Pool,
    Result, ToSqlColumn,
};

/// Set up an in-memory database with schema.
async fn setup_db() -> Result<Pool> {
    // Plain `:memory:` — the pool forces max_size=1 so all checkouts share one DB.
    let pool = Pool::open(":memory:")?;

    // Set up schema.
    pool.execute(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, username TEXT, display_name TEXT)",
        vec![],
    )
    .await?;

    pool.execute(
        "CREATE TABLE posts (id INTEGER PRIMARY KEY, slug TEXT, author_id INTEGER, title TEXT, body TEXT)",
        vec![],
    )
    .await?;

    Ok(pool)
}

/// Strongly-typed user ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UserId(i64);

impl FromSqlColumn for UserId {
    fn from_column(value: &ColumnValue) -> Result<Self> {
        Ok(UserId(i64::from_column(value)?))
    }
}

impl ToSqlColumn for UserId {
    fn to_column(&self) -> Result<dao::Param> {
        self.0.to_column()
    }
}

/// Strongly-typed post ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PostId(i64);

impl FromSqlColumn for PostId {
    fn from_column(value: &ColumnValue) -> Result<Self> {
        Ok(PostId(i64::from_column(value)?))
    }
}

impl ToSqlColumn for PostId {
    fn to_column(&self) -> Result<dao::Param> {
        self.0.to_column()
    }
}

/// URL slug newtype with symmetric FromSqlColumn / ToSqlColumn.
#[derive(Debug, Clone, PartialEq)]
struct Slug(String);

impl FromSqlColumn for Slug {
    fn from_column(value: &ColumnValue) -> Result<Self> {
        let s = String::from_column(value)?;
        if s.chars()
            .all(|c| c.is_ascii_lowercase() || c == '-' || c.is_ascii_digit())
        {
            Ok(Slug(s))
        } else {
            Err(Error::custom(format!("invalid slug: {s}")))
        }
    }
}

impl ToSqlColumn for Slug {
    fn to_column(&self) -> Result<dao::Param> {
        self.0.to_column()
    }
}

#[derive(Debug, Clone, Entity)]
#[dao(table = "users")]
struct User {
    #[dao(pk)]
    id: UserId,
    username: String,
    display_name: String,
}

#[derive(Debug, Clone, Entity)]
#[dao(table = "posts")]
struct Post {
    #[dao(pk)]
    id: PostId,
    slug: Slug,
    author_id: UserId,
    title: String,
    body: Option<String>,
}

#[dao]
#[async_trait]
#[allow(dead_code)]
trait UserDao {
    #[query("SELECT id, username, display_name FROM users WHERE id = ?")]
    async fn get_by_id(&self, id: UserId) -> Result<Option<User>>;

    #[query("SELECT id, username, display_name FROM users ORDER BY id")]
    async fn get_all(&self) -> Result<Vec<User>>;

    #[insert]
    async fn insert(&self, user: User) -> Result<ExecuteResult>;
}

#[dao]
#[async_trait]
trait PostDao {
    #[query("SELECT id, slug, author_id, title, body FROM posts WHERE slug = ?")]
    async fn get_by_slug(&self, slug: String) -> Result<Option<Post>>;

    #[query("SELECT id, slug, author_id, title, body FROM posts WHERE author_id = ?")]
    async fn get_by_author(&self, author_id: UserId) -> Result<Vec<Post>>;

    #[query("SELECT id, slug, author_id, title, body FROM posts ORDER BY id")]
    async fn get_all(&self) -> Result<Vec<Post>>;

    #[insert]
    async fn insert(&self, post: Post) -> Result<ExecuteResult>;
}

#[tokio::main]
async fn main() -> Result<()> {
    let pool = setup_db().await?;
    let user_dao = UserDao::new(pool.clone());
    let post_dao = PostDao::new(pool);

    // Insert users via the DAO.
    user_dao
        .insert(User {
            id: UserId(1),
            username: "alice".to_string(),
            display_name: "Alice".to_string(),
        })
        .await?;

    user_dao
        .insert(User {
            id: UserId(2),
            username: "bob".to_string(),
            display_name: "Bob".to_string(),
        })
        .await?;

    // Insert posts via the DAO.
    post_dao
        .insert(Post {
            id: PostId(1),
            slug: Slug("hello-world".to_string()),
            author_id: UserId(1),
            title: "Hello World".to_string(),
            body: Some("My first post!".to_string()),
        })
        .await?;

    post_dao
        .insert(Post {
            id: PostId(2),
            slug: Slug("rust-basics".to_string()),
            author_id: UserId(1),
            title: "Rust Basics".to_string(),
            body: None, // draft
        })
        .await?;

    post_dao
        .insert(Post {
            id: PostId(3),
            slug: Slug("bobs-guide".to_string()),
            author_id: UserId(2),
            title: "Bob's Guide".to_string(),
            body: Some("Welcome!".to_string()),
        })
        .await?;

    // Concurrent queries.
    let (users, posts) = tokio::try_join!(user_dao.get_all(), post_dao.get_all())?;

    println!("Users:");
    for u in &users {
        println!("  {} ({})", u.display_name, u.username);
    }

    println!("\nPosts:");
    for p in &posts {
        let body_status = match &p.body {
            Some(b) => b.as_str(),
            None => "<draft>",
        };
        println!("  [{}] {} — {}", p.slug.0, p.title, body_status);
    }

    // Verify.
    assert_eq!(users.len(), 2);
    assert_eq!(posts.len(), 3);

    // Slug newtype works.
    let post = post_dao
        .get_by_slug("hello-world".to_string())
        .await?
        .unwrap();
    assert_eq!(post.slug, Slug("hello-world".to_string()));

    // Option<String> — NULL body.
    let draft = post_dao
        .get_by_slug("rust-basics".to_string())
        .await?
        .unwrap();
    assert!(draft.body.is_none());

    // Author filter — uses UserId newtype.
    let alice_posts = post_dao.get_by_author(UserId(1)).await?;
    assert_eq!(alice_posts.len(), 2);

    println!("\nAll checks passed!");
    Ok(())
}
