mod model;
mod properties;
mod ui;

use proc_macro::TokenStream;
use proc_macro2::Span;
use syn::{Path, parse::Parser};

#[proc_macro_derive(ViewProperties, attributes(owned, environment))]
pub fn view_properties(input: TokenStream) -> TokenStream {
    runtime_crate()
        .and_then(|runtime| properties::expand(syn::parse(input)?, &runtime))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Declare a shared model with observed fields and explicit value access.
#[proc_macro_attribute]
pub fn observable(arguments: TokenStream, item: TokenStream) -> TokenStream {
    expand(arguments.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Nested child blocks over the ordinary view constructors.
#[proc_macro]
pub fn ui(input: TokenStream) -> TokenStream {
    ui::expand(input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand(
    arguments: proc_macro2::TokenStream,
    item: proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut runtime = None;
    syn::meta::parser(|meta| {
        if !meta.path.is_ident("crate") {
            return Err(meta.error("expected `crate = path`"));
        }
        if runtime.is_some() {
            return Err(meta.error("duplicate runtime crate option"));
        }
        runtime = Some(meta.value()?.parse::<Path>()?);
        Ok(())
    })
    .parse2(arguments)?;
    let runtime = match runtime {
        Some(path) => path,
        None => runtime_crate()?,
    };
    let model = model::Model::parse(syn::parse2(item)?)?;
    Ok(model.expand(&runtime))
}

fn runtime_crate() -> syn::Result<Path> {
    Ok(match proc_macro_crate::crate_name("retinue-ui") {
        Ok(proc_macro_crate::FoundCrate::Itself) => syn::parse_quote!(::retinue_ui),
        Ok(proc_macro_crate::FoundCrate::Name(name)) => {
            let name = syn::Ident::new(&name, Span::call_site());
            syn::parse_quote!(::#name)
        }
        Err(error) => {
            return Err(syn::Error::new(
                Span::call_site(),
                format!(
                    "observable requires retinue-ui: {error}; alternatively specify `crate = path`"
                ),
            ));
        }
    })
}
