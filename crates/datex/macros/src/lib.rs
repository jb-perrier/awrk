use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Data, DataEnum, DataStruct, DeriveInput, Fields, parse_macro_input};

fn is_option_type(ty: &syn::Type) -> bool {
    let syn::Type::Path(type_path) = ty else {
        return false;
    };
    let Some(seg) = type_path.path.segments.last() else {
        return false;
    };
    if seg.ident != "Option" {
        return false;
    }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return false;
    };
    args.args
        .iter()
        .any(|arg| matches!(arg, syn::GenericArgument::Type(_)))
}

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

#[proc_macro_derive(Encode, attributes(awrk_datex))]
pub fn Encode(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_wire_encode(&input) {
        Ok(ts) => ts,
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro_derive(Decode, attributes(awrk_datex))]
pub fn Decode(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_wire_decode(&input) {
        Ok(ts) => ts,
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro_derive(Patch, attributes(awrk_datex))]
pub fn derive_wire_patch(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_wire_patch(&input) {
        Ok(ts) => ts,
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand_wire_encode(input: &DeriveInput) -> syn::Result<TokenStream> {
    let params = parse_type_attrs(&input.attrs)?;

    let ident = &input.ident;

    let body = match &input.data {
        Data::Struct(s) => expand_struct_encode(ident, s, &params.type_name)?,
        Data::Enum(e) => expand_enum_encode(ident, e, &params.type_name)?,
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                input,
                "Encode derive does not support unions",
            ));
        }
    };

    Ok(quote! {
        impl ::awrk_datex::Encode for #ident {
            fn wire_encode(&self, enc: &mut ::awrk_datex::codec::Encoder) -> ::awrk_datex::Result<()> {
                #body
            }
        }
    }
    .into())
}

fn expand_wire_decode(input: &DeriveInput) -> syn::Result<TokenStream> {
    let params = parse_type_attrs(&input.attrs)?;

    let ident = &input.ident;

    let body = match &input.data {
        Data::Struct(s) => expand_struct_decode(ident, s, &params.type_name)?,
        Data::Enum(e) => expand_enum_decode(ident, e, &params.type_name)?,
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                input,
                "Decode derive does not support unions",
            ));
        }
    };

    Ok(quote! {
        impl<'a> ::awrk_datex::Decode<'a> for #ident {
            fn wire_decode(value: ::awrk_datex::value::SerializedValueRef<'a>) -> ::awrk_datex::Result<Self> {
                #body
            }
        }
    }
    .into())
}

fn expand_wire_patch(input: &DeriveInput) -> syn::Result<TokenStream> {
    let params = parse_type_attrs(&input.attrs)?;

    let ident = &input.ident;

    let patch_body = match &input.data {
        Data::Struct(s) => expand_struct_patch(ident, s, &params.type_name)?,
        Data::Enum(e) => expand_enum_patch(ident, e)?,
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                input,
                "Patch derive does not support unions",
            ));
        }
    };

    let validate_body = match &input.data {
        Data::Struct(s) => expand_struct_patch_validate(ident, s, &params.type_name)?,
        Data::Enum(e) => expand_enum_patch_validate(ident, e)?,
        Data::Union(_) => unreachable!(),
    };

    Ok(quote! {
        impl ::awrk_datex::Patch for #ident {
            fn wire_patch<'a>(&mut self, patch: ::awrk_datex::value::SerializedValueRef<'a>) -> ::awrk_datex::Result<()> {
                #patch_body
            }
        }

        impl ::awrk_datex::PatchValidate for #ident {
            fn wire_patch_validate<'a>(&self, patch: ::awrk_datex::value::SerializedValueRef<'a>) -> ::awrk_datex::Result<()> {
                #validate_body
            }
        }
    }
    .into())
}

