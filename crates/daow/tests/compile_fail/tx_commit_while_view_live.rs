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

// A `.with(&tx)` view borrows the transaction for its whole usable lifetime
// (NLL keeps the borrow alive until the view's last use). Committing the
// transaction (which takes `tx` by value) while the view is *still in use
// later* must be rejected by the borrow checker (E0505): you cannot move `tx`
// out while it is borrowed.
//
// The realistic misuse this guards: commit the tx, then keep issuing statements
// through the view bound to that (now-consumed) transaction.
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
    let tx = pool.begin().await.unwrap();
    let view = dao.with(&tx);
    view.get(1).await.unwrap();
    tx.commit().await.unwrap(); // ERROR E0505: cannot move out of borrowed `tx`
    view.get(2).await.unwrap(); // this later use keeps the borrow live across commit
}
