use super::*;

impl Model {
    pub fn expand(&self, runtime: &Path) -> TokenStream {
        let Self {
            attributes,
            conditions,
            name,
            storage,
            visibility,
            generics,
            ..
        } = self;
        let (implementation, arguments, clause) = generics.split_for_impl();
        let declarations = self.fields.iter().map(|field| {
            let Field {
                attributes,
                name,
                ty,
                visibility,
                ignored,
                ..
            } = field;
            let wrapper = if *ignored {
                quote!(#runtime::state::Unobserved)
            } else {
                quote!(#runtime::state::State)
            };
            quote!(#(#attributes)* #visibility #name: #wrapper<#ty>)
        });
        let parameters = self.fields.iter().map(|field| {
            let Field {
                conditions,
                name,
                ty,
                ..
            } = field;
            quote!(#(#conditions)* #name: #ty)
        });
        let initializers = self.fields.iter().map(|field| {
            let Field {
                conditions,
                name,
                ignored,
                ..
            } = field;
            let wrapper = if *ignored {
                quote!(#runtime::state::Unobserved)
            } else {
                quote!(#runtime::state::State)
            };
            quote!(#(#conditions)* #name: #wrapper::new(#name))
        });
        let storage_derives = [
            self.debug.then(|| quote!(::core::fmt::Debug)),
            self.default.then(|| quote!(::core::default::Default)),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        let storage_derives =
            (!storage_derives.is_empty()).then(|| quote!(#[derive(#(#storage_derives),*)]));
        let debug = self.debug.then(|| {
            let mut generics = generics.clone();
            generics.make_where_clause().predicates.push(syn::parse_quote!(#storage #arguments: ::core::fmt::Debug));
            let (implementation, _, clause) = generics.split_for_impl();
            quote! {
                #(#conditions)*
                impl #implementation ::core::fmt::Debug for #name #arguments #clause {
                    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        formatter.debug_tuple(stringify!(#name)).field(&self.0).finish()
                    }
                }
            }
        });
        let default = self.default.then(|| {
            let mut generics = generics.clone();
            generics.make_where_clause().predicates.push(syn::parse_quote!(#storage #arguments: ::core::default::Default));
            let (implementation, _, clause) = generics.split_for_impl();
            quote! {
                #(#conditions)*
                impl #implementation ::core::default::Default for #name #arguments #clause {
                    fn default() -> Self { Self(::std::rc::Rc::new(::core::default::Default::default())) }
                }
            }
        });
        let hash = self.hash.then(|| {
            quote! {
                #(#conditions)*
                impl #implementation ::core::hash::Hash for #name #arguments #clause {
                    fn hash<H: ::core::hash::Hasher>(&self, state: &mut H) {
                        ::core::hash::Hash::hash(&::std::rc::Rc::as_ptr(&self.0), state);
                    }
                }
            }
        });
        quote! {
            #(#attributes)*
            #visibility struct #name #generics (::std::rc::Rc<#storage #arguments>) #clause;

            #(#conditions)*
            #[doc(hidden)]
            #storage_derives
            #visibility struct #storage #generics #clause {
                #(#declarations,)*
            }

            #(#conditions)*
            impl #implementation #name #arguments #clause {
                pub fn new(#(#parameters),*) -> Self {
                    Self(::std::rc::Rc::new(#storage { #(#initializers,)* }))
                }
            }

            #(#conditions)*
            impl #implementation ::core::clone::Clone for #name #arguments #clause {
                fn clone(&self) -> Self { Self(self.0.clone()) }
            }

            #(#conditions)*
            impl #implementation ::core::cmp::PartialEq for #name #arguments #clause {
                fn eq(&self, other: &Self) -> bool { ::std::rc::Rc::ptr_eq(&self.0, &other.0) }
            }

            #(#conditions)*
            impl #implementation ::core::cmp::Eq for #name #arguments #clause {}

            #(#conditions)*
            impl #implementation ::core::ops::Deref for #name #arguments #clause {
                type Target = #storage #arguments;
                fn deref(&self) -> &Self::Target { &self.0 }
            }

            #debug
            #default
            #hash
        }
    }
}
