// Copyright (C) 2026 Jayson Lennon
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU Lesser General Public
// License as published by the Free Software Foundation; either
// version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
// Lesser General Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License
// along with this program; if not, see <https://opensource.org/license/lgpl-3-0>.

// A `.with(&tx)` view is lifetime-tied to the transaction (`'a`). Returning it
// from an inner scope where `tx` is dropped must be rejected by the borrow
// checker as "tx does not live long enough" (E0597).
use daow::{async_trait, dao, Entity, Pool, Result};

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
