//! Derive macros for the [`zvec`] crate.
//!
//! Currently only [`IntoDoc`] is provided. See
//! `zvec::IntoDoc` for the trait and the generated code's contract.
//!
//! [`zvec`]: https://docs.rs/zvec
//! [`IntoDoc`]: macro@IntoDoc

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{parse_macro_input, Data, DataStruct, DeriveInput, Field, Fields, LitStr, Type};

/// Derive an `IntoDoc` impl that constructs a `zvec::Doc` from `&self`.
///
/// # Field attributes
///
/// Each field accepts at most one `#[zvec(...)]` attribute. Recognised
/// keys:
///
/// | key                      | effect                                                           |
/// |--------------------------|------------------------------------------------------------------|
/// | `pk`                     | Use this field as the document's primary key (must be a `String`). |
/// | `rename = "other"`       | Use `"other"` as the field name in zvec instead of the Rust ident. |
/// | `skip`                   | Don't emit this field at all.                                    |
/// | `binary`                 | Treat `Vec<u8>` as `DataType::Binary`.                           |
/// | `vector_fp32`            | Treat `Vec<f32>` as `DataType::VectorFp32`.                      |
/// | `vector_fp64`            | Treat `Vec<f64>` as `DataType::VectorFp64`.                      |
/// | `vector_int8`            | Treat `Vec<i8>`  as `DataType::VectorInt8`.                      |
/// | `vector_int16`           | Treat `Vec<i16>` as `DataType::VectorInt16`.                     |
///
/// # Supported field types (without explicit type hint)
///
/// `String`, `bool`, `i32`, `i64`, `u32`, `u64`, `f32`, `f64`. Each of
/// these may be wrapped in `Option<T>` — `None` emits
/// `Doc::set_field_null(name)`.
///
/// `Vec<_>`-typed fields **require** an explicit type hint (see table
/// above), because the same Rust type can map to several different
/// zvec `DataType`s.
#[proc_macro_derive(IntoDoc, attributes(zvec))]
pub fn derive_into_doc(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand(input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let fields = match &input.data {
        Data::Struct(DataStruct {
            fields: Fields::Named(f),
            ..
        }) => &f.named,
        _ => {
            return Err(syn::Error::new_spanned(
                &input,
                "IntoDoc can only be derived for structs with named fields",
            ));
        }
    };

    let mut body = TokenStream2::new();
    let mut pk_seen = false;

    for field in fields {
        let attrs = FieldAttrs::from(field)?;
        if attrs.skip {
            continue;
        }
        let rust_ident = field.ident.as_ref().unwrap();
        let zvec_name = attrs.rename.unwrap_or_else(|| rust_ident.to_string());

        if attrs.pk {
            if pk_seen {
                return Err(syn::Error::new_spanned(
                    field,
                    "duplicate #[zvec(pk)] — only one field may be the primary key",
                ));
            }
            pk_seen = true;
            body.extend(quote_spanned! { field.span() =>
                __doc.set_pk(&self.#rust_ident)?;
            });
        }

        let setter = emit_setter(field, &attrs.kind, &zvec_name)?;
        body.extend(setter);
    }

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    Ok(quote! {
        impl #impl_generics ::zvec::IntoDoc for #name #ty_generics #where_clause {
            fn into_doc(&self) -> ::zvec::Result<::zvec::Doc> {
                let mut __doc = ::zvec::Doc::new()?;
                #body
                Ok(__doc)
            }
        }
    })
}

#[derive(Default)]
struct FieldAttrs {
    pk: bool,
    skip: bool,
    rename: Option<String>,
    kind: TypeHint,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum TypeHint {
    #[default]
    Auto,
    Binary,
    VectorFp32,
    VectorFp64,
    VectorInt8,
    VectorInt16,
}

impl FieldAttrs {
    fn from(field: &Field) -> syn::Result<Self> {
        let mut out = FieldAttrs::default();
        for attr in &field.attrs {
            if !attr.path().is_ident("zvec") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                let p = &meta.path;
                if p.is_ident("pk") {
                    out.pk = true;
                } else if p.is_ident("skip") {
                    out.skip = true;
                } else if p.is_ident("rename") {
                    let lit: LitStr = meta.value()?.parse()?;
                    out.rename = Some(lit.value());
                } else if p.is_ident("binary") {
                    out.kind = TypeHint::Binary;
                } else if p.is_ident("vector_fp32") {
                    out.kind = TypeHint::VectorFp32;
                } else if p.is_ident("vector_fp64") {
                    out.kind = TypeHint::VectorFp64;
                } else if p.is_ident("vector_int8") {
                    out.kind = TypeHint::VectorInt8;
                } else if p.is_ident("vector_int16") {
                    out.kind = TypeHint::VectorInt16;
                } else {
                    return Err(meta.error(
                        "unknown zvec attribute; expected one of: \
                         pk, skip, rename, binary, vector_fp32, vector_fp64, \
                         vector_int8, vector_int16",
                    ));
                }
                Ok(())
            })?;
        }
        Ok(out)
    }
}

