use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, FnArg, GenericArgument, ItemTrait, PathArguments, ReturnType, TraitItem,
    TraitItemFn, Type,
};

/// The kind of DAO method, determined by its annotation.
#[derive(Debug, PartialEq, Clone)]
enum MethodKind {
    Query,
    Insert,
    Update,
    Delete,
    Execute,
}

/// Describes a single method parsed from the trait.
struct DaoMethod {
    /// The method name.
    ident: syn::Ident,
    /// The kind of method (query, insert, update, delete, execute).
    method_kind: MethodKind,
    /// The SQL string (Some for Query and Execute, None for Insert/Update/Delete).
    sql: Option<String>,
    /// The method parameters (excluding &self).
    params: Vec<syn::PatType>,
    /// Whether the inner return is Option<T>, Vec<T>, or bare T.
    return_kind: ReturnKind,
    /// The full return type as written by the user (e.g., Result<Option<RecallEntity>>).
    full_return_type: Type,
    /// The number of placeholders in the SQL (set during validation for Execute methods).
    sql_param_count: Option<usize>,
}

#[derive(Debug, PartialEq)]
enum ReturnKind {
    Option,
    Vec,
    Bare,
}

/// Implements the `#[dao]` attribute macro.
///
/// Parses a trait definition, extracts methods annotated with `#[query("...")]`,
/// `#[insert]`, `#[update]`, `#[delete]`, or `#[execute("...")]`, validates SQL
/// statements at compile time, and generates a `{TraitName}Impl` struct with async
/// method implementations.
pub fn dao_impl(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemTrait);

    let trait_name = &input.ident;
    let trait_name_str = trait_name.to_string();
    let struct_name = trait_name.clone();
    let renamed_trait = syn::Ident::new(&format!("{}Trait", trait_name_str), trait_name.span());

    // Extract outer attributes from the trait, filtering out #[dao] itself
    // to avoid infinite macro recursion
    let outer_attrs: Vec<_> = input
        .attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("dao"))
        .collect();

    // Extract all annotated methods from the trait
    let mut methods = match extract_methods(&input) {
        Ok(m) => m,
        Err(e) => return e.to_compile_error().into(),
    };

    // Validate SQL at compile time for Query and Execute methods
    if let Err(e) = validate_sql(&mut methods) {
        return e.to_compile_error().into();
    }

    // Generate the impl methods
    let generated_methods: Vec<_> = methods.iter().map(generate_method).collect();

    // Re-emit the original trait with all DAO attributes stripped and renamed to {Name}Trait
    let clean_trait = strip_dao_attrs(&input, &renamed_trait);

    let expanded = quote! {
        #clean_trait

        pub struct #struct_name {
            pool: dao::Pool,
        }

        impl #struct_name {
            pub fn new(pool: dao::Pool) -> Self {
                Self { pool }
            }
        }

        #(#outer_attrs)*
        impl #renamed_trait for #struct_name {
            #(#generated_methods)*
        }
    };

    expanded.into()
}

