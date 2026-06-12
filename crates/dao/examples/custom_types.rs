//! Custom types example: demonstrates FromSqlColumn for newtypes and column rename.
//!
//! Run with: cargo run --example custom_types

use dao::{async_trait, dao, error::Error, row::ColumnValue, Entity, FromSqlColumn, Pool, Result};

/// Set up an in-memory database with schema and sample data.
async fn setup_db() -> Pool {
    let pool = Pool::open(":memory:").unwrap();

    pool.query_all::<i64>(
        "CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT, price INTEGER)",
        vec![],
    )
    .await
    .unwrap();

    pool.query_all::<i64>(
        "INSERT INTO products (id, name, price) VALUES (?, ?, ?)",
        vec![
            Box::new(1i64),
            Box::new("Widget".to_string()),
            Box::new(999i64),
        ],
    )
    .await
    .unwrap();

    pool.query_all::<i64>(
        "INSERT INTO products (id, name, price) VALUES (?, ?, ?)",
        vec![
            Box::new(2i64),
            Box::new("Gadget".to_string()),
            Box::new(1499i64),
        ],
    )
    .await
    .unwrap();

    pool.query_all::<i64>(
        "CREATE TABLE customers (id INTEGER PRIMARY KEY, email_address TEXT)",
        vec![],
    )
    .await
    .unwrap();

    pool.query_all::<i64>(
        "INSERT INTO customers (id, email_address) VALUES (?, ?)",
        vec![Box::new(1i64), Box::new("alice@example.com".to_string())],
    )
    .await
    .unwrap();

    pool
}

/// A validated email newtype.
#[derive(Debug, PartialEq)]
struct Email(String);

impl FromSqlColumn for Email {
    fn from_column(value: &ColumnValue) -> Result<Self> {
        let s = String::from_column(value)?;
        if s.contains('@') {
            Ok(Email(s))
        } else {
            Err(Error::custom(format!("invalid email: {s}")))
        }
    }
}

/// A cents newtype — wraps monetary amounts as integers.
#[derive(Debug, PartialEq)]
struct Cents(i64);

impl FromSqlColumn for Cents {
    fn from_column(value: &ColumnValue) -> Result<Self> {
        let n = i64::from_column(value)?;
        Ok(Cents(n))
    }
}

#[derive(Debug, Entity)]
struct Product {
    id: i64,
    name: String,
    price: Cents,
}

#[derive(Debug, Entity)]
struct Customer {
    id: i64,
    #[dao(column = "email_address")]
    email: Email,
}

#[dao]
#[async_trait]
trait ProductDao {
    #[query("SELECT id, name, price FROM products WHERE id = ?")]
    async fn get_by_id(&self, id: i64) -> Result<Option<Product>>;

    #[query("SELECT id, name, price FROM products ORDER BY price")]
    async fn get_all(&self) -> Result<Vec<Product>>;
}

#[dao]
#[async_trait]
trait CustomerDao {
    #[query("SELECT id, email_address FROM customers WHERE id = ?")]
    async fn get_by_id(&self, id: i64) -> Result<Option<Customer>>;
}

#[tokio::main]
async fn main() {
    let pool = setup_db().await;
    let products = ProductDao::new(pool.clone());
    let customers = CustomerDao::new(pool);

    // Custom type: Cents
    let widget = products.get_by_id(1).await.unwrap().unwrap();
    println!("Product: {:?} — price = {:?}", widget.name, widget.price);
    assert_eq!(widget.price, Cents(999));

    // Custom type: Email with column rename
    let alice = customers.get_by_id(1).await.unwrap().unwrap();
    println!("Customer email: {:?}", alice.email);
    assert_eq!(alice.email, Email("alice@example.com".to_string()));

    // All products ordered by price
    let all = products.get_all().await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].price, Cents(999));
    assert_eq!(all[1].price, Cents(1499));

    println!("\nAll checks passed!");
}
