# dao

A small, async SQLite data-access layer for Rust. `dao` gives you typed query
traits whose SQL is **validated against a real database at compile time** — a
typo'd column name fails your build, not your test suite (or worse, production).

```rust
#[dao]
trait SessionDao {
    #[query("SELECT id, title FROM sessions WHERE id = ?")]
    async fn by_id(&self, id: String) -> dao::Result<Option<SessionRow>>;

    #[execute("UPDATE sessions SET archived = ? WHERE id = ?")]
    async fn set_archived(&self, archived: bool, id: String) -> dao::Result<dao::ExecuteResult>;
}
```

The macro calls `rusqlite::Connection::prepare(sql)` against a database you
point it at; if the SQL doesn't parse against the real schema, you get a
compile error naming the offending method and column.

This file documents how to wire that compile-time validation into a project.

---

## (a) On first run, you'll get a hard error

If you write a `#[dao]` trait with a `#[query]` or `#[execute]` method and
build without telling `dao` where the validation database is, the build fails:

```
error: DAO_DATABASE_URL environment variable not set.
       Set it to a SQLite database path for compile-time SQL validation.
```

**This is the guarantee, not a bug.** `dao` validates your SQL by `prepare`-ing
it against a real SQLite database (the one at `DAO_DATABASE_URL`). That database
must exist and must reflect the schema your code expects — otherwise the macro
is validating against a fiction. So `DAO_DATABASE_URL` is mandatory whenever
your crate uses `#[query]` / `#[execute]`.

The catch: *your migrator* is what defines that schema, and your library code
needs the validation database to *compile*. That's a chicken-and-egg problem,
solved below.

## (b) Structure your migrations as a separate schema crate

A crate's `build.rs` **cannot depend on the crate it belongs to.** So if your
migrations live inside your main domain crate, `build.rs` can't run them to
create the validation database, and you're stuck.

The fix is the **schema-crate pattern**: put your migrations in a standalone
*leaf* crate that nothing depends on (downward). Name it `<your-app>-schema`.

```
my-app-schema               ← leaf crate, NO dep on my-app
├── src/lib.rs              ← pub fn run_migrations(&mut Connection)
├── src/migrate/v0.rs ...   ← one module per migration
└── deps: rusqlite, serde, serde_json, ...
                            (NO dep on your domain crate — that's what breaks the cycle)
        ▲                          ▲
        │ [build-dependencies]     │ [dependencies]
        │                          │
   my-app/build.rs           my-app/.../migrator.rs
   (creates the validation   (delegates to my-app-schema
    DB, sets rustc-env)       at runtime through the dao pool)
```

The schema crate depends only on `rusqlite` and the serialization crates your
data migrations need. Your domain crate depends on it in **both**
`[dependencies]` (runtime) and `[build-dependencies]` (to create the validation
DB). No cycle: `build.rs` depends on a *different* crate, not itself.

The schema crate is the **single source of truth** for your schema. Adding a
migration is "add a module to `my-app-schema`"; `build.rs` and your runtime
migrator both pick it up automatically — no `.sql` file to keep in sync, no drift.

## (c) Data migrations use version-pinned structs

A *data* migration (one that reads old rows and rewrites them) often needs to
reconstruct a domain object — deserialize an old JSON column, transform it,
re-serialize. The temptation is to `use crate::MyType`. **Don't.**

A migration describes data **at a point in time**. If you import the live
`MyType`, then a future field added to `MyType` retroactively breaks the
migration (the struct no longer matches what the old rows contained). Instead,
define a version-pinned struct **in the schema crate**, named for the migration
that owns it:

```rust
// ❌ BAD — couples the migration to the live type
use my_app::PersistableCore;
fn migrate_v20(conn: &mut Connection) -> Result<()> {
    let blob: PersistableCore = serde_json::from_str(&row.metadata)?;
    /* ... */
}

// ✅ GOOD — a frozen snapshot of the blob shape at v20 time
#[derive(serde::Serialize)]
struct PersistableCoreV20 {
    session_id: String,
    title: Option<String>,
    profile: serde_json::Value,   // pass-through; a prior migration already normalized it
    // ...the fields as they existed at v20...
}

fn migrate_v20(conn: &mut Connection) -> Result<()> {
    let blob = PersistableCoreV20 { /* from old columns */ };
    let json = serde_json::to_string(&blob)?;
    /* ... */
}
```

`PersistableCoreV20` is **not duplication** — it is a snapshot. If the live
`PersistableCore` gains a field in v21, v20's snapshot correctly stays frozen;
v21 owns its own logic for the new field.

The one unavoidable coupling: the JSON `PersistableCoreV20` produces must still
deserialize via your *runtime* type. Pin that with a test — serialize a
`PersistableCoreV20`, deserialize it via the live type, assert it loads.

For migrations that only touch schema (DDL) or do pure string JSON surgery
(`&str` → `&str`), no struct is needed — they move to the schema crate verbatim.

## (d) The build.rs to apply to your project

With your schema crate in place, `build.rs` is ~10 lines:

```rust
// my-app/build.rs
use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"))
        .join("dao_validation");
    std::fs::create_dir_all(&dir).expect("create dao_validation dir");

    let db_path = dir.join("validation.db");
    let _ = std::fs::remove_file(&db_path);   // always recreate, schema is always current

    let mut conn = rusqlite::Connection::open(&db_path)
        .unwrap_or_else(|e| panic!("failed to open dao validation db: {e}"));

    // Single source of truth: the schema crate's own migrator defines the DB shape.
    my_app_schema::run_migrations(&mut conn).expect("apply schema migrations");

    println!("cargo:rustc-env=DAO_DATABASE_URL={}", db_path.to_string_lossy());
    println!("cargo:rerun-if-changed=../my-app-schema/src/lib.rs");
}
```

with the matching manifest entry:

```toml
# my-app/Cargo.toml
[build-dependencies]
my-app-schema = { path = "../my-app-schema" }
```

That's it. Every build recreates the validation database from your migrations,
sets `DAO_DATABASE_URL`, and the macro validates against it. Works in IDEs, CI,
and fresh clones — no environment variables to remember.

> **If the validation DB acts stale** after you edit a migration (cargo's
> build-script caching can lag), `cargo clean -p my-app` forces `build.rs` to
> rerun.

---

## Escape hatches

`dao`'s typed `#[dao]` traits cover static SQL. When you need something they
can't express, two escape hatches are available — both are exercised in
[`tests/jinn_patterns.rs`](crates/dao/tests/jinn_patterns.rs):

- **`pool.with_conn(|conn| { ... })`** — get a raw `&mut rusqlite::Connection`
  for a closure. Used for migrations (toggle `PRAGMA foreign_keys` off, run DDL,
  re-enable, then `PRAGMA foreign_key_check`) and any dynamic SQL.
- **`pool.query_one::<T>("PRAGMA ...", vec![])`** with a hand-written
  `FromRow` — for statements like `PRAGMA wal_checkpoint(TRUNCATE)` whose result
  columns you read by name.
