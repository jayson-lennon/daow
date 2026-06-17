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

// This file should fail to compile because a DAO method has BOTH an annotation
// (`#[query]`) and a body. That's contradictory: the annotation generates the
// body, so a hand-written body is ambiguous. The macro emits a clear error.
use dao::{async_trait, dao, Entity, Pool, Result};

#[derive(Entity)]
struct Widget {
    id: i64,
}

#[dao]
#[async_trait]
trait BadDao {
    #[query("SELECT id FROM widgets WHERE id = ?")]
    async fn get(&self, id: i64) -> Result<Option<Widget>> {
        // This body is illegal alongside #[query] — remove the annotation to use
        // a pass-through body, or remove the body to let the annotation generate it.
        self.query_one::<Widget>("SELECT id FROM widgets WHERE id = ?", vec![])
            .await
    }
}

fn main() {}
