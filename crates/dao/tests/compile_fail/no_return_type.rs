// This file should fail to compile because the method has no return type.
use dao::{async_trait, dao, Entity, Pool};

#[derive(Entity)]
struct Item {
    id: i64,
}

#[dao]
#[async_trait]
trait BadDao {
    #[query("SELECT * FROM recalls")]
    async fn get_all(&self);
}

fn main() {}