fn emit_setter(field: &Field, hint: &TypeHint, name: &str) -> syn::Result<TokenStream2> {
    let ident = field.ident.as_ref().unwrap();
    let ty = &field.ty;
    let name_lit = LitStr::new(name, field.span());

    // Option<T>: emit a `match` that writes null for None or recurses
    // on Some(inner).
    if let Some(inner) = option_inner(ty) {
        let inner_ty = inner.clone();
        let inner_call =
            scalar_or_hinted_setter(&inner_ty, hint, &name_lit, quote!(__inner), field.span())?;
        return Ok(quote_spanned! { field.span() =>
            match &self.#ident {
                ::core::option::Option::Some(__inner) => { #inner_call },
                ::core::option::Option::None => { __doc.set_field_null(#name_lit)?; },
            }
        });
    }

    // Not Option: emit a single setter call on `&self.#ident`.
    let access = quote_spanned! { field.span() => &self.#ident };
    scalar_or_hinted_setter(ty, hint, &name_lit, access, field.span())
}

fn scalar_or_hinted_setter(
    ty: &Type,
    hint: &TypeHint,
    name: &LitStr,
    access: TokenStream2,
    span: proc_macro2::Span,
) -> syn::Result<TokenStream2> {
    match hint {
        TypeHint::Binary => {
            return Ok(quote_spanned! { span =>
                __doc.add_binary(#name, #access)?;
            });
        }
        TypeHint::VectorFp32 => {
            return Ok(quote_spanned! { span =>
                __doc.add_vector_fp32(#name, #access)?;
            });
        }
        TypeHint::VectorFp64 => {
            return Ok(quote_spanned! { span =>
                __doc.add_vector_fp64(#name, #access)?;
            });
        }
        TypeHint::VectorInt8 => {
            return Ok(quote_spanned! { span =>
                __doc.add_vector_int8(#name, #access)?;
            });
        }
        TypeHint::VectorInt16 => {
            return Ok(quote_spanned! { span =>
                __doc.add_vector_int16(#name, #access)?;
            });
        }
        TypeHint::Auto => {}
    }

    // Auto path: match on the last path segment's ident.
    let last_segment = match ty {
        Type::Path(p) => p.path.segments.last(),
        _ => None,
    };
    let Some(last) = last_segment else {
        return Err(syn::Error::new(
            span,
            "unsupported field type for IntoDoc; add a #[zvec(...)] type hint \
             (e.g. #[zvec(vector_fp32)] for Vec<f32>)",
        ));
    };
    let name_s = last.ident.to_string();
    let setter = match name_s.as_str() {
        "String" => quote!(add_string),
        "bool" => {
            // add_bool takes bool by value.
            return Ok(quote_spanned! { span =>
                __doc.add_bool(#name, *#access)?;
            });
        }
        "i32" => {
            return Ok(quote_spanned! { span =>
                __doc.add_int32(#name, *#access)?;
            });
        }
        "i64" => {
            return Ok(quote_spanned! { span =>
                __doc.add_int64(#name, *#access)?;
            });
        }
        "u32" => {
            return Ok(quote_spanned! { span =>
                __doc.add_uint32(#name, *#access)?;
            });
        }
        "u64" => {
            return Ok(quote_spanned! { span =>
                __doc.add_uint64(#name, *#access)?;
            });
        }
        "f32" => {
            return Ok(quote_spanned! { span =>
                __doc.add_float(#name, *#access)?;
            });
        }
        "f64" => {
            return Ok(quote_spanned! { span =>
                __doc.add_double(#name, *#access)?;
            });
        }
        _ => {
            return Err(syn::Error::new(
                span,
                format!(
                    "unsupported field type `{name_s}` for IntoDoc; \
                     add a #[zvec(...)] type hint or extend the derive \
                     to cover this type",
                ),
            ));
        }
    };
    Ok(quote_spanned! { span =>
        __doc.#setter(#name, #access)?;
    })
}

fn option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(p) = ty else { return None };
    let seg = p.path.segments.last()?;
    if seg.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    let syn::GenericArgument::Type(inner) = args.args.first()? else {
        return None;
    };
    Some(inner)
}
