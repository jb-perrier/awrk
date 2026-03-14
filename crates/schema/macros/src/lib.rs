use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Data, DataEnum, DataStruct, DeriveInput, Fields, parse_macro_input};

fn parse_field_rename(attrs: &[Attribute]) -> syn::Result<Option<syn::LitStr>> {
    let mut rename: Option<syn::LitStr> = None;

    for attr in attrs {
        if !attr.path().is_ident("awrk_datex") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                rename = Some(meta.value()?.parse()?);
                return Ok(());
            }
            if meta.path.is_ident("type_name") {
                return Err(meta
                    .error("awrk_datex(type_name=...) is only valid on the type, not on fields"));
            }
            Err(meta.error("unknown awrk_datex field attribute key"))
        })?;
    }

    Ok(rename)
}

fn effective_field_name(field: &syn::Field) -> syn::Result<syn::LitStr> {
    if let Some(rename) = parse_field_rename(&field.attrs)? {
        return Ok(rename);
    }

    let Some(ident) = &field.ident else {
        return Err(syn::Error::new_spanned(
            field,
            "awrk_datex(rename=...) is only supported on named fields",
        ));
    };
    Ok(syn::LitStr::new(&ident.to_string(), ident.span()))
}

#[proc_macro_derive(Schema, attributes(awrk_datex))]
pub fn derive_wire_schema(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_wire_schema(&input) {
        Ok(ts) => ts,
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand_wire_schema(input: &DeriveInput) -> syn::Result<TokenStream> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "Schema derive does not support generics",
        ));
    }

    let params = parse_type_attrs(&input.attrs)?;
    let ident = &input.ident;

    let body = match &input.data {
        Data::Struct(s) => expand_struct_schema(s, &params.type_name)?,
        Data::Enum(e) => expand_enum_schema(e, &params.type_name)?,
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                input,
                "Schema derive does not support unions",
            ));
        }
    };

    Ok(
        quote! {
            impl ::awrk_datex_schema::Schema for #ident {
                fn wire_schema(builder: &mut ::awrk_datex_schema::SchemaBuilder) -> ::awrk_datex_schema::TypeId {
                    #body
                }
            }
        }
        .into(),
    )
}

