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

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Field, Fields};

/// Field metadata extracted from struct attributes.
struct FieldInfo {
    /// The Rust field name.
    field_ident: syn::Ident,
    /// The column name to use in SQL (may differ from field name via #[dao(column = "...")]).
    column_name: String,
    /// The field type.
    field_type: syn::Type,
    /// Whether this field is a primary key (via #[dao(pk)]).
    is_pk: bool,
    /// The field's position index in the struct (0-based).
    field_index: usize,
}

/// Implements the `#[derive(Entity)]` proc macro.
///
/// Generates a `FromRow` impl that maps each struct field to a database column
/// by name. When `#[dao(table = "...")]` is present on the struct and at least
/// one field is marked with `#[dao(pk)]`, also generates `ToRow` and `EntityMeta` impls.
pub fn derive_entity_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let struct_name = &input.ident;

    // Only support named structs (not tuple structs or unit structs)
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return syn::Error::new_spanned(
                    &input,
                    "Entity derive only supports structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&input, "Entity derive only supports structs")
                .to_compile_error()
                .into();
        }
    };

    // Extract table name from #[dao(table = "...")] on the struct
    let table_name = match extract_table_name(&input) {
        Ok(name) => name,
        Err(err) => return err.to_compile_error().into(),
    };

    // Extract field info with column name resolution, pk flag, and index
    let field_infos: Vec<FieldInfo> = match fields
        .iter()
        .enumerate()
        .map(|(i, field)| extract_field_info(field, i))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(infos) => infos,
        Err(err) => return err.to_compile_error().into(),
    };

    // Generate the field assignments for the FromRow impl
    let field_assignments = field_infos.iter().map(|info| {
        let ident = &info.field_ident;
        let col_name = &info.column_name;
        let ty = &info.field_type;
        quote! {
            #ident: row.get::<#ty>(#col_name)?
        }
    });

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Always generate FromRow
    let from_row_impl = quote! {
        impl #impl_generics dao::FromRow for #struct_name #ty_generics #where_clause {
            fn from_row(row: &dao::Row) -> dao::Result<Self> {
                Ok(Self {
                    #(#field_assignments),*
                })
            }
        }
    };

    // Generate write support only if table attribute is present
    let write_impls = if let Some(ref table) = table_name {
        // Check that at least one PK field exists
        let pk_fields: Vec<_> = field_infos.iter().filter(|f| f.is_pk).collect();
        if pk_fields.is_empty() {
            return syn::Error::new_spanned(
                &input,
                "#[dao(table = \"...\")] requires at least one field marked with #[dao(pk)]",
            )
            .to_compile_error()
            .into();
        }

        generate_write_impls(
            struct_name,
            table,
            &field_infos,
            &impl_generics,
            &ty_generics,
            where_clause,
        )
    } else {
        quote! {}
    };

    let expanded = quote! {
        #from_row_impl
        #write_impls
    };

    expanded.into()
}

/// Extract the table name from #[dao(table = "...")] on the struct.
fn extract_table_name(input: &DeriveInput) -> syn::Result<Option<String>> {
    let mut table_name = None;

    for attr in &input.attrs {
        if attr.path().is_ident("dao") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("table") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    table_name = Some(value.value());
                    Ok(())
                } else {
                    Err(meta.error("expected `table`"))
                }
            })
            .map_err(|e| syn::Error::new_spanned(attr, e.to_string()))?;
        }
    }

    Ok(table_name)
}

/// Extract field info including column rename, pk flag, from #[dao(...)] attribute.
fn extract_field_info(field: &Field, field_index: usize) -> syn::Result<FieldInfo> {
    let field_ident = field
        .ident
        .clone()
        .ok_or_else(|| syn::Error::new_spanned(field, "unnamed fields are not supported"))?;

    let field_type = field.ty.clone();

    // Default column name is the field name
    let mut column_name = field_ident.to_string();
    let mut is_pk = false;

    // Check for #[dao(column = "...", pk)] attribute
    for attr in &field.attrs {
        if attr.path().is_ident("dao") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("column") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    column_name = value.value();
                    Ok(())
                } else if meta.path.is_ident("pk") {
                    is_pk = true;
                    Ok(())
                } else {
                    Err(meta.error("expected `column` or `pk`"))
                }
            })
            .map_err(|e| syn::Error::new_spanned(attr, e.to_string()))?;
        }
    }

    Ok(FieldInfo {
        field_ident,
        column_name,
        field_type,
        is_pk,
        field_index,
    })
}

