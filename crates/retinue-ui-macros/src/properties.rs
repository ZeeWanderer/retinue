use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Data, DeriveInput, Expr, Fields, GenericArgument, Meta, Path, PathArguments, Type,
    spanned::Spanned,
};

pub(super) fn expand(input: DeriveInput, runtime: &Path) -> syn::Result<TokenStream> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "ViewProperties requires a struct with named fields",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new(
            data.fields.span(),
            "ViewProperties requires named fields",
        ));
    };
    let mut generics = input.generics.clone();
    let mut values = Vec::new();
    for field in &fields.named {
        let name = field.ident.as_ref().unwrap();
        let ty = &field.ty;
        let mut source = None;
        for attr in &field.attrs {
            if attr.path().is_ident("owned") || attr.path().is_ident("environment") {
                if source.replace(attr).is_some() {
                    return Err(syn::Error::new(
                        attr.span(),
                        "a property has one source: owned or environment",
                    ));
                }
            }
        }
        let value = if let Some(attr) = source.filter(|attr| attr.path().is_ident("environment")) {
            if matches!(attr.meta, Meta::Path(_)) {
                if let Some(inner) = optional(ty) {
                    generics
                        .make_where_clause()
                        .predicates
                        .push(syn::parse_quote!(#inner: Clone + 'static));
                    quote!(context.environment().model::<#inner>())
                } else {
                    generics
                        .make_where_clause()
                        .predicates
                        .push(syn::parse_quote!(#ty: Clone + 'static));
                    quote!(context.environment().model::<#ty>().expect(concat!("missing environment model for ", stringify!(#name))))
                }
            } else {
                let key: Path = attr.parse_args()?;
                quote!(context.environment().get::<#key>())
            }
        } else {
            generics
                .make_where_clause()
                .predicates
                .push(syn::parse_quote!(#ty: Clone + 'static));
            let initial = match source {
                Some(attr) if !matches!(attr.meta, Meta::Path(_)) => {
                    let expression: Expr = attr.parse_args()?;
                    quote!(#expression)
                }
                _ => {
                    generics
                        .make_where_clause()
                        .predicates
                        .push(syn::parse_quote!(#ty: Default));
                    quote!(<#ty as Default>::default())
                }
            };
            quote!(context.owned_property::<Self, #ty>(stringify!(#name), || #initial))
        };
        values.push(quote!(#name: #value));
    }
    generics
        .make_where_clause()
        .predicates
        .push(syn::parse_quote!(Self: 'static));
    let name = &input.ident;
    let (implementation, arguments, bounds) = generics.split_for_impl();
    Ok(quote! {
        impl #implementation #runtime::view::ViewProperties for #name #arguments #bounds {
            fn resolve(context: &mut #runtime::view::ViewContext<'_>) -> Self {
                Self { #(#values),* }
            }
        }
    })
}

fn optional(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let part = path.path.segments.last()?;
    if part.ident != "Option" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &part.arguments else {
        return None;
    };
    match args.args.first()? {
        GenericArgument::Type(ty) if args.args.len() == 1 => Some(ty),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_ambiguous_storage_and_invalid_initializers() {
        for input in [
            quote!(
                enum Invalid {
                    A,
                }
            ),
            quote!(
                struct Invalid(u32);
            ),
            quote!(
                struct Invalid {
                    #[owned]
                    #[environment]
                    value: u32,
                }
            ),
            quote!(
                struct Invalid {
                    #[owned(1, 2)]
                    value: u32,
                }
            ),
        ] {
            assert!(expand(syn::parse2(input).unwrap(), &syn::parse_quote!(retinue_ui)).is_err());
        }
    }
}
