use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    Expr, Ident, Stmt, Token, braced, parenthesized,
    parse::{Parse, ParseStream, Parser},
};

pub fn view(arguments: TokenStream, input: TokenStream) -> syn::Result<TokenStream> {
    if !arguments.is_empty() {
        return Err(syn::Error::new_spanned(
            arguments,
            "view takes no arguments",
        ));
    }
    let mut implementation: syn::ItemImpl = syn::parse2(input)?;
    let captures: Vec<_> = implementation
        .generics
        .params
        .iter()
        .map(|parameter| match parameter {
            syn::GenericParam::Type(ty) => {
                let name = &ty.ident;
                quote!(#name)
            }
            syn::GenericParam::Const(value) => {
                let name = &value.ident;
                quote!(#name)
            }
            syn::GenericParam::Lifetime(value) => {
                let name = &value.lifetime;
                quote!(#name)
            }
        })
        .collect();
    let method = implementation
        .items
        .iter_mut()
        .find_map(|item| match item {
            syn::ImplItem::Fn(method) if method.sig.ident == "body" => Some(method),
            _ => None,
        })
        .ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "view requires a body method",
            )
        })?;
    if method.sig.receiver().is_none() || !method.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &method.sig,
            "body requires a non-generic view method",
        ));
    }
    let syn::ReturnType::Type(_, ty) = &mut method.sig.output else {
        return Err(syn::Error::new_spanned(
            &method.sig,
            "body must return impl Body",
        ));
    };
    let syn::Type::ImplTrait(output) = ty.as_mut() else {
        return Err(syn::Error::new_spanned(ty, "body must return impl Body"));
    };
    if !output
        .bounds
        .iter()
        .any(|bound| matches!(bound, syn::TypeParamBound::PreciseCapture(_)))
    {
        output.bounds.push(syn::parse_quote!(use<#(#captures),*>));
    }
    Ok(quote!(#implementation))
}

pub fn expand(input: TokenStream) -> syn::Result<TokenStream> {
    (|input: ParseStream| {
        let value = node(input)?;
        if input.peek(Token![;]) {
            input.parse::<Token![;]>()?;
        }
        if !input.is_empty() {
            return Err(input.error("expected one root view"));
        }
        Ok(value)
    })
    .parse2(input)
}

fn node(input: ParseStream) -> syn::Result<TokenStream> {
    if input.peek(Token![if]) {
        return conditional(input);
    }
    if input.peek(Token![match]) {
        return selection(input);
    }
    if input.peek(Token![for]) {
        return Err(input
            .error("use ForEach(items, key) { |item| ... } so item identity survives reordering"));
    }
    let expression: Expr = input.parse()?;
    if !input.peek(syn::token::Brace) {
        return Ok(quote!(#expression));
    }
    let mut value = match expression {
        Expr::Call(call) => {
            let function = call.func;
            let mut arguments = call.args;
            let children = children(input)?;
            arguments.push(syn::parse2(children)?);
            quote!(#function(#arguments))
        }
        Expr::Field(field) => {
            let base = field.base;
            let method = field.member;
            let child = single(input)?;
            quote!(#base.#method(#child))
        }
        Expr::MethodCall(call) => {
            let base = call.receiver;
            let method = call.method;
            let generic = call.turbofish;
            let mut arguments = call.args;
            arguments.push(syn::parse2(single(input)?)?);
            quote!(#base.#method #generic (#arguments))
        }
        expression => {
            return Err(syn::Error::new_spanned(
                expression,
                "child block requires a constructor call",
            ));
        }
    };
    while input.peek(Token![.]) {
        input.parse::<Token![.]>()?;
        let method: Ident = input.parse()?;
        let generic: Option<syn::AngleBracketedGenericArguments> = if input.peek(Token![::]) {
            Some(input.parse()?)
        } else {
            None
        };
        let mut args = syn::punctuated::Punctuated::<Expr, Token![,]>::new();
        if input.peek(syn::token::Paren) {
            let content;
            parenthesized!(content in input);
            args = content.parse_terminated(Expr::parse, Token![,])?;
        }
        if input.peek(syn::token::Brace) {
            args.push(syn::parse2(single(input)?)?);
        }
        value = quote!(#value.#method #generic (#args));
    }
    Ok(value)
}

fn single(input: ParseStream) -> syn::Result<TokenStream> {
    let content;
    braced!(content in input);
    let value = node(&content)?;
    if content.peek(Token![;]) {
        content.parse::<Token![;]>()?;
    }
    if !content.is_empty() {
        return Err(content.error("expected one modifier view"));
    }
    Ok(value)
}

fn children(input: ParseStream) -> syn::Result<TokenStream> {
    let content;
    braced!(content in input);
    if content.peek(Token![|]) || content.peek(Token![move]) {
        let capture: Option<Token![move]> = content.parse()?;
        content.parse::<Token![|]>()?;
        let mut parameters = syn::punctuated::Punctuated::<syn::Pat, Token![,]>::new();
        while !content.peek(Token![|]) {
            parameters.push_value(syn::Pat::parse_single(&content)?);
            if content.peek(Token![|]) {
                break;
            }
            parameters.push_punct(content.parse()?);
        }
        content.parse::<Token![|]>()?;
        let body = if content.peek(syn::token::Brace) {
            body_block(&content)?
        } else {
            body_contents(&content)?
        };
        if !content.is_empty() {
            return Err(content.error("expected one collection item body"));
        }
        return Ok(quote!(#capture |#parameters| { #body }));
    }
    let (locals, children) = child_list(&content)?;
    let children = child_tuple(&children);
    Ok(quote!({ #(#locals)* #children }))
}

fn child_tuple(children: &[Ident]) -> TokenStream {
    let mut groups: Vec<_> = children.iter().map(|child| quote!(#child)).collect();
    while groups.len() > 8 {
        groups = groups
            .chunks(8)
            .map(|group| quote!((#(#group,)*)))
            .collect();
    }
    quote!((#(#groups,)*))
}

fn child_list(content: ParseStream) -> syn::Result<(Vec<Stmt>, Vec<Ident>)> {
    let mut locals = Vec::<Stmt>::new();
    let mut children = Vec::new();
    while !content.is_empty() {
        if content.peek(Token![let]) {
            locals.push(content.parse()?);
        } else {
            let child = node(content)?;
            let name = Ident::new(
                &format!("__retinue_child_{}", children.len()),
                Span::mixed_site(),
            );
            locals.push(syn::parse_quote!(let #name = #child;));
            children.push(name);
            if content.peek(Token![;]) {
                content.parse::<Token![;]>()?;
            }
        }
    }
    Ok((locals, children))
}

fn body_block(input: ParseStream) -> syn::Result<TokenStream> {
    let content;
    braced!(content in input);
    body_contents(&content)
}

fn body_contents(content: ParseStream) -> syn::Result<TokenStream> {
    let (locals, children) = child_list(content)?;
    if let [child] = children.as_slice() {
        return Ok(quote!({ #(#locals)* #child }));
    }
    let runtime = crate::runtime_crate()?;
    let children = child_tuple(&children);
    Ok(quote!({ #(#locals)* #runtime::authoring::Group(#children) }))
}

fn conditional(input: ParseStream) -> syn::Result<TokenStream> {
    input.parse::<Token![if]>()?;
    let condition = Expr::parse_without_eager_brace(input)?;
    let yes = body_block(input)?;
    if !input.peek(Token![else]) {
        return Ok(
            quote!(if #condition { ::core::option::Option::Some(#yes) } else { ::core::option::Option::None }),
        );
    }
    input.parse::<Token![else]>()?;
    let no = if input.peek(Token![if]) {
        conditional(input)?
    } else {
        body_block(input)?
    };
    let runtime = crate::runtime_crate()?;
    Ok(
        quote!(if #condition { #runtime::authoring::Conditional::Then(#yes) } else { #runtime::authoring::Conditional::Else(#no) }),
    )
}

fn selection(input: ParseStream) -> syn::Result<TokenStream> {
    input.parse::<Token![match]>()?;
    let value = Expr::parse_without_eager_brace(input)?;
    let content;
    braced!(content in input);
    let mut arms = Vec::new();
    while !content.is_empty() {
        let pattern = syn::Pat::parse_multi_with_leading_vert(&content)?;
        let guard = if content.peek(Token![if]) {
            content.parse::<Token![if]>()?;
            let condition: Expr = content.parse()?;
            quote!(if #condition)
        } else {
            TokenStream::new()
        };
        content.parse::<Token![=>]>()?;
        let block = content.peek(syn::token::Brace);
        let body = if block {
            body_block(&content)?
        } else {
            node(&content)?
        };
        arms.push((pattern, guard, body));
        if content.peek(Token![,]) || !block && !content.is_empty() {
            content.parse::<Token![,]>()?;
        }
    }
    if arms.is_empty() {
        return Err(content.error("a view match requires at least one arm"));
    }
    let runtime = crate::runtime_crate()?;
    let last = arms.len() - 1;
    let arms = arms
        .into_iter()
        .enumerate()
        .map(|(index, (pattern, guard, mut body))| {
            if index < last {
                body = quote!(#runtime::authoring::Conditional::Then(#body));
            }
            for _ in 0..index {
                body = quote!(#runtime::authoring::Conditional::Else(#body));
            }
            quote!(#pattern #guard => { #body })
        });
    Ok(quote!(match #value { #(#arms,)* }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_remain_rust_calls_and_children_are_typed_tuples() {
        let result =
            expand(quote!(Stack(4.0) { label("left") label("right") }.background { fill() }))
                .unwrap();
        let expression: Expr = syn::parse2(result.clone()).unwrap();
        assert!(matches!(expression, Expr::MethodCall(_)));
        let text = result.to_string();
        assert!(!text.contains("Vec") && !text.contains("Element") && !text.contains(":: new"));
        assert!(text.contains("label (\"left\")") && text.contains("label (\"right\")"));
    }

    #[test]
    fn trailing_arguments_empty_containers_and_leaf_modifiers_parse() {
        for input in [
            quote!(Stack(4.0,) { label() }),
            quote!(Stack() {}),
            quote!(label().background { fill() }),
            quote!(Stack(4.0) { label() let text = "later"; label(text) }),
        ] {
            syn::parse2::<Expr>(expand(input).unwrap()).unwrap();
        }
    }

    #[test]
    fn body_precise_capture_does_not_borrow_its_view_or_context() {
        let result = view(
            quote!(),
            quote!(
                impl<T> View for Label<T> {
                    fn body(&self, cx: &mut Context<'_>) -> impl Body {
                        self.label(cx)
                    }
                }
            ),
        )
        .unwrap();
        assert!(result.to_string().contains("use < T >"));
        assert!(view(quote!(ignored), quote!()).is_err());
    }

    #[test]
    fn branches_and_keyed_closures_lower_to_typed_rust() {
        for input in [
            quote!(if visible { Text("Yes") } else { Text("No") }),
            quote!(Stack(0.0) { if let Some(value) = item { Text(value) } }),
            quote!(ForEach(items, |item| item.id) { |item| HStack(2.0) { Text(item.name) Text("Tail") } }),
            quote!(ForEach(items, |item| item.id) { move |item| { let name = item.name; Text(name) } }),
            quote!(ForEach(items, |item| item.id) { |item| Text(item.name) Text("Detail") }),
            quote!(ForEach(items, |item| item.id) { |item| let name = item.name; let show = !name.is_empty(); if show { Text(name) } }),
            quote!(match mode { 0 | 1 if ready => Text("Ready"), 2 => { Text("A") Text("B") }, _ => Text("Other"), }),
            quote!(match mode {
                0 => {
                    Text("A")
                }
                1 => {
                    Text("B")
                }
                _ => {
                    Text("Other")
                }
            }),
        ] {
            syn::parse2::<Expr>(expand(input).unwrap()).unwrap();
        }
        assert!(
            expand(quote!(for item in items {
                Text(item)
            }))
            .unwrap_err()
            .to_string()
            .contains("ForEach")
        );
    }
}
