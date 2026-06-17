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

//! Multiple DAOs sharing a pool: a mini blog with cross-entity writes.
//!
//! Two DAOs (AuthorDao, ArticleDao) backed by the same Pool. Demonstrates
//! insert, update, delete, and query across related entities.
//!
//! Uses AuthorId and ArticleId newtypes for strongly-typed IDs.
//!
//! Run with: cargo run --example multi_dao

use daow::{
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

/// Strongly-typed author ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AuthorId(i64);

impl FromSqlColumn for AuthorId {
    fn from_column(value: &ColumnValue) -> Result<Self> {
        Ok(AuthorId(i64::from_column(value)?))
    }
}

impl ToSqlColumn for AuthorId {
    fn to_column(&self) -> Result<daow::Param> {
        self.0.to_column()
    }
}

/// Strongly-typed article ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArticleId(i64);

impl FromSqlColumn for ArticleId {
    fn from_column(value: &ColumnValue) -> Result<Self> {
        Ok(ArticleId(i64::from_column(value)?))
    }
}

impl ToSqlColumn for ArticleId {
    fn to_column(&self) -> Result<daow::Param> {
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
#[allow(dead_code)]
trait AuthorDao {
    #[query("SELECT id, name FROM blog_authors WHERE id = ?")]
    async fn get(&self, id: AuthorId) -> Result<Option<Author>>;

    #[query("SELECT id, name FROM blog_authors ORDER BY id")]
    async fn list(&self) -> Result<Vec<Author>>;

    #[insert]
    async fn create(&self, author: Author) -> Result<ExecuteResult>;

    #[delete]
    async fn delete(&self, author: Author) -> Result<ExecuteResult>;
}

#[dao]
#[async_trait]
trait ArticleDao {
    #[query("SELECT id, author_id, title, body FROM blog_articles WHERE author_id = ?")]
    async fn by_author(&self, author_id: AuthorId) -> Result<Vec<Article>>;

    #[insert]
    async fn publish(&self, article: Article) -> Result<ExecuteResult>;

    #[update]
    async fn edit(&self, article: Article) -> Result<ExecuteResult>;

    #[execute("DELETE FROM blog_articles WHERE author_id = ?")]
    async fn delete_by_author(&self, author_id: AuthorId) -> Result<ExecuteResult>;
}

#[tokio::main]
async fn main() -> Result<()> {
    let pool = setup_db().await?;
    let authors = AuthorDao::new(pool.clone());
    let articles = ArticleDao::new(pool);

    // Create authors
    authors.create(Author { id: AuthorId(1), name: "Alice".into() }).await?;
    authors.create(Author { id: AuthorId(2), name: "Bob".into() }).await?;

    // Publish articles
    articles.publish(Article {
        id: ArticleId(1),
        author_id: AuthorId(1),
        title: "First post".into(),
        body: "Hello world!".into(),
    }).await?;

    articles.publish(Article {
        id: ArticleId(2),
        author_id: AuthorId(1),
        title: "Rust tips".into(),
        body: "Use cargo clippy.".into(),
    }).await?;

    articles.publish(Article {
        id: ArticleId(3),
        author_id: AuthorId(2),
        title: "Bob here".into(),
        body: "Hi everyone.".into(),
    }).await?;

    // List Alice's articles
    let alice_posts = articles.by_author(AuthorId(1)).await?;
    println!("Alice's articles:");
    for a in &alice_posts {
        println!("  [{}] {}", a.id.0, a.title);
    }
    assert_eq!(alice_posts.len(), 2);

    // Edit an article
    articles.edit(Article {
        id: ArticleId(1),
        author_id: AuthorId(1),
        title: "First post (edited)".into(),
        body: "Updated content.".into(),
    }).await?;

    // Delete Bob's articles then delete Bob
    let deleted = articles.delete_by_author(AuthorId(2)).await?;
    println!("\nDeleted {} article(s) by Bob", deleted.rows_affected);
    assert_eq!(deleted.rows_affected, 1);

    authors.delete(Author { id: AuthorId(2), name: "Bob".into() }).await?;
    let remaining = authors.list().await?;
    assert_eq!(remaining.len(), 1);
    println!("Remaining authors: {:?}", remaining);

    println!("\nAll checks passed!");
    Ok(())
}
