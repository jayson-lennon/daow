// This file should fail to compile because the destructured pattern has 2 fields
// but the SQL has 3 placeholders.
use dao::{async_trait, dao, Entity, ExecuteResult, Result};

#[derive(Entity)]
struct Item {
    id: i64,
    name: String,
    price: f64,
}

#[dao]
#[async_trait]
trait BadDao {
    #[execute("UPDATE items SET name = ?, price = ? WHERE id = ?")]
    async fn bad(&self, Item { name, id, .. }: Item) -> Result<ExecuteResult>;
}

fn main() {}
