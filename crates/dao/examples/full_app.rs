//! Full app example: multi-entity blog with Users, Posts, concurrent queries, and a Slug newtype.
//!
//! Run with: cargo run --example full_app

use dao::{async_trait, dao, error::Error, row::ColumnValue, Entity, FromSqlColumn, Pool, Result};
/// Set up an in-memory database with schema and sample data.
async fn setup_db() -> Pool {
    let pool = Pool::open(":memory:").unwrap();

    // Set up schema.
    pool.query_all::<i64>(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, username TEXT, display_name TEXT)",
        vec![],
    )
    .await
    .unwrap();

    pool.query_all::<i64>(
        "CREATE TABLE posts (id INTEGER PRIMARY KEY, slug TEXT, author_id INTEGER, title TEXT, body TEXT)",
        vec![],
    )
    .await
    .unwrap();

    // Insert users.
    pool.query_all::<i64>(
        "INSERT INTO users (id, username, display_name) VALUES (?, ?, ?)",
        vec![
            Box::new(1i64),
            Box::new("alice".to_string()),
            Box::new("Alice".to_string()),
        ],
    )
    .await
    .unwrap();

    pool.query_all::<i64>(
        "INSERT INTO users (id, username, display_name) VALUES (?, ?, ?)",
        vec![
            Box::new(2i64),
            Box::new("bob".to_string()),
            Box::new("Bob".to_string()),
        ],
    )
    .await
    .unwrap();

    // Insert posts (one has a NULL body — it's a draft).
    pool.query_all::<i64>(
        "INSERT INTO posts (id, slug, author_id, title, body) VALUES (?, ?, ?, ?, ?)",
        vec![
            Box::new(1i64),
            Box::new("hello-world".to_string()),
            Box::new(1i64),
            Box::new("Hello World".to_string()),
            Box::new("My first post!".to_string()),
        ],
    )
    .await
    .unwrap();

    pool.query_all::<i64>(
        "INSERT INTO posts (id, slug, author_id, title, body) VALUES (?, ?, ?, ?, ?)",
        vec![
            Box::new(2i64),
            Box::new("rust-basics".to_string()),
            Box::new(1i64),
            Box::new("Rust Basics".to_string()),
            Box::new(None::<String>) as dao::Param,
        ],
    )
    .await
    .unwrap();

    pool.query_all::<i64>(
        "INSERT INTO posts (id, slug, author_id, title, body) VALUES (?, ?, ?, ?, ?)",
        vec![
            Box::new(3i64),
            Box::new("bobs-guide".to_string()),
            Box::new(2i64),
            Box::new("Bob's Guide".to_string()),
            Box::new("Welcome!".to_string()),
        ],
    )
    .await
    .unwrap();

    pool
}

/// URL slug newtype.
#[derive(Debug, PartialEq)]
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

#[derive(Debug, Entity)]
struct User {
    id: i64,
    username: String,
    display_name: String,
}

#[derive(Debug, Entity)]
struct Post {
    id: i64,
    slug: Slug,
    author_id: i64,
    title: String,
    body: Option<String>,
}

#[dao]
#[async_trait]
trait UserDao {
    #[query("SELECT id, username, display_name FROM users WHERE id = ?")]
    async fn get_by_id(&self, id: i64) -> Result<Option<User>>;

    #[query("SELECT id, username, display_name FROM users ORDER BY id")]
    async fn get_all(&self) -> Result<Vec<User>>;
}

#[dao]
#[async_trait]
trait PostDao {
    #[query("SELECT id, slug, author_id, title, body FROM posts WHERE slug = ?")]
    async fn get_by_slug(&self, slug: String) -> Result<Option<Post>>;

    #[query("SELECT id, slug, author_id, title, body FROM posts WHERE author_id = ?")]
    async fn get_by_author(&self, author_id: i64) -> Result<Vec<Post>>;

    #[query("SELECT id, slug, author_id, title, body FROM posts ORDER BY id")]
    async fn get_all(&self) -> Result<Vec<Post>>;
}

#[tokio::main]
async fn main() {
    let pool = setup_db().await;
    let user_dao = UserDao::new(pool.clone());
    let post_dao = PostDao::new(pool);

    // Concurrent queries.
    let (users, posts) = tokio::join!(user_dao.get_all(), post_dao.get_all(),);

    let users = users.unwrap();
    let posts = posts.unwrap();

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
        .await
        .unwrap()
        .unwrap();
    assert_eq!(post.slug, Slug("hello-world".to_string()));

    // Option<String> — NULL body.
    let draft = post_dao
        .get_by_slug("rust-basics".to_string())
        .await
        .unwrap()
        .unwrap();
    assert!(draft.body.is_none());

    // Author filter.
    let alice_posts = post_dao.get_by_author(1).await.unwrap();
    assert_eq!(alice_posts.len(), 2);

    println!("\nAll checks passed!");
}
