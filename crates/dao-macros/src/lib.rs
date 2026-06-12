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
