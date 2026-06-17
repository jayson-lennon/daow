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

mod dao;
mod entity;

use proc_macro::TokenStream;

/// Attribute macro that transforms a trait with `#[query("...")]` methods into a concrete
/// `{TraitName}Impl` struct with async method implementations.
#[proc_macro_attribute]
pub fn dao(attr: TokenStream, item: TokenStream) -> TokenStream {
    dao::dao_impl(attr, item)
}

/// Derive macro that generates a `FromRow` impl for a struct,
/// mapping database columns to struct fields.
///
/// Supports `#[dao(column = "custom_name")]` on fields to rename the
/// database column used for mapping.
#[proc_macro_derive(Entity, attributes(dao))]
pub fn derive_entity(item: TokenStream) -> TokenStream {
    entity::derive_entity_impl(item)
}
