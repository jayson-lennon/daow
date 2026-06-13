//! Execute entity expansion: #[execute] with a single entity parameter.
//!
//! When #[execute] has one struct parameter but multiple `?` placeholders,
//! the macro auto-expands the entity's fields via ToRow. A compile-time
//! const assertion verifies the field count matches the placeholder count.
//!
//! Uses a ProductId newtype for strongly-typed IDs.
//!
//! Run with: cargo run --example execute_entity

use dao::{
    async_trait, dao, row::ColumnValue, Entity, ExecuteResult, FromSqlColumn, Pool, Result,
    ToSqlColumn,
};

async fn setup_db() -> Result<Pool> {
    let pool = Pool::open(":memory:")?;
    pool.execute(
        "CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT, price REAL)",
        vec![],
    )
    .await?;
    Ok(pool)
}

/// Strongly-typed product ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProductId(i64);

impl FromSqlColumn for ProductId {
    fn from_column(value: &ColumnValue) -> Result<Self> {
        Ok(ProductId(i64::from_column(value)?))
    }
}

impl ToSqlColumn for ProductId {
    fn to_column(&self) -> Result<dao::Param> {
        self.0.to_column()
    }
}

#[derive(Debug, Clone, PartialEq, Entity)]
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
    #[query("SELECT id, name, price FROM products WHERE id = ?")]
    async fn get(&self, id: ProductId) -> Result<Option<Product>>;

    // Single entity param, 3 placeholders — macro expands via ToRow.
    #[execute("INSERT OR REPLACE INTO products (id, name, price) VALUES (?, ?, ?)")]
    async fn upsert(&self, product: Product) -> Result<ExecuteResult>;
}

#[tokio::main]
async fn main() -> Result<()> {
    let pool = setup_db().await?;
    let dao = ProductDao::new(pool);

    // Insert
    let widget = Product {
        id: ProductId(1),
        name: "Widget".to_string(),
        price: 9.99,
    };
    let result = dao.upsert(widget.clone()).await?;
    println!("Inserted: rows_affected={}", result.rows_affected);

    // Verify
    let fetched = dao.get(ProductId(1)).await?.unwrap();
    println!("Found: {:?}", fetched);
    assert_eq!(fetched, widget);

    // Upsert (replace) with new price
    let updated = Product {
        id: ProductId(1),
        name: "Widget".to_string(),
        price: 19.99,
    };
    dao.upsert(updated.clone()).await?;

    let after = dao.get(ProductId(1)).await?.unwrap();
    println!("After upsert: {:?}", after);
    assert_eq!(after.price, 19.99);

    println!("\nAll checks passed!");
    Ok(())
}
