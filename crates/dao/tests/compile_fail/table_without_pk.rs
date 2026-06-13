// This file should fail to compile because #[dao(table)] is present but no field has #[dao(pk)].
use dao::Entity;

#[derive(Entity)]
#[dao(table = "users")]
struct User {
    id: i64,
    name: String,
}

fn main() {}
