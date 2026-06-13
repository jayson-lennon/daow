//! Custom types example: demonstrates FromSqlColumn for newtypes and column rename.
//!
//! Uses ProductId and CustomerId newtypes for strongly-typed IDs, plus
//! Cents and Email as domain newtypes.
//!
//! Run with: cargo run --example custom_types

#![allow(dead_code)]
use dao::{
    async_trait, dao, error::Error, row::ColumnValue, Entity, FromSqlColumn, Pool, Result,
    ToSqlColumn,
};

/// Set up an in-memory database with schema and sample data.
async fn setup_db() -> Result<Pool> {
    let pool = Pool::open(":memory:")?;

    pool.query_all::<i64>(
        "CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT, price INTEGER)",
        vec![],
    )
    .await?;

    pool.query_all::<i64>(
        "INSERT INTO products (id, name, price) VALUES (?, ?, ?)",
        vec![
            Box::new(1i64),
            Box::new("Widget".to_string()),
            Box::new(999i64),
        ],
    )
    .await?;

    pool.query_all::<i64>(
        "INSERT INTO products (id, name, price) VALUES (?, ?, ?)",
        vec![
            Box::new(2i64),
            Box::new("Gadget".to_string()),
            Box::new(1499i64),
        ],
    )
    .await?;

    pool.query_all::<i64>(
        "CREATE TABLE customers (id INTEGER PRIMARY KEY, email_address TEXT)",
        vec![],
    )
    .await?;

    pool.query_all::<i64>(
        "INSERT INTO customers (id, email_address) VALUES (?, ?)",
        vec![Box::new(1i64), Box::new("alice@example.com".to_string())],
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

/// Strongly-typed customer ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CustomerId(i64);

impl FromSqlColumn for CustomerId {
    fn from_column(value: &ColumnValue) -> Result<Self> {
        Ok(CustomerId(i64::from_column(value)?))
    }
}

impl ToSqlColumn for CustomerId {
    fn to_column(&self) -> Result<dao::Param> {
        self.0.to_column()
    }
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
    id: ProductId,
    name: String,
    price: Cents,
}

#[derive(Debug, Entity)]
struct Customer {
    id: CustomerId,
    #[dao(column = "email_address")]
    email: Email,
}

#[dao]
#[async_trait]
trait ProductDao {
    #[query("SELECT id, name, price FROM products WHERE id = ?")]
    async fn get_by_id(&self, id: ProductId) -> Result<Option<Product>>;

    #[query("SELECT id, name, price FROM products ORDER BY price")]
    async fn get_all(&self) -> Result<Vec<Product>>;
}

#[dao]
#[async_trait]
trait CustomerDao {
    #[query("SELECT id, email_address FROM customers WHERE id = ?")]
    async fn get_by_id(&self, id: CustomerId) -> Result<Option<Customer>>;
}

#[tokio::main]
async fn main() -> Result<()> {
    let pool = setup_db().await?;
    let products = ProductDao::new(pool.clone());
    let customers = CustomerDao::new(pool);

    // Custom type: Cents
    let widget = products.get_by_id(ProductId(1)).await?.unwrap();
    println!("Product: {:?} — price = {:?}", widget.name, widget.price);
    assert_eq!(widget.price, Cents(999));

    // Custom type: Email with column rename
    let alice = customers.get_by_id(CustomerId(1)).await?.unwrap();
    println!("Customer email: {:?}", alice.email);
    assert_eq!(alice.email, Email("alice@example.com".to_string()));

    // All products ordered by price
    let all = products.get_all().await?;
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].price, Cents(999));
    assert_eq!(all[1].price, Cents(1499));

    println!("\nAll checks passed!");
    Ok(())
}