/// Strip all DAO-related attributes from the trait's methods and rename the trait.
fn strip_dao_attrs(trait_def: &ItemTrait, new_name: &syn::Ident) -> proc_macro2::TokenStream {
    let mut trait_clone = trait_def.clone();
    trait_clone.ident = new_name.clone();

    for item in &mut trait_clone.items {
        if let TraitItem::Fn(method) = item {
            method.attrs.retain(|attr| {
                !attr.path().is_ident("query")
                    && !attr.path().is_ident("insert")
                    && !attr.path().is_ident("update")
                    && !attr.path().is_ident("delete")
                    && !attr.path().is_ident("execute")
            });
        }
    }

    quote! { #trait_clone }
}

/// Extract all annotated methods from the trait.
fn extract_methods(trait_def: &ItemTrait) -> syn::Result<Vec<DaoMethod>> {
    let mut methods = Vec::new();

    for item in &trait_def.items {
        if let TraitItem::Fn(method) = item {
            if let Some(extracted) = extract_method_kind(method)? {
                let (params, return_kind, full_return_type) = analyze_signature(method)?;
                methods.push(DaoMethod {
                    ident: method.sig.ident.clone(),
                    method_kind: extracted.method_kind,
                    sql: extracted.sql,
                    params,
                    return_kind,
                    full_return_type,
                    sql_param_count: None,
                });
            }
        }
    }

    if methods.is_empty() {
        return Err(syn::Error::new_spanned(
            trait_def,
            "#[dao] trait must have at least one annotated method (#[query], #[insert], #[update], #[delete], or #[execute])",
        ));
    }

    Ok(methods)
}

struct ExtractedMethod {
    method_kind: MethodKind,
    sql: Option<String>,
}

/// Determine the method kind from its annotation.
fn extract_method_kind(method: &TraitItemFn) -> syn::Result<Option<ExtractedMethod>> {
    let mut found: Option<ExtractedMethod> = None;

    for attr in &method.attrs {
        if attr.path().is_ident("query") {
            if found.is_some() {
                return Err(syn::Error::new_spanned(
                    attr,
                    "method has multiple DAO annotations",
                ));
            }
            let sql: syn::LitStr = attr.parse_args()?;
            found = Some(ExtractedMethod {
                method_kind: MethodKind::Query,
                sql: Some(sql.value()),
            });
        } else if attr.path().is_ident("insert") {
            if found.is_some() {
                return Err(syn::Error::new_spanned(
                    attr,
                    "method has multiple DAO annotations",
                ));
            }
            found = Some(ExtractedMethod {
                method_kind: MethodKind::Insert,
                sql: None,
            });
        } else if attr.path().is_ident("update") {
            if found.is_some() {
                return Err(syn::Error::new_spanned(
                    attr,
                    "method has multiple DAO annotations",
                ));
            }
            found = Some(ExtractedMethod {
                method_kind: MethodKind::Update,
                sql: None,
            });
        } else if attr.path().is_ident("delete") {
            if found.is_some() {
                return Err(syn::Error::new_spanned(
                    attr,
                    "method has multiple DAO annotations",
                ));
            }
            found = Some(ExtractedMethod {
                method_kind: MethodKind::Delete,
                sql: None,
            });
        } else if attr.path().is_ident("execute") {
            if found.is_some() {
                return Err(syn::Error::new_spanned(
                    attr,
                    "method has multiple DAO annotations",
                ));
            }
            let sql: syn::LitStr = attr.parse_args()?;
            found = Some(ExtractedMethod {
                method_kind: MethodKind::Execute,
                sql: Some(sql.value()),
            });
        }
    }

    Ok(found)
}

fn analyze_signature(method: &TraitItemFn) -> syn::Result<(Vec<syn::PatType>, ReturnKind, Type)> {
    // Extract params, expecting first argument to be &self
    let mut params = Vec::new();
    let mut first = true;

    for arg in &method.sig.inputs {
        if first {
            first = false;
            match arg {
                FnArg::Receiver(_) => {} // &self — good
                FnArg::Typed(_) => {
                    return Err(syn::Error::new_spanned(
                        arg,
                        "DAO methods must have &self as the first parameter",
                    ));
                }
            }
            continue;
        }

        if let FnArg::Typed(pat_type) = arg {
            params.push(pat_type.clone());
        } else {
            return Err(syn::Error::new_spanned(
                arg,
                "DAO method parameters must be typed",
            ));
        }
    }

    // Analyze return type
    let return_type = &method.sig.output;
    let return_kind = match return_type {
        ReturnType::Default => {
            return Err(syn::Error::new_spanned(
                method,
                "DAO methods must have a return type",
            ));
        }
        ReturnType::Type(_, ty) => analyze_return_type(ty)?.0,
    };

    let full_return_type: Type = match &method.sig.output {
        ReturnType::Type(_, ty) => (**ty).clone(),
        _ => unreachable!(),
    };

    Ok((params, return_kind, full_return_type))
}

/// Analyze the return type to unwrap Result<T> first, then determine if inner is Option<T>, Vec<T>, or bare T.
fn analyze_return_type(ty: &Type) -> syn::Result<(ReturnKind, Type)> {
    if let Type::Path(type_path) = ty {
        let segment = type_path
            .path
            .segments
            .last()
            .ok_or_else(|| syn::Error::new_spanned(ty, "empty return type path"))?;

        let ident = segment.ident.to_string();

        match ident.as_str() {
            "Result" => {
                let inner_type = extract_generic_arg(&segment.arguments)?;
                analyze_inner_type(&inner_type)
            }
            _ => analyze_inner_type(ty),
        }
    } else {
        Ok((ReturnKind::Bare, (*ty).clone()))
    }
}

/// Analyze the inner type (inside Result) to determine if it's Option<T>, Vec<T>, or bare T.
fn analyze_inner_type(ty: &Type) -> syn::Result<(ReturnKind, Type)> {
    if let Type::Path(type_path) = ty {
        let segment = type_path
            .path
            .segments
            .last()
            .ok_or_else(|| syn::Error::new_spanned(ty, "empty return type path"))?;

        let ident = segment.ident.to_string();

        match ident.as_str() {
            "Option" => {
                let inner = extract_generic_arg(&segment.arguments)?;
                Ok((ReturnKind::Option, inner))
            }
            "Vec" => {
                let inner = extract_generic_arg(&segment.arguments)?;
                Ok((ReturnKind::Vec, inner))
            }
            _ => Ok((ReturnKind::Bare, (*ty).clone())),
        }
    } else {
        Ok((ReturnKind::Bare, (*ty).clone()))
    }
}

/// Extract the first generic argument from PathArguments.
fn extract_generic_arg(args: &PathArguments) -> syn::Result<Type> {
    if let PathArguments::AngleBracketed(args) = args {
        if let Some(GenericArgument::Type(ty)) = args.args.first() {
            return Ok(ty.clone());
        }
    }
    Err(syn::Error::new_spanned(
        args,
        "expected generic type argument (e.g., Option<T>)",
    ))
}
/// Count the effective number of SQL parameters from method params.
/// Simple ident params (e.g., `id: i64`) count as 1.
/// Destructured struct params (e.g., `User { name, id, .. }: User`) count as the number of named fields.
fn count_effective_params(params: &[syn::PatType]) -> usize {
    params
        .iter()
        .map(|p| match &*p.pat {
            syn::Pat::Ident(_) => 1,
            syn::Pat::Struct(s) => s.fields.len(),
            _ => panic!(
                "expected identifiable or struct destructured parameter"
            ),
        })
        .sum()
}


/// Validate SQL statements at compile time for Query and Execute methods.
fn validate_sql(methods: &mut [DaoMethod]) -> syn::Result<()> {
    // Collect methods that need SQL validation
    let sql_methods: Vec<_> = methods
        .iter_mut()
        .filter(|m| matches!(m.method_kind, MethodKind::Query | MethodKind::Execute))
        .collect();

    if sql_methods.is_empty() {
        return Ok(());
    }

    let db_url = match std::env::var("DAO_DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "DAO_DATABASE_URL environment variable not set. \
                 Set it to a SQLite database path for compile-time SQL validation.",
            ));
        }
    };

    let conn = rusqlite::Connection::open(&db_url).map_err(|e| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("failed to open database at {}: {}", db_url, e),
        )
    })?;

    for method in sql_methods {
        let sql = method.sql.as_ref().unwrap();
        let annotation_name = match method.method_kind {
            MethodKind::Query => "query",
            MethodKind::Execute => "execute",
            _ => unreachable!(),
        };

        let param_count = match conn.prepare(sql) {
            Ok(stmt) => stmt.parameter_count(),
            Err(e) => {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    format!(
                        "invalid SQL in #[{}] on method '{}': {}",
                        annotation_name, method.ident, e
                    ),
                ));
            }
        };

        let expected_params = count_effective_params(&method.params);
        if param_count != expected_params {
            // Allow single-entity expansion for #[execute]: if there's exactly one
            // non-self parameter and more placeholders than params, defer validation
            // to a compile-time FIELD_COUNT const assertion.
            let is_entity_expand = matches!(method.method_kind, MethodKind::Execute)
                && expected_params == 1
                && param_count > 1
                && method.params.len() == 1
                && matches!(&*method.params[0].pat, syn::Pat::Ident(_));

            if !is_entity_expand {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    format!(
                        "parameter count mismatch in method '{}': SQL has {} placeholders but method has {} parameters",
                        method.ident, param_count, expected_params
                    ),
                ));
            }

            // Store the SQL param count for entity expansion code generation
            method.sql_param_count = Some(param_count);
        }
    }

    Ok(())
}

