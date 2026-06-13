// This file should fail to compile because #[execute] has 2 placeholders but 1 scalar param.
use dao::{async_trait, dao, Entity, ExecuteResult, Pool, Result};

#[derive(Entity)]
struct Item {
    id: i64,
}

#[dao]
#[async_trait]
trait BadDao {
    #[execute("DELETE FROM items WHERE id = ? AND name = ?")]
    async fn delete(&self, id: i64) -> Result<ExecuteResult>;
}

fn main() {}
