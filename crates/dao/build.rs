use std::path::PathBuf;

fn main() {
    let db_dir = PathBuf::from("tests/db");
    let db_path = db_dir.join("test.db");

    // Create the directory if it doesn't exist
    std::fs::create_dir_all(&db_dir).expect("Failed to create tests/db directory");

    // Always recreate the DB to ensure schema is up to date
    if db_path.exists() {
        std::fs::remove_file(&db_path).expect("Failed to remove old test.db");
    }

    let conn = rusqlite::Connection::open(&db_path)
        .unwrap_or_else(|e| panic!("Failed to create {}: {e}", db_path.display()));

    // Tables used by compile-time #[query] validation (tests + examples + compile-fail).
    // NOTE: items needs price column for entity_derive tests.
    // Some tables are also created at runtime in :memory: DBs by their owning unit, but the
    // proc-macro validates SQL against THIS build-time DB, so every referenced table must exist here.
    conn.execute_batch(
        "CREATE TABLE recalls (id INTEGER PRIMARY KEY, name TEXT);
         CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, price REAL);
         CREATE TABLE opt_items (id INTEGER PRIMARY KEY, name TEXT, price REAL);
         CREATE TABLE renamed (id INTEGER PRIMARY KEY, item_name TEXT);
         CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT, display_name TEXT, username TEXT);
         CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT, price INTEGER);
         CREATE TABLE customers (id INTEGER PRIMARY KEY, email_address TEXT);
         CREATE TABLE posts (id INTEGER PRIMARY KEY, slug TEXT, author_id INTEGER, title TEXT, body TEXT);
         CREATE TABLE articles (id INTEGER PRIMARY KEY, slug TEXT, title TEXT);
         CREATE TABLE accounts (id INTEGER PRIMARY KEY, email TEXT, balance INTEGER);
         CREATE TABLE blog_authors (id INTEGER PRIMARY KEY, name TEXT);
         CREATE TABLE blog_articles (id INTEGER PRIMARY KEY, author_id INTEGER, title TEXT, body TEXT);"
    )
    .expect("Failed to create schema");

    // Tell cargo to rerun if the build.rs changes
    println!("cargo:rerun-if-changed=build.rs");
}