fn expand_struct_encode(
    _ident: &syn::Ident,
    s: &DataStruct,
    type_name: &proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    match &s.fields {
        Fields::Named(fields) => {
            let mut field_id_lets = Vec::new();
            let mut order_entries = Vec::new();
            let mut match_arms = Vec::new();

            for (idx, f) in fields.named.iter().enumerate() {
                let Some(name) = &f.ident else { continue };
                let idx_lit = idx;
                let fid_ident = format_ident!("__upi_wire_fid_{}", name);
                let field_name_lit = effective_field_name(f)?;

                field_id_lets.push(quote! {
                    let #fid_ident = ::awrk_datex_schema::field_id(__upi_wire_ty, #field_name_lit).0;
                });
                order_entries.push(quote! { (#fid_ident, #idx_lit) });
                match_arms.push(quote! {
                    #idx_lit => { ::awrk_datex::Encode::wire_encode(&self.#name, enc)?; }
                });
            }

            let field_count = fields.named.len();

            Ok(quote! {
                let __upi_wire_ty = ::awrk_datex_schema::type_id(#type_name);
                #(#field_id_lets)*

                let mut __upi_wire_order = [#(#order_entries),*];
                {
                    let __upi_wire_order_slice: &mut [(u64, usize)] = &mut __upi_wire_order;
                    __upi_wire_order_slice.sort_unstable_by_key(|(fid, _)| *fid);
                }

                enc.map(#field_count as u32, |w| {
                    for &(fid, idx) in __upi_wire_order.iter() {
                        w.entry(
                            |enc| {
                                enc.u64(fid);
                                Ok(())
                            },
                            |enc| {
                                match idx {
                                    #(#match_arms)*
                                    _ => unreachable!(),
                                }
                                Ok(())
                            },
                        )?;
                    }
                    Ok(())
                })
            })
        }
        Fields::Unnamed(fields) => {
            for f in &fields.unnamed {
                if parse_field_rename(&f.attrs)?.is_some() {
                    return Err(syn::Error::new_spanned(
                        f,
                        "awrk_datex(rename=...) is not supported on tuple struct fields",
                    ));
                }
            }

            let field_count = fields.unnamed.len();
            let mut writes = Vec::new();
            for (idx, _f) in fields.unnamed.iter().enumerate() {
                let index = syn::Index::from(idx);
                writes.push(quote! {
                    w.value(|enc| ::awrk_datex::Encode::wire_encode(&self.#index, enc))?;
                });
            }

            Ok(quote! {
                enc.array(#field_count as u32, |w| {
                    #(#writes)*
                    Ok(())
                })
            })
        }
        Fields::Unit => Ok(quote! {
            enc.array(0, |_w| Ok(()))
        }),
    }
}

fn expand_struct_decode(
    _ident: &syn::Ident,
    s: &DataStruct,
    type_name: &proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    match &s.fields {
        Fields::Named(fields) => {
            let mut decode_fields = Vec::new();
            let mut init_fields = Vec::new();

            for f in &fields.named {
                let Some(name) = &f.ident else { continue };
                let ty = &f.ty;
                let fid_name = effective_field_name(f)?;
                let tmp = format_ident!("__upi_wire_field_{}", name);

                if is_option_type(ty) {
                    decode_fields.push(quote! {
                        let #tmp = __upi_wire_map_get(
                            s,
                            ::awrk_datex_schema::field_id(__upi_wire_ty, #fid_name).0,
                        )?;
                        let #tmp: #ty = match #tmp {
                            Some(v) => ::awrk_datex::Decode::wire_decode(v)?,
                            None => None,
                        };
                    });
                } else {
                    decode_fields.push(quote! {
                        let #tmp = __upi_wire_map_get(
                            s,
                            ::awrk_datex_schema::field_id(__upi_wire_ty, #fid_name).0,
                        )?
                        .ok_or(::awrk_datex::WireError::Malformed("missing struct field"))?;
                        let #tmp: #ty = ::awrk_datex::Decode::wire_decode(#tmp)?;
                    });
                }
                init_fields.push(quote! { #name: #tmp });
            }

            Ok(quote! {
                let __upi_wire_ty = ::awrk_datex_schema::type_id(#type_name);
                let s = value.as_map().ok_or(::awrk_datex::WireError::Malformed("expected map"))?;

                fn __upi_wire_map_get<'a>(
                    map: ::awrk_datex::value::MapRef<'a>,
                    key: u64,
                ) -> ::awrk_datex::Result<Option<::awrk_datex::value::SerializedValueRef<'a>>> {
                    let mut found = None;
                    let mut it = map.iter_pairs();
                    while let Some(entry) = it.next() {
                        let (k, v) = entry?;
                        let Some(k) = k.as_u64() else {
                            return Err(::awrk_datex::WireError::Malformed("expected u64 map key"));
                        };
                        if k == key {
                            found = Some(v);
                        }
                    }
                    it.finish()?;
                    Ok(found)
                }

                #(#decode_fields)*
                Ok(Self { #(#init_fields),* })
            })
        }
        Fields::Unnamed(fields) => {
            for f in &fields.unnamed {
                if parse_field_rename(&f.attrs)?.is_some() {
                    return Err(syn::Error::new_spanned(
                        f,
                        "awrk_datex(rename=...) is not supported on tuple struct fields",
                    ));
                }
            }

            let field_count = fields.unnamed.len();
            let mut decode_lets = Vec::new();
            let mut init_args = Vec::new();

            for (idx, f) in fields.unnamed.iter().enumerate() {
                let ty = &f.ty;
                let tmp = format_ident!("__upi_wire_tuple_field_{}", idx);
                decode_lets.push(quote! {
                    let #tmp = __upi_wire_it
                        .next()
                            .ok_or(::awrk_datex::WireError::Malformed("missing tuple field"))??;
                        let #tmp: #ty = ::awrk_datex::Decode::wire_decode(#tmp)?;
                });
                init_args.push(quote! { #tmp });
            }

            Ok(quote! {
                let a = value.as_array().ok_or(::awrk_datex::WireError::Malformed("expected array"))?;
                if a.len() != #field_count {
                    return Err(::awrk_datex::WireError::Malformed("tuple length mismatch"));
                }
                let mut __upi_wire_it = a.iter();
                #(#decode_lets)*
                __upi_wire_it.finish()?;
                Ok(Self(#(#init_args),*))
            })
        }
        Fields::Unit => Ok(quote! {
            let a = value.as_array().ok_or(::awrk_datex::WireError::Malformed("expected array"))?;
            if a.len() != 0 {
                return Err(::awrk_datex::WireError::Malformed("tuple length mismatch"));
            }
            let mut __upi_wire_it = a.iter();
            __upi_wire_it.finish()?;
            Ok(Self)
        }),
    }
}

fn expand_struct_patch(
    _ident: &syn::Ident,
    s: &DataStruct,
    type_name: &proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    match &s.fields {
        Fields::Named(fields) => {
            let mut apply_fields = Vec::new();
            let mut field_name_lits = Vec::new();
            let field_count = fields.named.len();

            for (idx, f) in fields.named.iter().enumerate() {
                let Some(name) = &f.ident else { continue };
                let idx_lit = idx;
                let field_name_lit = effective_field_name(f)?;
                field_name_lits.push(field_name_lit);

                apply_fields.push(quote! {
                    if let Some(entry_value) = __upi_wire_map_get(__upi_wire_patch, __upi_wire_field_keys[#idx_lit])? {
                        ::awrk_datex::Patch::wire_patch(&mut self.#name, entry_value)?;
                    }
                });
            }

            Ok(quote! {
                let __upi_wire_patch = patch
                    .as_map()
                    .ok_or(::awrk_datex::WireError::Malformed("expected map patch"))?;

                static __UPI_WIRE_FIELD_KEYS: ::std::sync::OnceLock<[u64; #field_count]> = ::std::sync::OnceLock::new();

                let __upi_wire_field_keys = __UPI_WIRE_FIELD_KEYS.get_or_init(|| {
                    let __upi_wire_ty = ::awrk_datex_schema::type_id(#type_name);
                    [
                        #(::awrk_datex_schema::field_id(__upi_wire_ty, #field_name_lits).0),*
                    ]
                });

                fn __upi_wire_map_get<'a>(
                    map: ::awrk_datex::value::MapRef<'a>,
                    key: u64,
                ) -> ::awrk_datex::Result<Option<::awrk_datex::value::SerializedValueRef<'a>>> {
                    let mut found = None;
                    let mut it = map.iter_pairs();
                    while let Some(entry) = it.next() {
                        let (k, v) = entry?;
                        let Some(k) = k.as_u64() else {
                            return Err(::awrk_datex::WireError::Malformed("expected u64 map key"));
                        };
                        if k == key {
                            found = Some(v);
                        }
                    }
                    it.finish()?;
                    Ok(found)
                }

                #(#apply_fields)*

                Ok(())
            })
        }
        Fields::Unnamed(fields) => {
            for f in &fields.unnamed {
                if parse_field_rename(&f.attrs)?.is_some() {
                    return Err(syn::Error::new_spanned(
                        f,
                        "awrk_datex(rename=...) is not supported on tuple struct fields",
                    ));
                }
            }

            Ok(quote! {
                *self = <Self as ::awrk_datex::Decode>::wire_decode(patch)?;
                Ok(())
            })
        }
        Fields::Unit => Ok(quote! {
            *self = <Self as ::awrk_datex::Decode>::wire_decode(patch)?;
            Ok(())
        }),
    }
}

fn expand_struct_patch_validate(
    _ident: &syn::Ident,
    s: &DataStruct,
    type_name: &proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    match &s.fields {
        Fields::Named(fields) => {
            let mut validate_fields = Vec::new();
            let mut field_name_lits = Vec::new();
            let field_count = fields.named.len();

            for (idx, f) in fields.named.iter().enumerate() {
                let Some(name) = &f.ident else { continue };
                let idx_lit = idx;
                let field_name_lit = effective_field_name(f)?;
                field_name_lits.push(field_name_lit);

                validate_fields.push(quote! {
                    if let Some(entry_value) = __upi_wire_map_get(__upi_wire_patch, __upi_wire_field_keys[#idx_lit])? {
                        ::awrk_datex::PatchValidate::wire_patch_validate(&self.#name, entry_value)?;
                    }
                });
            }

            Ok(quote! {
                let __upi_wire_patch = patch
                    .as_map()
                    .ok_or(::awrk_datex::WireError::Malformed("expected map patch"))?;

                static __UPI_WIRE_FIELD_KEYS: ::std::sync::OnceLock<[u64; #field_count]> = ::std::sync::OnceLock::new();

                let __upi_wire_field_keys = __UPI_WIRE_FIELD_KEYS.get_or_init(|| {
                    let __upi_wire_ty = ::awrk_datex_schema::type_id(#type_name);
                    [
                        #(::awrk_datex_schema::field_id(__upi_wire_ty, #field_name_lits).0),*
                    ]
                });

                fn __upi_wire_map_get<'a>(
                    map: ::awrk_datex::value::MapRef<'a>,
                    key: u64,
                ) -> ::awrk_datex::Result<Option<::awrk_datex::value::SerializedValueRef<'a>>> {
                    let mut found = None;
                    let mut it = map.iter_pairs();
                    while let Some(entry) = it.next() {
                        let (k, v) = entry?;
                        let Some(k) = k.as_u64() else {
                            return Err(::awrk_datex::WireError::Malformed("expected u64 map key"));
                        };
                        if k == key {
                            found = Some(v);
                        }
                    }
                    it.finish()?;
                    Ok(found)
                }

                // Touch every entry to force full structural validation (even for unknown keys).
                let mut __upi_wire_it = __upi_wire_patch.iter_pairs();
                while let Some(entry) = __upi_wire_it.next() {
                    let (k, _v) = entry?;
                    if k.as_u64().is_none() {
                        return Err(::awrk_datex::WireError::Malformed("expected u64 map key"));
                    }
                }
                __upi_wire_it.finish()?;

                #(#validate_fields)*

                Ok(())
            })
        }
        Fields::Unnamed(fields) => {
            for f in &fields.unnamed {
                if parse_field_rename(&f.attrs)?.is_some() {
                    return Err(syn::Error::new_spanned(
                        f,
                        "awrk_datex(rename=...) is not supported on tuple struct fields",
                    ));
                }
            }

            Ok(quote! {
                let _ = <Self as ::awrk_datex::Decode>::wire_decode(patch)?;
                Ok(())
            })
        }
        Fields::Unit => Ok(quote! {
            let _ = <Self as ::awrk_datex::Decode>::wire_decode(patch)?;
            Ok(())
        }),
    }
}

fn expand_enum_encode(
    _ident: &syn::Ident,
    e: &DataEnum,
    type_name: &proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut arms = Vec::new();

    for (idx, v) in e.variants.iter().enumerate() {
        let v_ident = &v.ident;
        let variant_index = idx as u32;
        let variant_name = syn::LitStr::new(&v.ident.to_string(), v.ident.span());

        match &v.fields {
            Fields::Unit => {
                arms.push(quote! {
                    Self::#v_ident => {
                        enc.map(1, |w| {
                            w.entry(
                                |enc| {
                                    enc.u64(#variant_index as u64);
                                    Ok(())
                                },
                                |enc| {
                                    enc.bool(true);
                                    Ok(())
                                },
                            )
                        })
                    }
                });
            }
            Fields::Unnamed(f) => {
                for field in &f.unnamed {
                    if parse_field_rename(&field.attrs)?.is_some() {
                        return Err(syn::Error::new_spanned(
                            field,
                            "awrk_datex(rename=...) is not supported on tuple enum variant fields",
                        ));
                    }
                }

                if f.unnamed.len() == 1 {
                    let binding = format_ident!("__upi_wire_payload");
                    arms.push(quote! {
                        Self::#v_ident(#binding) => {
                            enc.map(1, |w| {
                                w.entry(
                                    |enc| {
                                        enc.u64(#variant_index as u64);
                                        Ok(())
                                    },
                                    |enc| ::awrk_datex::Encode::wire_encode(#binding, enc),
                                )
                            })
                        }
                    });
                } else {
                    let field_count = f.unnamed.len();
                    let bindings: Vec<_> = (0..field_count)
                        .map(|field_idx| format_ident!("__upi_wire_payload_{}", field_idx))
                        .collect();
                    let writes = bindings.iter().map(|binding| {
                        quote! {
                            w.value(|enc| ::awrk_datex::Encode::wire_encode(#binding, enc))?;
                        }
                    });

                    arms.push(quote! {
                        Self::#v_ident(#(#bindings),*) => {
                            enc.map(1, |w| {
                                w.entry(
                                    |enc| {
                                        enc.u64(#variant_index as u64);
                                        Ok(())
                                    },
                                    |enc| {
                                        enc.array(#field_count as u32, |w| {
                                            #(#writes)*
                                            Ok(())
                                        })
                                    },
                                )
                            })
                        }
                    });
                }
            }
            Fields::Named(fields) => {
                let field_count = fields.named.len();
                let mut binding_idents = Vec::new();
                let mut field_id_lets = Vec::new();
                let mut order_entries = Vec::new();
                let mut match_arms = Vec::new();

                for (field_idx, field) in fields.named.iter().enumerate() {
                    let Some(name) = &field.ident else { continue };
                    let binding = format_ident!("__upi_wire_variant_field_{}", name);
                    let fid_ident = format_ident!("__upi_wire_fid_{}", name);
                    let field_name_lit = effective_field_name(field)?;

                    binding_idents.push(quote! { #name: #binding });
                    field_id_lets.push(quote! {
                        let #fid_ident = ::awrk_datex_schema::field_id(__upi_wire_ty, #field_name_lit).0;
                    });
                    order_entries.push(quote! { (#fid_ident, #field_idx) });
                    match_arms.push(quote! {
                        #field_idx => { ::awrk_datex::Encode::wire_encode(#binding, enc)?; }
                    });
                }

                arms.push(quote! {
                    Self::#v_ident { #(#binding_idents),* } => {
                        let __upi_wire_ty_name = ::std::format!("{}::{}", #type_name, #variant_name);
                        let __upi_wire_ty = ::awrk_datex_schema::type_id(&__upi_wire_ty_name);
                        #(#field_id_lets)*

                        let mut __upi_wire_order = [#(#order_entries),*];
                        {
                            let __upi_wire_order_slice: &mut [(u64, usize)] = &mut __upi_wire_order;
                            __upi_wire_order_slice.sort_unstable_by_key(|(fid, _)| *fid);
                        }

                        enc.map(1, |w| {
                            w.entry(
                                |enc| {
                                    enc.u64(#variant_index as u64);
                                    Ok(())
                                },
                                |enc| {
                                    enc.map(#field_count as u32, |w| {
                                        for &(fid, idx) in __upi_wire_order.iter() {
                                            w.entry(
                                                |enc| {
                                                    enc.u64(fid);
                                                    Ok(())
                                                },
                                                |enc| {
                                                    match idx {
                                                        #(#match_arms)*
                                                        _ => unreachable!(),
                                                    }
                                                    Ok(())
                                                },
                                            )?;
                                        }
                                        Ok(())
                                    })
                                },
                            )
                        })
                    }
                });
            }
        }
    }

    Ok(quote! {
        match self {
            #(#arms),*
        }
    })
}

fn expand_enum_decode(
    _ident: &syn::Ident,
    e: &DataEnum,
    type_name: &proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut arms = Vec::new();

    for (idx, v) in e.variants.iter().enumerate() {
        let v_ident = &v.ident;
        let variant_index = idx as u32;
        let variant_name = syn::LitStr::new(&v.ident.to_string(), v.ident.span());

        match &v.fields {
            Fields::Unit => {
                arms.push(quote! {
                    #variant_index => {
                        match entry_value {
                            ::awrk_datex::value::SerializedValueRef::Bool(true) => Ok(Self::#v_ident),
                            _ => Err(::awrk_datex::WireError::Malformed("unexpected unit variant payload")),
                        }
                    }
                });
            }
            Fields::Unnamed(f) => {
                for field in &f.unnamed {
                    if parse_field_rename(&field.attrs)?.is_some() {
                        return Err(syn::Error::new_spanned(
                            field,
                            "awrk_datex(rename=...) is not supported on tuple enum variant fields",
                        ));
                    }
                }

                if f.unnamed.len() == 1 {
                    let ty = &f.unnamed.first().unwrap().ty;
                    arms.push(quote! {
                        #variant_index => {
                            let v: #ty = ::awrk_datex::Decode::wire_decode(entry_value)?;
                            Ok(Self::#v_ident(v))
                        }
                    });
                } else {
                    let field_count = f.unnamed.len();
                    let mut decode_lets = Vec::new();
                    let mut init_args = Vec::new();

                    for (field_idx, field) in f.unnamed.iter().enumerate() {
                        let ty = &field.ty;
                        let tmp = format_ident!("__upi_wire_tuple_field_{}", field_idx);
                        decode_lets.push(quote! {
                            let #tmp = __upi_wire_it
                                .next()
                                .ok_or(::awrk_datex::WireError::Malformed("missing tuple field"))??;
                            let #tmp: #ty = ::awrk_datex::Decode::wire_decode(#tmp)?;
                        });
                        init_args.push(quote! { #tmp });
                    }

                    arms.push(quote! {
                        #variant_index => {
                            let a = entry_value
                                .as_array()
                                .ok_or(::awrk_datex::WireError::Malformed("expected array"))?;
                            if a.len() != #field_count {
                                return Err(::awrk_datex::WireError::Malformed("tuple length mismatch"));
                            }
                            let mut __upi_wire_it = a.iter();
                            #(#decode_lets)*
                            __upi_wire_it.finish()?;
                            Ok(Self::#v_ident(#(#init_args),*))
                        }
                    });
                }
            }
            Fields::Named(fields) => {
                let mut decode_fields = Vec::new();
                let mut init_fields = Vec::new();

                for field in &fields.named {
                    let Some(name) = &field.ident else { continue };
                    let ty = &field.ty;
                    let field_name_lit = effective_field_name(field)?;
                    let tmp = format_ident!("__upi_wire_field_{}", name);

                    if is_option_type(ty) {
                        decode_fields.push(quote! {
                            let #tmp = __upi_wire_map_get(
                                s,
                                ::awrk_datex_schema::field_id(__upi_wire_ty, #field_name_lit).0,
                            )?;
                            let #tmp: #ty = match #tmp {
                                Some(v) => ::awrk_datex::Decode::wire_decode(v)?,
                                None => None,
                            };
                        });
                    } else {
                        decode_fields.push(quote! {
                            let #tmp = __upi_wire_map_get(
                                s,
                                ::awrk_datex_schema::field_id(__upi_wire_ty, #field_name_lit).0,
                            )?
                            .ok_or(::awrk_datex::WireError::Malformed("missing struct field"))?;
                            let #tmp: #ty = ::awrk_datex::Decode::wire_decode(#tmp)?;
                        });
                    }

                    init_fields.push(quote! { #name: #tmp });
                }

                arms.push(quote! {
                    #variant_index => {
                        let __upi_wire_ty_name = ::std::format!("{}::{}", #type_name, #variant_name);
                        let __upi_wire_ty = ::awrk_datex_schema::type_id(&__upi_wire_ty_name);
                        let s = entry_value
                            .as_map()
                            .ok_or(::awrk_datex::WireError::Malformed("expected map"))?;

                        fn __upi_wire_map_get<'a>(
                            map: ::awrk_datex::value::MapRef<'a>,
                            key: u64,
                        ) -> ::awrk_datex::Result<Option<::awrk_datex::value::SerializedValueRef<'a>>> {
                            let mut found = None;
                            let mut it = map.iter_pairs();
                            while let Some(entry) = it.next() {
                                let (k, v) = entry?;
                                let Some(k) = k.as_u64() else {
                                    return Err(::awrk_datex::WireError::Malformed("expected u64 map key"));
                                };
                                if k == key {
                                    found = Some(v);
                                }
                            }
                            it.finish()?;
                            Ok(found)
                        }

                        #(#decode_fields)*
                        Ok(Self::#v_ident { #(#init_fields),* })
                    }
                });
            }
        }
    }

    Ok(quote! {
        let map = value.as_map().ok_or(::awrk_datex::WireError::Malformed("expected map"))?;
        if map.len() != 1 {
            return Err(::awrk_datex::WireError::Malformed("enum map must contain exactly one entry"));
        }
        let mut it = map.iter_pairs();
        let (variant_key, entry_value) = it
            .next()
            .ok_or(::awrk_datex::WireError::Malformed("missing enum entry"))??;
        if it.next().is_some() {
            return Err(::awrk_datex::WireError::Malformed("enum map must contain exactly one entry"));
        }
        it.finish()?;
        let Some(variant_key) = variant_key.as_u64() else {
            return Err(::awrk_datex::WireError::Malformed("enum variant key must be integer"));
        };
        let variant_index = u32::try_from(variant_key)
            .map_err(|_| ::awrk_datex::WireError::Malformed("enum variant key out of range"))?;
        match variant_index {
            #(#arms),*,
            _ => Err(::awrk_datex::WireError::Malformed("unknown enum variant")),
        }
    })
}

fn expand_enum_patch(_ident: &syn::Ident, _e: &DataEnum) -> syn::Result<proc_macro2::TokenStream> {
    Ok(quote! {
        *self = <Self as ::awrk_datex::Decode>::wire_decode(patch)?;
        Ok(())
    })
}

fn expand_enum_patch_validate(
    _ident: &syn::Ident,
    _e: &DataEnum,
) -> syn::Result<proc_macro2::TokenStream> {
    Ok(quote! {
        let _ = <Self as ::awrk_datex::Decode>::wire_decode(patch)?;
        Ok(())
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