/// Generate a single async method implementation.
fn generate_method(method: &DaoMethod) -> proc_macro2::TokenStream {
    match method.method_kind {
        MethodKind::Query => generate_query_method(method),
        MethodKind::Insert => generate_insert_method(method),
        MethodKind::Update => generate_update_method(method),
        MethodKind::Delete => generate_delete_method(method),
        MethodKind::Execute => generate_execute_method(method),
    }
}

/// Generate a query method (existing behavior).
fn generate_query_method(method: &DaoMethod) -> proc_macro2::TokenStream {
    let ident = &method.ident;
    let sql = method.sql.as_ref().unwrap();
    let full_return_type = &method.full_return_type;

    let param_tokens = generate_param_tokens(method);
    let param_bindings = generate_param_bindings(method);

    match method.return_kind {
        ReturnKind::Option => quote! {
            async fn #ident(&self, #(#param_tokens),*) -> #full_return_type {
                self.pool.query_one(#sql, #param_bindings).await
            }
        },
        ReturnKind::Vec => quote! {
            async fn #ident(&self, #(#param_tokens),*) -> #full_return_type {
                self.pool.query_all(#sql, #param_bindings).await
            }
        },
        ReturnKind::Bare => quote! {
            async fn #ident(&self, #(#param_tokens),*) -> #full_return_type {
                self.pool.query_one(#sql, #param_bindings).await
                    .and_then(|opt| opt.ok_or_else(|| dao::Error::custom("query returned no rows")))
            }
        },
    }
}

