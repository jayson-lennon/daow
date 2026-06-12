// This file should fail to compile because the SQL has 2 placeholders but the method has 1 param.
use dao::{async_trait, dao, Entity, Pool, Result};

#[derive(Entity)]
struct Item {
    id: i64,
}

#[dao]
#[async_trait]
trait BadDao {
    #[query("SELECT * FROM items WHERE id = ? AND name = ?")]
    async fn get_by_id(&self, id: i64) -> Result<Option<Item>>;
}

fn main() {}
