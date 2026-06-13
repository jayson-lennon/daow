// A `.with(&tx)` view borrows the transaction for its whole usable lifetime
// (NLL keeps the borrow alive until the view's last use). Committing the
// transaction (which takes `tx` by value) while the view is *still in use
// later* must be rejected by the borrow checker (E0505): you cannot move `tx`
// out while it is borrowed.
//
// The realistic misuse this guards: commit the tx, then keep issuing statements
// through the view bound to that (now-consumed) transaction.
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
    let tx = pool.begin().await.unwrap();
    let view = dao.with(&tx);
    view.get(1).await.unwrap();
    tx.commit().await.unwrap(); // ERROR E0505: cannot move out of borrowed `tx`
    view.get(2).await.unwrap(); // this later use keeps the borrow live across commit
}
