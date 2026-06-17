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

//! Writes example: demonstrates all write annotations — #[insert], #[update], #[delete], #[execute].
//!
//! Uses a strongly-typed ItemId newtype for primary keys.
//!
//! Run with: cargo run --example writes

use dao::{
    async_trait, dao, row::ColumnValue, Entity, ExecuteResult, FromSqlColumn, Pool, Result,
    ToSqlColumn,
};

async fn setup_db() -> Result<Pool> {
    let pool = Pool::open(":memory:")?;

    pool.execute(
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, price REAL)",
        vec![],
    )
    .await?;

    Ok(pool)
}

/// Strongly-typed item ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ItemId(i64);

impl FromSqlColumn for ItemId {
    fn from_column(value: &ColumnValue) -> Result<Self> {
        Ok(ItemId(i64::from_column(value)?))
    }
}

impl ToSqlColumn for ItemId {
    fn to_column(&self) -> Result<dao::Param> {
        self.0.to_column()
    }
}

#[derive(Debug, Clone, Entity)]
#[dao(table = "items")]
struct Item {
    #[dao(pk)]
    id: ItemId,
    name: String,
    price: f64,
}

#[dao]
#[async_trait]
trait ItemDao {
    // --- Generated SQL annotations ---
    #[insert]
    async fn insert(&self, item: Item) -> Result<ExecuteResult>;

    #[update]
    async fn update(&self, item: Item) -> Result<ExecuteResult>;

    #[delete]
    async fn delete(&self, item: Item) -> Result<ExecuteResult>;

    // --- User-provided SQL via #[execute] ---
    #[execute("UPDATE items SET price = ? WHERE id = ?")]
    async fn set_price(&self, price: f64, id: ItemId) -> Result<ExecuteResult>;

    #[execute("DELETE FROM items")]
    async fn delete_all(&self) -> Result<ExecuteResult>;

    // --- Read ---
    #[query("SELECT id, name, price FROM items WHERE id = ?")]
    async fn get_by_id(&self, id: ItemId) -> Result<Option<Item>>;

    #[query("SELECT id, name, price FROM items ORDER BY id")]
    async fn get_all(&self) -> Result<Vec<Item>>;

    #[query("SELECT COUNT(*) FROM items")]
    async fn count(&self) -> Result<i64>;
}

#[tokio::main]
async fn main() -> Result<()> {
    let pool = setup_db().await?;
    let dao = ItemDao::new(pool);

    // --- #[insert]: generated SQL ---
    let r = dao
        .insert(Item {
            id: ItemId(1),
            name: "Widget".to_string(),
            price: 9.99,
        })
        .await?;
    println!(
        "Insert: rows_affected={}, last_insert_rowid={}",
        r.rows_affected, r.last_insert_rowid
    );

    dao.insert(Item {
        id: ItemId(2),
        name: "Gadget".to_string(),
        price: 14.99,
    })
    .await?;

    dao.insert(Item {
        id: ItemId(3),
        name: "Doohickey".to_string(),
        price: 0.0,
    })
    .await?;

    println!("\nAfter 3 inserts:");
    for item in dao.get_all().await? {
        println!("  {} @${:.2}", item.name, item.price);
    }

    // --- #[update]: generated SQL ---
    let r = dao
        .update(Item {
            id: ItemId(1),
            name: "Widget Pro".to_string(),
            price: 19.99,
        })
        .await?;
    println!("\nUpdate: rows_affected={}", r.rows_affected);

    let updated = dao.get_by_id(ItemId(1)).await?.unwrap();
    println!("  Updated: {} @${:.2}", updated.name, updated.price);

    // --- #[execute]: user-provided SQL ---
    let r = dao.set_price(99.99, ItemId(2)).await?;
    println!("\nExecute (set_price): rows_affected={}", r.rows_affected);

    let gadget = dao.get_by_id(ItemId(2)).await?.unwrap();
    assert_eq!(gadget.price, 99.99);
    println!("  Gadget price is now ${:.2}", gadget.price);

    // --- #[delete]: generated SQL ---
    let r = dao
        .delete(Item {
            id: ItemId(3),
            name: "Doohickey".to_string(),
            price: 0.0,
        })
        .await?;
    println!("\nDelete: rows_affected={}", r.rows_affected);
    assert!(dao.get_by_id(ItemId(3)).await?.is_none());

    // --- #[execute]: delete all ---
    let r = dao.delete_all().await?;
    println!("\nDelete all: rows_affected={}", r.rows_affected);
    assert_eq!(dao.count().await?, 0);

    println!("\nAll write annotations demonstrated!");
    Ok(())
}
