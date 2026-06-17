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

// This file should fail to compile because the entity has 3 fields but #[execute] has 2 placeholders.
use daow::{async_trait, dao, Entity, ExecuteResult, Result};

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
