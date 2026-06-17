# dao

> **crates.io README — TODO (owner-authored).** See
> [`docs/guide.md`](docs/guide.md) for the compile-time SQL-validation guide.

A small, async SQLite data-access layer for Rust.

- Compile-time SQL validation against a real schema (no typo'd column ships).
- `#[dao]` trait → async method generation (`#[query]`, `#[insert]`, `#[update]`,
  `#[delete]`, `#[upsert]`).
- Bounded connection pool + typed multi-DAO transactions.

## Status

Pre-release, served from GitHub (the `dao` crate name is squatted on
crates.io pending a rename). Builds on Rust **1.85** / **edition 2024**.

## License

LGPL-3.0-or-later. See [`LICENSE`](LICENSE).