/// Generate ToRow and EntityMeta impls for a struct with table metadata.
fn generate_write_impls(
    struct_name: &syn::Ident,
    table_name: &str,
    field_infos: &[FieldInfo],
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
) -> proc_macro2::TokenStream {
    let field_count = field_infos.len();

    // --- ToRow impl ---

    // to_insert_params: all fields in declaration order
    let insert_param_exprs = field_infos.iter().map(|info| {
        let ident = &info.field_ident;
        quote! { dao::ToSqlColumn::to_column(&self.#ident)? }
    });

    // to_update_params: non-pk fields first, then pk fields
    let non_pk_fields: Vec<_> = field_infos.iter().filter(|f| !f.is_pk).collect();
    let pk_fields: Vec<_> = field_infos.iter().filter(|f| f.is_pk).collect();

    let update_param_exprs = non_pk_fields
        .iter()
        .map(|info| {
            let ident = &info.field_ident;
            quote! { dao::ToSqlColumn::to_column(&self.#ident)? }
        })
        .chain(pk_fields.iter().map(|info| {
            let ident = &info.field_ident;
            quote! { dao::ToSqlColumn::to_column(&self.#ident)? }
        }));

    // to_delete_params: pk fields only
    let delete_param_exprs = pk_fields.iter().map(|info| {
        let ident = &info.field_ident;
        quote! { dao::ToSqlColumn::to_column(&self.#ident)? }
    });

    let to_row_impl = quote! {
        impl #impl_generics dao::ToRow for #struct_name #ty_generics #where_clause {
            fn to_insert_params(&self) -> dao::Result<Vec<dao::Param>> {
                Ok(vec![#(#insert_param_exprs),*])
            }

            fn to_update_params(&self) -> dao::Result<Vec<dao::Param>> {
                Ok(vec![#(#update_param_exprs),*])
            }

            fn to_delete_params(&self) -> dao::Result<Vec<dao::Param>> {
                Ok(vec![#(#delete_param_exprs),*])
            }
        }
    };

    // --- EntityMeta impl ---

    // Generate SQL strings
    let columns: Vec<&str> = field_infos.iter().map(|f| f.column_name.as_str()).collect();
    let placeholders: Vec<&str> = (0..field_count).map(|_| "?").collect();
    let insert_sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        table_name,
        columns.join(", "),
        placeholders.join(", "),
    );

    // UPDATE: SET non-pk columns, WHERE pk columns
    let set_clause: String = non_pk_fields
        .iter()
        .map(|f| format!("{} = ?", f.column_name))
        .collect::<Vec<_>>()
        .join(", ");
    let where_clause_sql: String = pk_fields
        .iter()
        .map(|f| format!("{} = ?", f.column_name))
        .collect::<Vec<_>>()
        .join(" AND ");
    let update_sql = format!(
        "UPDATE {} SET {} WHERE {}",
        table_name, set_clause, where_clause_sql
    );

    // DELETE: WHERE pk columns
    let delete_sql = format!("DELETE FROM {} WHERE {}", table_name, where_clause_sql);

    // UPSERT: INSERT ... ON CONFLICT(pk) DO UPDATE SET non_pk = excluded.non_pk
    // For all-PK entities (junction tables), there's nothing to SET, so emit DO NOTHING.
    let conflict_target: String = pk_fields
        .iter()
        .map(|f| f.column_name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let upsert_sql = if non_pk_fields.is_empty() {
        format!(
            "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT({}) DO NOTHING",
            table_name,
            columns.join(", "),
            placeholders.join(", "),
            conflict_target
        )
    } else {
        let set_excluded: String = non_pk_fields
            .iter()
            .map(|f| format!("{} = excluded.{}", f.column_name, f.column_name))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT({}) DO UPDATE SET {}",
            table_name,
            columns.join(", "),
            placeholders.join(", "),
            conflict_target,
            set_excluded
        )
    };

    // PK indices
    let pk_indices: Vec<usize> = pk_fields.iter().map(|f| f.field_index).collect();

    let entity_meta_impl = quote! {
        impl #impl_generics dao::EntityMeta for #struct_name #ty_generics #where_clause {
            const TABLE_NAME: &'static str = #table_name;
            const FIELD_COUNT: usize = #field_count;
            const PK_INDICES: &'static [usize] = &[#(#pk_indices),*];

            fn insert_sql() -> &'static str {
                #insert_sql
            }

            fn upsert_sql() -> &'static str {
                #upsert_sql
            }

            fn update_sql() -> &'static str {
                #update_sql
            }

            fn delete_sql() -> &'static str {
                #delete_sql
            }
        }
    };

    quote! {
        #to_row_impl
        #entity_meta_impl
    }
}
