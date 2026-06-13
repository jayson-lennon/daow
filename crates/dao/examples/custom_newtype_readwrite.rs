//! Custom newtypes with symmetric read/write.
//!
//! Demonstrates implementing both FromSqlColumn and ToSqlColumn for
//! validated newtypes. The type enforces invariants on both read and write.
//!
//! Uses AccountId for strongly-typed IDs, plus Email and Cents newtypes.
//!
//! Run with: cargo run --example custom_newtype_readwrite

use dao::{
    async_trait, dao, error::Error, row::ColumnValue, Entity, ExecuteResult, FromSqlColumn, Pool,
    Result, ToSqlColumn,
};

async fn setup_db() -> Result<Pool> {
    let pool = Pool::open(":memory:")?;
    pool.execute(
        "CREATE TABLE accounts (id INTEGER PRIMARY KEY, email TEXT, balance INTEGER)",
        vec![],
    )
    .await?;
    Ok(pool)
}

/// Strongly-typed account ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AccountId(i64);

impl FromSqlColumn for AccountId {
    fn from_column(value: &ColumnValue) -> Result<Self> {
        Ok(AccountId(i64::from_column(value)?))
    }
}

impl ToSqlColumn for AccountId {
    fn to_column(&self) -> Result<dao::Param> {
        self.0.to_column()
    }
}

/// Validated email — must contain '@' on read and write.
#[derive(Debug, Clone, PartialEq)]
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

impl ToSqlColumn for Email {
    fn to_column(&self) -> Result<dao::Param> {
        if self.0.contains('@') {
            self.0.to_column()
        } else {
            Err(Error::custom(format!("refusing to write invalid email: {}", self.0)))
        }
    }
}

/// Money in cents — never negative.
#[derive(Debug, Clone, PartialEq)]
struct Cents(i64);

impl FromSqlColumn for Cents {
    fn from_column(value: &ColumnValue) -> Result<Self> {
        let n = i64::from_column(value)?;
        Ok(Cents(n))
    }
}

impl ToSqlColumn for Cents {
    fn to_column(&self) -> Result<dao::Param> {
        self.0.to_column()
    }
}

#[derive(Debug, Clone, PartialEq, Entity)]
#[dao(table = "accounts")]
struct Account {
    #[dao(pk)]
    id: AccountId,
    email: Email,
    balance: Cents,
}

#[dao]
#[async_trait]
trait AccountDao {
    #[query("SELECT id, email, balance FROM accounts WHERE id = ?")]
    async fn get(&self, id: AccountId) -> Result<Option<Account>>;

    #[insert]
    async fn create(&self, account: Account) -> Result<ExecuteResult>;

    #[update]
    async fn update(&self, account: Account) -> Result<ExecuteResult>;
}

#[tokio::main]
async fn main() -> Result<()> {
    let pool = setup_db().await?;
    let dao = AccountDao::new(pool);

    // Create account with custom types
    let account = Account {
        id: AccountId(1),
        email: Email("alice@example.com".to_string()),
        balance: Cents(5000),
    };
    dao.create(account.clone()).await?;

    // Read back — FromSqlColumn converts columns to newtypes
    let fetched = dao.get(AccountId(1)).await?.unwrap();
    println!("Account: email={:?}, balance={:?}", fetched.email, fetched.balance);
    assert_eq!(fetched, account);

    // Update with new email
    let updated = Account {
        id: AccountId(1),
        email: Email("alice@newdomain.com".to_string()),
        balance: Cents(7500),
    };
    dao.update(updated.clone()).await?;

    let after = dao.get(AccountId(1)).await?.unwrap();
    assert_eq!(after.email, Email("alice@newdomain.com".to_string()));
    assert_eq!(after.balance, Cents(7500));
    println!("Updated: email={:?}, balance={:?}", after.email, after.balance);

    println!("\nAll checks passed!");
    Ok(())
}
