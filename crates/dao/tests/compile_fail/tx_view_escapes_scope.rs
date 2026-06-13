// A `.with(&tx)` view is lifetime-tied to the transaction (`'a`). Returning it
// from an inner scope where `tx` is dropped must be rejected by the borrow
// checker as "tx does not live long enough" (E0597).
use dao::{async_trait, dao, Entity, Pool, Result};

#[derive(Entity)]
struct Item {
    id: i64,
}

#[dao]
#[async_trait]
trait ItemDao {
    #[query("SELECT id FROM items WHERE id = ?")]
    async fn get(&self, id: i64) -> Result<Option<Item>>;
}

#[tokio::main]
async fn main() {
    let pool = Pool::open("test.db").unwrap();
    let dao = ItemDao::new(pool.clone());

    // The view is tied to the lifetime of a transaction that only lives inside
    // this block — it cannot escape.
    let leaked = {
        let tx = pool.begin().await.unwrap();
        dao.with(&tx) // ERROR: `tx` does not live long enough
    };
    leaked.get(1).await.unwrap();
}
