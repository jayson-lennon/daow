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
         CREATE TABLE blog_articles (id INTEGER PRIMARY KEY, author_id INTEGER, title TEXT, body TEXT);
         CREATE TABLE widgets (id INTEGER PRIMARY KEY, name TEXT);
         -- Tables for #[upsert] tests (Phase 2).
         -- `children` references widgets with ON DELETE CASCADE so upsert tests can
         -- assert that non-destructive ON CONFLICT DO UPDATE does NOT cascade-delete children
         -- (unlike INSERT OR REPLACE / REPLACE which would).
         CREATE TABLE children (id INTEGER PRIMARY KEY, parent_id INTEGER NOT NULL REFERENCES widgets(id) ON DELETE CASCADE);
         -- All-PK junction table to exercise the DO NOTHING upsert fallback.
         CREATE TABLE junctions (a INTEGER NOT NULL, b INTEGER NOT NULL, PRIMARY KEY(a, b));"
    )
    .expect("Failed to create schema");
}
