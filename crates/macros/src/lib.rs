use proc_macro::TokenStream;
use quote::quote;
use syn::Token;
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, spanned::Spanned};

fn expand_register(input: &syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ty_ident = &input.ident;

    // Registering a generic type doesn't make sense (no concrete T to encode/decode).
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new(
            input.generics.span(),
            "Type does not support generic types",
        ));
    }

    Ok(quote! {
        const _: () = {
            fn register(process: &mut ::awrk_world::core::Process) {
                let _ = process.register_component::<#ty_ident>();
            }

            ::awrk_world::inventory::submit! {
                ::awrk_world::registration::TypeRegistrar { register }
            }
        };
    })
}

fn path_ends_with(path: &syn::Path, segments: &[&str]) -> bool {
    let n = segments.len();
    if path.segments.len() < n {
        return false;
    }
    let start = path.segments.len() - n;
    path.segments
        .iter()
        .skip(start)
        .zip(segments.iter())
        .all(|(seg, expected)| seg.ident == expected)
}

fn has_derive(derives: &[syn::Path], suffix: &[&str], last_only: &str) -> bool {
    derives.iter().any(|d| {
        path_ends_with(d, suffix) || (d.segments.len() == 1 && d.segments[0].ident == last_only)
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TypeMode {
    Full,
    Opaque,
}

fn add_wire_derives(attrs: &mut Vec<syn::Attribute>, mode: TypeMode) -> syn::Result<()> {
    let mut derives: Vec<syn::Path> = Vec::new();
    let mut out_attrs: Vec<syn::Attribute> = Vec::with_capacity(attrs.len() + 1);

    for attr in attrs.drain(..) {
        if attr.path().is_ident("derive") {
            let parsed =
                attr.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)?;
            derives.extend(parsed);
        } else {
            out_attrs.push(attr);
        }
    }

    match mode {
        TypeMode::Full => {
            if !has_derive(&derives, &["awrk_datex", "Encode"], "Encode") {
                derives.push(syn::parse_quote!(awrk_datex::Encode));
            }
            if !has_derive(&derives, &["awrk_datex", "Decode"], "Decode") {
                derives.push(syn::parse_quote!(awrk_datex::Decode));
            }
            if !has_derive(&derives, &["awrk_datex", "Patch"], "Patch") {
                derives.push(syn::parse_quote!(awrk_datex::Patch));
            }
            if !has_derive(&derives, &["awrk_schema_macros", "Schema"], "Schema") {
                derives.push(syn::parse_quote!(awrk_schema_macros::Schema));
            }
        }
        TypeMode::Opaque => {
            // Intentionally do not implement Encode/Decode/Patch.
            if has_derive(&derives, &["awrk_datex", "Encode"], "Encode")
                || has_derive(&derives, &["awrk_datex", "Decode"], "Decode")
                || has_derive(&derives, &["awrk_datex", "Patch"], "Patch")
                || has_derive(&derives, &["awrk_schema_macros", "Schema"], "Schema")
            {
                return Err(syn::Error::new(
                    attrs
                        .first()
                        .map(|a| a.span())
                        .unwrap_or_else(proc_macro2::Span::call_site),
                    "#[awrk_macros::Type(Opaque)] must not derive Encode/Decode/Patch/Schema; it generates an opaque Schema impl and is not serializable",
                ));
            }
        }
    }

    out_attrs.insert(0, syn::parse_quote!(#[derive(#(#derives),*)]));
    *attrs = out_attrs;
    Ok(())
}

/// Type attribute macro.
///
/// Usage:
///
/// ```ignore
/// #[awrk_macros::Type]
/// #[derive(Debug)]
/// enum MyType { ... }
/// ```
///
/// Expands to:
/// - inject `#[derive(awrk_datex::{Encode,Decode,Patch}, awrk_schema_macros::Schema)]`
/// - auto-register the type into `Process` via `register_component::<T>()`
#[proc_macro_attribute]
#[allow(non_snake_case)]
pub fn Type(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mode = if attr.is_empty() {
        TypeMode::Full
    } else {
        let ident = parse_macro_input!(attr as syn::Ident);
        if ident == "Opaque" {
            TypeMode::Opaque
        } else {
            return syn::Error::new_spanned(
                ident,
                "unsupported #[awrk_macros::Type(..)] argument; expected `Opaque`",
            )
            .to_compile_error()
            .into();
        }
    };

    let mut input = parse_macro_input!(item as syn::DeriveInput);

    if let Err(err) = add_wire_derives(&mut input.attrs, mode) {
        return err.to_compile_error().into();
    }

    let register = match mode {
        TypeMode::Full => match expand_register(&input) {
            Ok(tokens) => tokens,
            Err(err) => return err.to_compile_error().into(),
        },
        TypeMode::Opaque => {
            let ty_ident = &input.ident;

            // Registering a generic type doesn't make sense (no concrete T).
            if !input.generics.params.is_empty() {
                return syn::Error::new(
                    input.generics.span(),
                    "Type(Opaque) does not support generic types",
                )
                .to_compile_error()
                .into();
            }

            quote! {
                impl ::awrk_datex_schema::Schema for #ty_ident {
                    fn wire_schema(builder: &mut ::awrk_datex_schema::SchemaBuilder) -> ::awrk_datex_schema::TypeId {
                        builder.register_opaque_type(::core::any::type_name::<Self>())
                    }
                }

                const _: () = {
                    fn register(process: &mut ::awrk_world::core::Process) {
                        let _ = process.register_component_opaque::<#ty_ident>();
                    }

                    ::awrk_world::inventory::submit! {
                        ::awrk_world::registration::TypeRegistrar { register }
                    }
                };
            }
        }
    };

    quote! {
        #input
        #register
    }
    .into()
}