/// Generate an insert method using EntityMeta::insert_sql() and to_insert_params().
fn generate_insert_method(method: &DaoMethod) -> proc_macro2::TokenStream {
    let ident = &method.ident;
    let full_return_type = &method.full_return_type;
    let entity_type = get_entity_type(method);
    let param_tokens = generate_param_tokens(method);
    let param_name = get_param_name(method, 0);

    quote! {
        async fn #ident(&self, #(#param_tokens),*) -> #full_return_type {
            self.pool.execute(
                <#entity_type as dao::EntityMeta>::insert_sql(),
                dao::ToRow::to_insert_params(&#param_name)?,
            ).await
        }
    }
}

/// Generate an update method using EntityMeta::update_sql() and to_update_params().
fn generate_update_method(method: &DaoMethod) -> proc_macro2::TokenStream {
    let ident = &method.ident;
    let full_return_type = &method.full_return_type;
    let entity_type = get_entity_type(method);
    let param_tokens = generate_param_tokens(method);
    let param_name = get_param_name(method, 0);

    quote! {
        async fn #ident(&self, #(#param_tokens),*) -> #full_return_type {
            self.pool.execute(
                <#entity_type as dao::EntityMeta>::update_sql(),
                dao::ToRow::to_update_params(&#param_name)?,
            ).await
        }
    }
}

