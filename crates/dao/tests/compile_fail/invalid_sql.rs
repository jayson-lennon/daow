// This file should fail to compile because the SQL references a nonexistent table.
use dao::{async_trait, dao, Entity, Pool, Result};

#[derive(Entity)]
struct Item {
    id: i64,
}

#[dao]
#[async_trait]
trait BadDao {
    #[query("SELECT * FROM nonexistent_table")]
    async fn get_all(&self) -> Result<Vec<Item>>;
}

fn main() {}
