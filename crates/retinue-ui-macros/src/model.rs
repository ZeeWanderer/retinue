use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Attribute, Fields, GenericParam, Generics, Ident, ItemStruct, Path, Type, Visibility,
    ext::IdentExt, punctuated::Punctuated, spanned::Spanned,
};

mod emit;
#[cfg(test)]
mod tests;

pub(super) struct Model {
    attributes: Vec<Attribute>,
    conditions: Vec<Attribute>,
    name: Ident,
    storage: Ident,
    visibility: Visibility,
    generics: Generics,
    fields: Vec<Field>,
    debug: bool,
    default: bool,
    hash: bool,
}

struct Field {
    attributes: Vec<Attribute>,
    conditions: Vec<Attribute>,
    name: Ident,
    ty: Type,
    visibility: Visibility,
    ignored: bool,
}

impl Model {
    pub fn parse(item: ItemStruct) -> syn::Result<Self> {
        let Fields::Named(fields) = item.fields else {
            return Err(syn::Error::new(
                item.fields.span(),
                "observable requires a struct with named fields",
            ));
        };
        let mut model = Self {
            storage: format_ident!("__{}Fields", item.ident.unraw()),
            name: item.ident,
            visibility: item.vis,
            generics: item.generics,
            attributes: Vec::new(),
            conditions: Vec::new(),
            fields: Vec::new(),
            debug: false,
            default: false,
            hash: false,
        };
        for attribute in item.attrs {
            if attribute.path().is_ident("derive") {
                let derives = attribute
                    .parse_args_with(Punctuated::<Path, syn::Token![,]>::parse_terminated)?;
                let mut other = Vec::new();
                for derive in derives {
                    match builtin_derive(&derive).as_deref() {
                        Some("Clone" | "PartialEq" | "Eq") => {}
                        Some("Debug") => model.debug = true,
                        Some("Default") => model.default = true,
                        Some("Hash") => model.hash = true,
                        Some("Copy") => {
                            return Err(syn::Error::new(
                                derive.span(),
                                "observable models share mutable state; use Clone, not Copy",
                            ));
                        }
                        Some("PartialOrd" | "Ord") => {
                            return Err(syn::Error::new(
                                derive.span(),
                                "observable models compare identity; order field values or snapshots explicitly",
                            ));
                        }
                        _ => other.push(derive),
                    }
                }
                if !other.is_empty() {
                    model
                        .attributes
                        .push(syn::parse_quote!(#[derive(#(#other),*)]));
                }
            } else {
                if attribute.path().is_ident("cfg") {
                    model.conditions.push(attribute.clone());
                }
                model.attributes.push(attribute);
            }
        }
        for mut field in fields.named {
            let mut ignored = false;
            let mut seen = false;
            let mut attributes = Vec::new();
            let mut conditions = Vec::new();
            for attribute in field.attrs {
                if attribute.path().is_ident("observation") {
                    attribute.parse_nested_meta(|meta| {
                        if !meta.path.is_ident("ignore") {
                            return Err(meta.error("expected `ignore`"));
                        }
                        if seen {
                            return Err(meta.error("duplicate observation option"));
                        }
                        if meta.input.peek(syn::Token![=]) || meta.input.peek(syn::token::Paren) {
                            return Err(meta.error("ignore does not take a value"));
                        }
                        seen = true;
                        ignored = true;
                        Ok(())
                    })?;
                    if !seen {
                        return Err(syn::Error::new(
                            attribute.span(),
                            "expected `observation(ignore)`",
                        ));
                    }
                } else {
                    if attribute.path().is_ident("cfg") || attribute.path().is_ident("cfg_attr") {
                        conditions.push(attribute.clone());
                    }
                    attributes.push(attribute);
                }
            }
            substitute_self(&mut field.ty, &model.name, &model.generics);
            model.fields.push(Field {
                attributes,
                conditions,
                name: field.ident.unwrap(),
                ty: field.ty,
                visibility: field.vis,
                ignored,
            });
        }
        for parameter in &mut model.generics.params {
            match parameter {
                GenericParam::Type(parameter) => parameter.bounds.push(syn::parse_quote!('static)),
                GenericParam::Lifetime(parameter) => {
                    parameter.bounds.push(syn::parse_quote!('static))
                }
                GenericParam::Const(_) => {}
            }
        }
        Ok(model)
    }
}

fn builtin_derive(path: &Path) -> Option<String> {
    let names: Vec<_> = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    match names.as_slice() {
        [name] => Some(name.clone()),
        [root, module, name]
            if (root == "std" || root == "core")
                && matches!(
                    (module.as_str(), name.as_str()),
                    ("clone", "Clone")
                        | ("cmp", "PartialEq" | "Eq" | "PartialOrd" | "Ord")
                        | ("fmt", "Debug")
                        | ("default", "Default")
                        | ("hash", "Hash")
                        | ("marker", "Copy")
                ) =>
        {
            Some(name.clone())
        }
        _ => None,
    }
}

fn substitute_self(ty: &mut Type, name: &Ident, generics: &Generics) {
    use syn::visit_mut::VisitMut;
    struct Substitute(Type);
    impl VisitMut for Substitute {
        fn visit_type_mut(&mut self, ty: &mut Type) {
            if matches!(ty, Type::Path(path) if path.qself.is_none() && path.path.is_ident("Self"))
            {
                *ty = self.0.clone();
            } else {
                syn::visit_mut::visit_type_mut(self, ty);
            }
        }
    }
    let (_, arguments, _) = generics.split_for_impl();
    Substitute(syn::parse_quote!(#name #arguments)).visit_type_mut(ty);
}
