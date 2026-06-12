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
}

/// Implements the `#[derive(Entity)]` proc macro.
///
/// Generates a `FromRow` impl that maps each struct field to a database column
/// by name. Fields use their Rust name by default, or a custom column name
/// via `#[dao(column = "custom_name")]`.
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
            return syn::Error::new_spanned(
                &input,
                "Entity derive only supports structs",
            )
            .to_compile_error()
            .into();
        }
    };

    // Extract field info with column name resolution
    let field_infos: Vec<FieldInfo> = match fields
        .iter()
        .map(extract_field_info)
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

    let expanded = quote! {
        impl #impl_generics dao::FromRow for #struct_name #ty_generics #where_clause {
            fn from_row(row: &dao::Row) -> dao::Result<Self> {
                Ok(Self {
                    #(#field_assignments),*
                })
            }
        }
    };

    expanded.into()
}

/// Extract field info including any column rename from #[dao(column = "...")] attribute.
fn extract_field_info(field: &Field) -> syn::Result<FieldInfo> {
    let field_ident = field
        .ident
        .clone()
        .ok_or_else(|| syn::Error::new_spanned(field, "unnamed fields are not supported"))?;

    let field_type = field.ty.clone();

    // Default column name is the field name
    let mut column_name = field_ident.to_string();

    // Check for #[dao(column = "...")] attribute
    for attr in &field.attrs {
        if attr.path().is_ident("dao") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("column") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    column_name = value.value();
                    Ok(())
                } else {
                    Err(meta.error("expected `column`"))
                }
            })
            .map_err(|e| syn::Error::new_spanned(attr, e.to_string()))?;
        }
    }

    Ok(FieldInfo {
        field_ident,
        column_name,
        field_type,
    })
}
