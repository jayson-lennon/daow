// This file should fail to compile because the entity has 3 fields but #[execute] has 2 placeholders.
use dao::{async_trait, dao, Entity, ExecuteResult, Result};

#[derive(Entity)]
#[dao(table = "items")]
struct Item {
    #[dao(pk)]
    id: i64,
    name: String,
    price: f64,
}

#[dao]
#[async_trait]
trait BadDao {
    // Item has 3 fields but SQL has 2 placeholders — should fail FIELD_COUNT const assertion
    #[execute("INSERT INTO items (id, name) VALUES (?, ?)")]
    async fn bad_insert(&self, item: Item) -> Result<ExecuteResult>;
}

fn main() {}
