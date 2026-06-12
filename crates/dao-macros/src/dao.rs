use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, FnArg, ItemTrait, ReturnType, TraitItem, TraitItemFn, Type,
    PathArguments, GenericArgument,
};

/// Describes a single query method parsed from the trait.
struct QueryMethod {
    /// The method name.
    ident: syn::Ident,
    /// The SQL string from the #[query("...")] attribute.
    sql: String,
    /// The method parameters (excluding &self).
    params: Vec<syn::PatType>,
    /// Whether the inner return is Option<T>, Vec<T>, or bare T.
    return_kind: ReturnKind,
    /// The full return type as written by the user (e.g., Result<Option<RecallEntity>>).
    full_return_type: Type,
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
/// validates each SQL statement at compile time against the database specified
/// by `DAO_DATABASE_URL`, and generates a `{TraitName}Impl` struct with async
/// method implementations that implement the trait.
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

    // Extract query methods from the trait
    let methods = match extract_query_methods(&input) {
        Ok(m) => m,
        Err(e) => return e.to_compile_error().into(),
    };

    // Validate SQL at compile time
    if let Err(e) = validate_sql(&methods) {
        return e.to_compile_error().into();
    }

    // Generate the impl methods
    let generated_methods: Vec<_> = methods.iter().map(generate_method).collect();

    // Re-emit the original trait with #[query] attributes stripped and renamed to {Name}Trait
    let clean_trait = strip_query_attrs(&input, &renamed_trait);

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

/// Strip all `#[query]` attributes from the trait's methods and rename the trait.
fn strip_query_attrs(trait_def: &ItemTrait, new_name: &syn::Ident) -> proc_macro2::TokenStream {
    let mut trait_clone = trait_def.clone();
    trait_clone.ident = new_name.clone();

    for item in &mut trait_clone.items {
        if let TraitItem::Fn(method) = item {
            method.attrs.retain(|attr| !attr.path().is_ident("query"));
        }
    }

    quote! { #trait_clone }
}

/// Extract all #[query("...")] methods from the trait.
fn extract_query_methods(trait_def: &ItemTrait) -> syn::Result<Vec<QueryMethod>> {
    let mut methods = Vec::new();

    for item in &trait_def.items {
        if let TraitItem::Fn(method) = item {
            if let Some(sql) = extract_query_sql(method)? {
                let (params, return_kind, full_return_type) =
                    analyze_signature(method)?;
                methods.push(QueryMethod {
                    ident: method.sig.ident.clone(),
                    sql,
                    params,
                    return_kind,
                    full_return_type,
                });
            }
        }
    }

    if methods.is_empty() {
        return Err(syn::Error::new_spanned(
            trait_def,
            "#[dao] trait must have at least one #[query(\"...\")] method",
        ));
    }

    Ok(methods)
}

/// Extract the SQL string from the #[query("...")] attribute on a method.
fn extract_query_sql(method: &TraitItemFn) -> syn::Result<Option<String>> {
    for attr in &method.attrs {
        if attr.path().is_ident("query") {
            let sql: syn::LitStr = attr.parse_args()?;
            return Ok(Some(sql.value()));
        }
    }
    Ok(None)
}

fn analyze_signature(
    method: &TraitItemFn,
) -> syn::Result<(Vec<syn::PatType>, ReturnKind, Type)> {
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
    // First, check if the outer type is Result
    if let Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last().ok_or_else(|| {
            syn::Error::new_spanned(ty, "empty return type path")
        })?;

        let ident = segment.ident.to_string();

        match ident.as_str() {
            "Result" => {
                // Unwrap Result<T> to get the inner T
                let inner_type = extract_generic_arg(&segment.arguments)?;
                // Now analyze the inner type for Option/Vec/Bare
                analyze_inner_type(&inner_type)
            }
            _ => {
                // Not wrapped in Result — analyze directly
                analyze_inner_type(ty)
            }
        }
    } else {
        Ok((ReturnKind::Bare, (*ty).clone()))
    }
}

/// Analyze the inner type (inside Result) to determine if it's Option<T>, Vec<T>, or bare T.
fn analyze_inner_type(ty: &Type) -> syn::Result<(ReturnKind, Type)> {
    if let Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last().ok_or_else(|| {
            syn::Error::new_spanned(ty, "empty return type path")
        })?;

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

/// Validate all SQL statements at compile time by connecting to the database
/// and preparing each statement.
fn validate_sql(methods: &[QueryMethod]) -> syn::Result<()> {
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

    for method in methods {
        let param_count = match conn.prepare(&method.sql) {
            Ok(stmt) => stmt.parameter_count(),
            Err(e) => {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    format!(
                        "invalid SQL in #[query] on method '{}': {}",
                        method.ident, e
                    ),
                ));
            }
        };

        // Check parameter count matches method params (excluding &self)
        let expected_params = method.params.len();
        if param_count != expected_params {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "parameter count mismatch in method '{}': SQL has {} placeholders but method has {} parameters",
                    method.ident, param_count, expected_params
                ),
            ));
        }
    }

    Ok(())
}

/// Generate a single async method implementation for a query.
fn generate_method(method: &QueryMethod) -> proc_macro2::TokenStream {
    let ident = &method.ident;
    let sql = &method.sql;
    let full_return_type = &method.full_return_type;

    // Generate parameter tokens for the function signature
    let param_tokens: Vec<_> = method
        .params
        .iter()
        .map(|p| {
            let pat = &p.pat;
            let ty = &p.ty;
            quote! { #pat: #ty }
        })
        .collect();

    // Generate parameter names for binding
    let param_names: Vec<_> = method
        .params
        .iter()
        .map(|p| {
            if let syn::Pat::Ident(pat_ident) = &*p.pat {
                &pat_ident.ident
            } else {
                panic!("expected identifiable parameter name");
            }
        })
        .collect();

    // Handle no-param case (empty vec![])
    let param_bindings = if param_names.is_empty() {
        quote! { vec![] }
    } else {
        quote! { vec![#(Box::new(#param_names) as dao::Param),*] }
    };

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