fn expand_struct_schema(
    s: &DataStruct,
    type_name: &proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    match &s.fields {
        Fields::Named(fields) => {
            let mut field_type_lets = Vec::new();
            let mut field_entries = Vec::new();

            for f in &fields.named {
                let Some(name) = &f.ident else { continue };
                let field_name_lit = effective_field_name(f)?;
                let tmp = format_ident!("__upi_wire_schema_field_{}", name);
                let ty = &f.ty;

                field_type_lets.push(quote! {
                    let #tmp = <#ty as ::awrk_datex_schema::Schema>::wire_schema(builder);
                });
                field_entries.push(quote! { (#field_name_lit, #tmp, 0u32) });
            }

            Ok(quote! {
                let __upi_wire_type_name = #type_name;
                #(#field_type_lets)*
                builder.register_struct_type(__upi_wire_type_name, [#(#field_entries),*])
            })
        }
        Fields::Unnamed(fields) => {
            // Tuple/newtype/unit structs encode as a tuple type in schema.
            for f in &fields.unnamed {
                if parse_field_rename(&f.attrs)?.is_some() {
                    return Err(syn::Error::new_spanned(
                        f,
                        "awrk_datex(rename=...) is not supported on tuple struct fields",
                    ));
                }
            }

            let mut item_type_lets = Vec::new();
            let mut item_idents = Vec::new();
            for (idx, f) in fields.unnamed.iter().enumerate() {
                let ty = &f.ty;
                let tmp = format_ident!("__upi_wire_schema_tuple_item_{}", idx);
                item_idents.push(tmp.clone());
                item_type_lets.push(quote! {
                    let #tmp = <#ty as ::awrk_datex_schema::Schema>::wire_schema(builder);
                });
            }

            Ok(quote! {
                let __upi_wire_type_name = #type_name;
                #(#item_type_lets)*
                builder.register_tuple_type(__upi_wire_type_name, vec![#(#item_idents),*])
            })
        }
        Fields::Unit => Ok(quote! {
            let __upi_wire_type_name = #type_name;
            builder.register_tuple_type(__upi_wire_type_name, vec![])
        }),
    }
}

fn expand_enum_schema(
    e: &DataEnum,
    type_name: &proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut variant_type_lets = Vec::new();
    let mut variant_entries = Vec::new();

    for (idx, v) in e.variants.iter().enumerate() {
        let vname = v.ident.to_string();

        match &v.fields {
            Fields::Unit => {
                variant_entries.push(quote! { (#vname, None) });
            }
            Fields::Unnamed(fields) => {
                for field in &fields.unnamed {
                    if parse_field_rename(&field.attrs)?.is_some() {
                        return Err(syn::Error::new_spanned(
                            field,
                            "awrk_datex(rename=...) is not supported on tuple enum variant fields",
                        ));
                    }
                }

                if fields.unnamed.len() == 1 {
                    let ty = &fields.unnamed.first().unwrap().ty;
                    let tmp = format_ident!("__upi_wire_schema_variant_{}", idx);
                    variant_type_lets.push(quote! {
                        let #tmp = <#ty as ::awrk_datex_schema::Schema>::wire_schema(builder);
                    });
                    variant_entries.push(quote! { (#vname, Some(#tmp)) });
                } else {
                    let tmp = format_ident!("__upi_wire_schema_variant_{}", idx);
                    let variant_type_name =
                        format_ident!("__upi_wire_schema_variant_type_name_{}", idx);
                    let mut item_type_lets = Vec::new();
                    let mut item_idents = Vec::new();

                    for (field_idx, field) in fields.unnamed.iter().enumerate() {
                        let ty = &field.ty;
                        let item_tmp =
                            format_ident!("__upi_wire_schema_variant_{}_item_{}", idx, field_idx);
                        item_idents.push(item_tmp.clone());
                        item_type_lets.push(quote! {
                            let #item_tmp = <#ty as ::awrk_datex_schema::Schema>::wire_schema(builder);
                        });
                    }

                    variant_type_lets.push(quote! {
                        let #variant_type_name = ::std::format!("{}::{}", __upi_wire_type_name, #vname);
                        #(#item_type_lets)*
                        let #tmp = builder.register_tuple_type(&#variant_type_name, vec![#(#item_idents),*]);
                    });
                    variant_entries.push(quote! { (#vname, Some(#tmp)) });
                }
            }
            Fields::Named(fields) => {
                let tmp = format_ident!("__upi_wire_schema_variant_{}", idx);
                let variant_type_name =
                    format_ident!("__upi_wire_schema_variant_type_name_{}", idx);

                let mut field_type_lets = Vec::new();
                let mut field_entries = Vec::new();

                for (field_idx, f) in fields.named.iter().enumerate() {
                    let fname = f
                        .ident
                        .as_ref()
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| field_idx.to_string());
                    let field_name_lit = effective_field_name(f)?;
                    let ftmp = format_ident!("__upi_wire_schema_variant_{}_field_{}", idx, fname);
                    let ty = &f.ty;

                    field_type_lets.push(quote! {
                        let #ftmp = <#ty as ::awrk_datex_schema::Schema>::wire_schema(builder);
                    });
                    field_entries.push(quote! { (#field_name_lit, #ftmp, 0u32) });
                }

                variant_type_lets.push(quote! {
                    let #variant_type_name = ::std::format!("{}::{}", __upi_wire_type_name, #vname);
                    #(#field_type_lets)*
                    let #tmp = builder.register_struct_type(&#variant_type_name, [#(#field_entries),*]);
                });
                variant_entries.push(quote! { (#vname, Some(#tmp)) });
            }
        }
    }

    Ok(quote! {
        let __upi_wire_type_name = #type_name;
        #(#variant_type_lets)*
        builder.register_enum_type_with_repr(
            __upi_wire_type_name,
            ::awrk_datex_schema::EnumRepr::IndexKeyedSingleEntryMap,
            [#(#variant_entries),*],
        )
    })
}

struct TypeParams {
    type_name: proc_macro2::TokenStream,
}

fn parse_type_attrs(attrs: &[Attribute]) -> syn::Result<TypeParams> {
    let mut type_name: Option<syn::LitStr> = None;

    for attr in attrs {
        if !attr.path().is_ident("awrk_datex") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("type_name") {
                type_name = Some(meta.value()?.parse()?);
                return Ok(());
            }
            if meta.path.is_ident("schema_salt") {
                return Err(
                    meta.error("awrk_datex(schema_salt=...) was removed; delete this attribute")
                );
            }
            Err(meta.error("unknown awrk_datex attribute key"))
        })?;
    }

    let type_name = match type_name {
        Some(lit) => quote! { #lit },
        None => quote! { ::core::any::type_name::<Self>() },
    };

    Ok(TypeParams { type_name })
}