/// Generate a delete method using EntityMeta::delete_sql() and to_delete_params().
fn generate_delete_method(method: &DaoMethod) -> proc_macro2::TokenStream {
    let ident = &method.ident;
    let full_return_type = &method.full_return_type;
    let entity_type = get_entity_type(method);
    let param_tokens = generate_param_tokens(method);
    let param_name = get_param_name(method, 0);

    quote! {
        async fn #ident(&self, #(#param_tokens),*) -> #full_return_type {
            self.pool.execute(
                <#entity_type as dao::EntityMeta>::delete_sql(),
                dao::ToRow::to_delete_params(&#param_name)?,
            ).await
        }
    }
}

/// Generate an execute method with user-provided SQL.
fn generate_execute_method(method: &DaoMethod) -> proc_macro2::TokenStream {
    let ident = &method.ident;
    let sql = method.sql.as_ref().unwrap();
    let full_return_type = &method.full_return_type;

    let param_tokens = generate_param_tokens(method);

    if let Some(sql_param_count) = method.sql_param_count {
        // Entity expansion: single param implementing ToRow, validated via FIELD_COUNT const assertion
        let entity_type = &method.params[0].ty;
        let param_name = get_param_name(method, 0);
        let count_literal = sql_param_count;
        quote! {
            async fn #ident(&self, #(#param_tokens),*) -> #full_return_type {
                const _: () = assert!(
                    <#entity_type as dao::EntityMeta>::FIELD_COUNT == #count_literal,
                    concat!("parameter count mismatch: SQL has ", stringify!(#count_literal), " placeholders but entity has a different field count")
                );
                self.pool.execute(#sql, dao::ToRow::to_insert_params(&#param_name)?).await
            }
        }
    } else {
        // Scalar params: 1:1 binding
        let param_bindings = generate_param_bindings(method);
        quote! {
            async fn #ident(&self, #(#param_tokens),*) -> #full_return_type {
                self.pool.execute(#sql, #param_bindings).await
            }
        }
    }
}

/// Get the entity type from the first parameter (for Insert/Update/Delete).
fn get_entity_type(method: &DaoMethod) -> &syn::Type {
    &method.params[0].ty
}

/// Get the parameter name at the given index.
fn get_param_name(method: &DaoMethod, index: usize) -> &syn::Ident {
    if let syn::Pat::Ident(pat_ident) = &*method.params[index].pat {
        &pat_ident.ident
    } else {
        panic!("expected simple ident parameter name for entity-based annotations (#[insert]/#[update]/#[delete]/entity expansion); destructured patterns are not supported here")
    }
}

/// Generate parameter tokens for the function signature.
fn generate_param_tokens(method: &DaoMethod) -> Vec<proc_macro2::TokenStream> {
    method
        .params
        .iter()
        .map(|p| {
            let pat = &p.pat;
            let ty = &p.ty;
            quote! { #pat: #ty }
        })
        .collect()
}

/// Generate parameter binding expressions for SQL execution.
/// Supports simple ident params and destructured struct params.
fn generate_param_bindings(method: &DaoMethod) -> proc_macro2::TokenStream {
    let bindings: Vec<_> = method
        .params
        .iter()
        .flat_map(|p| match &*p.pat {
            syn::Pat::Ident(pat_ident) => {
                let ident = &pat_ident.ident;
                vec![quote! { dao::ToSqlColumn::to_column(&#ident)? }]
            }
            syn::Pat::Struct(pat_struct) => {
                pat_struct.fields.iter().map(|field_pat| {
                    let binding = match &*field_pat.pat {
                        syn::Pat::Ident(ident) => &ident.ident,
                        _ => panic!("expected identifiable field binding in struct pattern"),
                    };
                    quote! { dao::ToSqlColumn::to_column(&#binding)? }
                }).collect()
            }
            _ => panic!("expected identifiable or struct destructured parameter"),
        })
        .collect();

    if bindings.is_empty() {
        quote! { vec![] }
    } else {
        quote! { vec![#(#bindings),*] }
    }
}
