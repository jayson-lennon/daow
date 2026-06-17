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

//! Demonstrates destructured entity params in #[execute] and #[query].
//!
//! Run with: cargo run --example destructured

use dao::{async_trait, dao, Entity, ExecuteResult, FromSqlColumn, Pool, Result, ToSqlColumn};

/// Newtype for product IDs.
#[derive(Debug, Clone, PartialEq)]
struct ProductId(i64);

impl FromSqlColumn for ProductId {
    fn from_column(value: &dao::ColumnValue) -> Result<Self> {
        Ok(ProductId(i64::from_column(value)?))
    }
}

impl ToSqlColumn for ProductId {
    fn to_column(&self) -> Result<dao::Param> {
        self.0.to_column()
    }
}

#[derive(Debug, Clone, Entity)]
#[dao(table = "products")]
struct Product {
    #[dao(pk)]
    id: ProductId,
    name: String,
    price: f64,
}

#[dao]
#[async_trait]
trait ProductDao {
    // --- Scalar params (existing) ---

    #[query("SELECT id, name, price FROM products WHERE id = ?")]
    async fn get(&self, id: ProductId) -> Result<Option<Product>>;

    // --- Destructured params ---

    // Partial: pick only name and price for a targeted update
    #[execute("UPDATE products SET name = ?, price = ? WHERE id = ?")]
    async fn rename_and_reprice(
        &self,
        Product { name, price, id, .. }: Product,
    ) -> Result<ExecuteResult>;

    // Complete destructuring: all fields explicitly listed
    #[execute("UPDATE products SET name = ?, price = ? WHERE id = ?")]
    async fn set_all(&self, Product { name, price, id }: Product) -> Result<ExecuteResult>;

    // Destructured entity + scalar: conditional update
    #[execute("UPDATE products SET name = ? WHERE price > ? AND id = ?")]
    async fn rename_if_cheap(
        &self,
        Product { name, id, .. }: Product,
        max_price: f64,
    ) -> Result<ExecuteResult>;

    // Destructured in a query: search by name and price
    #[query("SELECT id, name, price FROM products WHERE name = ? AND price = ?")]
    async fn find_by_name_and_price(
        &self,
        Product { name, price, .. }: Product,
    ) -> Result<Vec<Product>>;
}

async fn setup_db() -> Pool {
    let pool = Pool::open(":memory:").unwrap();

    pool.execute(
        "CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT, price REAL)",
        vec![],
    )
    .await
    .unwrap();

    pool
}

#[tokio::main]
async fn main() -> Result<()> {
    let pool = setup_db().await;
    let dao = ProductDao::new(pool.clone());

    // Insert some products directly
    let widget = Product {
        id: ProductId(1),
        name: "Widget".into(),
        price: 9.99,
    };
    let gadget = Product {
        id: ProductId(2),
        name: "Gadget".into(),
        price: 24.99,
    };
    let gizmo = Product {
        id: ProductId(3),
        name: "Gizmo".into(),
        price: 4.99,
    };

    for p in [&widget, &gadget, &gizmo] {
        pool.execute(
            "INSERT INTO products (id, name, price) VALUES (?, ?, ?)",
            dao::ToRow::to_insert_params(p)?,
        )
        .await?;
    }

    // --- Partial destructuring: rename and reprice ---
    let updated = Product {
        id: ProductId(1),
        name: "Super Widget".into(),
        price: 14.99,
    };
    let result = dao.rename_and_reprice(updated).await?;
    println!("rename_and_reprice: {} rows affected", result.rows_affected);

    let fetched = dao.get(ProductId(1)).await?.unwrap();
    println!("  -> {:?}\n", fetched);

    // --- Complete destructuring ---
    let result = dao
        .set_all(Product {
            id: ProductId(2),
            name: "Mega Gadget".into(),
            price: 29.99,
        })
        .await?;
    println!("set_all: {} rows affected", result.rows_affected);

    let fetched = dao.get(ProductId(2)).await?.unwrap();
    println!("  -> {:?}\n", fetched);

    // --- Destructured + scalar: rename if cheap ---
    let result = dao
        .rename_if_cheap(
            Product {
                id: ProductId(3),
                name: "Budget Gizmo".into(),
                price: 4.99,
            },
        3.0, // only update if price > 3.0 (Gizmo is 4.99, so it qualifies)
        )
        .await?;
    println!("rename_if_cheap: {} rows affected", result.rows_affected);

    let fetched = dao.get(ProductId(3)).await?.unwrap();
    println!("  -> {:?}\n", fetched);

    // --- Destructured in a query ---
    let search = Product {
        id: ProductId(0),
        name: "Super Widget".into(),
        price: 14.99,
    };
    let found = dao.find_by_name_and_price(search).await?;
    println!("find_by_name_and_price: found {} match(es)", found.len());
    for p in &found {
        println!("  -> {:?}", p);
    }

    Ok(())
}
