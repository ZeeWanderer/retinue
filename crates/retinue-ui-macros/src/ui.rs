use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::{
    Expr, Ident, Path, Stmt, Token, braced, parenthesized,
    parse::{Parse, ParseStream, Parser},
    spanned::Spanned,
};

pub fn expand(input: TokenStream) -> syn::Result<TokenStream> {
    let runtime = super::runtime_crate()?;
    (|input: ParseStream| {
        let value = node(input, &runtime)?;
        if input.peek(Token![;]) {
            input.parse::<Token![;]>()?;
        }
        if !input.is_empty() {
            return Err(input.error("expected one root view; put siblings in a stack"));
        }
        Ok(quote!(#runtime::view::IntoElement::into_element(#value)))
    })
    .parse2(input)
}

fn constructor(path: &Path) -> TokenStream {
    if path
        .segments
        .last()
        .is_some_and(|segment| segment.ident.to_string().starts_with(char::is_uppercase))
    {
        quote!(#path::new)
    } else {
        quote!(#path)
    }
}

fn leaf(mut expression: Expr) -> Expr {
    match &mut expression {
        Expr::Call(call) => {
            if let Expr::Path(path) = &*call.func {
                let constructor = constructor(&path.path);
                call.func = Box::new(syn::parse_quote!(#constructor));
            }
        }
        Expr::MethodCall(call) => call.receiver = Box::new(leaf(*call.receiver.clone())),
        _ => {}
    }
    expression
}

fn node(input: ParseStream, runtime: &Path) -> syn::Result<TokenStream> {
    if input.peek(Token![if]) {
        input.parse::<Token![if]>()?;
        let condition = input.call(Expr::parse_without_eager_brace)?;
        let yes = single(input, runtime)?;
        let no = if input.peek(Token![else]) {
            input.parse::<Token![else]>()?;
            if input.peek(Token![if]) {
                node(input, runtime)?
            } else {
                single(input, runtime)?
            }
        } else {
            quote!(#runtime::view::empty())
        };
        return Ok(quote!(if #condition {
            #runtime::view::IntoElement::into_element(#yes)
        } else { #runtime::view::IntoElement::into_element(#no) }));
    }
    let expression: Expr = input.parse()?;
    if !input.peek(syn::token::Brace) {
        let expression = leaf(expression);
        return Ok(quote!(#expression));
    }
    let mut output = match expression {
        Expr::Call(call) => {
            let Expr::Path(path) = *call.func else {
                return Err(syn::Error::new(
                    call.func.span(),
                    "use a named constructor for a child block",
                ));
            };
            let args = &call.args;
            let children = block(input, runtime)?;
            let constructor = constructor(&path.path);
            if args.is_empty() {
                quote!(#constructor(#children))
            } else {
                quote!(#constructor(#args, #children))
            }
        }
        Expr::Field(field) => {
            let base = leaf(*field.base);
            let method = field.member;
            let child = single(input, runtime)?;
            quote!(#base.#method(#child))
        }
        Expr::MethodCall(call) => {
            let base = leaf(*call.receiver);
            let method = call.method;
            let generic = call.turbofish;
            let mut arguments = call.args;
            arguments.push(syn::parse2(single(input, runtime)?)?);
            quote!(#base.#method #generic (#arguments))
        }
        other => {
            return Err(syn::Error::new(
                other.span(),
                "a child block follows a constructor or modifier",
            ));
        }
    };
    while input.peek(Token![.]) {
        input.parse::<Token![.]>()?;
        let method: Ident = input.parse()?;
        let generic = if input.peek(Token![::]) {
            Some(input.parse::<syn::AngleBracketedGenericArguments>()?)
        } else {
            None
        };
        let mut arguments = if input.peek(syn::token::Paren) {
            let arguments;
            parenthesized!(arguments in input);
            arguments.parse_terminated(Expr::parse, Token![,])?
        } else {
            syn::punctuated::Punctuated::new()
        };
        if input.peek(syn::token::Brace) {
            arguments.push(syn::parse2(single(input, runtime)?)?);
        }
        output = quote!(#output.#method #generic (#arguments));
    }
    Ok(output)
}

fn single(input: ParseStream, runtime: &Path) -> syn::Result<TokenStream> {
    let content;
    braced!(content in input);
    let child = node(&content, runtime)?;
    if content.peek(Token![;]) {
        content.parse::<Token![;]>()?;
    }
    if !content.is_empty() {
        return Err(content.error("expected one view in this modifier; use a stack for siblings"));
    }
    Ok(child)
}

fn block(input: ParseStream, runtime: &Path) -> syn::Result<TokenStream> {
    let content;
    braced!(content in input);
    let output = Ident::new("__retinue_children", Span::mixed_site());
    let statements = statements(&content, runtime, &output)?;
    Ok(quote!({
        let mut #output: ::std::vec::Vec<#runtime::view::Element> = ::std::vec::Vec::new();
        #statements
        #output
    }))
}

fn statements(input: ParseStream, runtime: &Path, output: &Ident) -> syn::Result<TokenStream> {
    let mut tokens = TokenStream::new();
    while !input.is_empty() {
        if input.peek(Token![let]) {
            let statement: Stmt = input.parse()?;
            tokens.extend(quote!(#statement));
        } else if input.peek(Token![if]) {
            let children = Ident::new("__retinue_branch", Span::mixed_site());
            let body = conditional(input, runtime, &children)?;
            tokens.extend(fragment(runtime, output, &children, body));
        } else if input.peek(Token![for]) {
            input.parse::<Token![for]>()?;
            let pattern = input.call(syn::Pat::parse_multi_with_leading_vert)?;
            input.parse::<Token![in]>()?;
            let iterable = input.call(Expr::parse_without_eager_brace)?;
            let content;
            braced!(content in input);
            let children = Ident::new("__retinue_items", Span::mixed_site());
            let body = statements(&content, runtime, &children)?;
            tokens.extend(fragment(
                runtime,
                output,
                &children,
                quote!(for #pattern in #iterable { #body }),
            ));
        } else if input.peek(Token![match]) {
            input.parse::<Token![match]>()?;
            let value = input.call(Expr::parse_without_eager_brace)?;
            let arms;
            braced!(arms in input);
            let mut branches = TokenStream::new();
            let children = Ident::new("__retinue_match", Span::mixed_site());
            while !arms.is_empty() {
                let pattern = arms.call(syn::Pat::parse_multi_with_leading_vert)?;
                let guard = if arms.peek(Token![if]) {
                    arms.parse::<Token![if]>()?;
                    let guard: Expr = arms.parse()?;
                    Some(quote!(if #guard))
                } else {
                    None
                };
                arms.parse::<Token![=>]>()?;
                let body = if arms.peek(syn::token::Brace) {
                    let content;
                    braced!(content in arms);
                    statements(&content, runtime, &children)?
                } else {
                    let view = node(&arms, runtime)?;
                    quote!(#children.push(#runtime::view::IntoElement::into_element(#view));)
                };
                branches.extend(quote!(#pattern #guard => { #body },));
                if arms.peek(Token![,]) {
                    arms.parse::<Token![,]>()?;
                }
            }
            tokens.extend(fragment(
                runtime,
                output,
                &children,
                quote!(match #value { #branches }),
            ));
        } else {
            let value = node(input, runtime)?;
            let expression = leaf(syn::parse2(value)?);
            tokens.extend(quote_spanned!(expression.span()=> #output.push(#runtime::view::IntoElement::into_element(#expression));));
        }
        if input.peek(Token![;]) {
            input.parse::<Token![;]>()?;
        } else if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
    }
    Ok(tokens)
}

fn fragment(runtime: &Path, output: &Ident, children: &Ident, body: TokenStream) -> TokenStream {
    quote! {
        #output.push({
            let mut #children: ::std::vec::Vec<#runtime::view::Element> = ::std::vec::Vec::new();
            #body
            #runtime::view::Element::fragment(#children)
        });
    }
}

fn conditional(input: ParseStream, runtime: &Path, output: &Ident) -> syn::Result<TokenStream> {
    input.parse::<Token![if]>()?;
    let condition = input.call(Expr::parse_without_eager_brace)?;
    let content;
    braced!(content in input);
    let body = statements(&content, runtime, output)?;
    let otherwise = if input.peek(Token![else]) {
        input.parse::<Token![else]>()?;
        let alternative = if input.peek(Token![if]) {
            conditional(input, runtime, output)?
        } else {
            let content;
            braced!(content in input);
            statements(&content, runtime, output)?
        };
        Some(quote!(else { #alternative }))
    } else {
        None
    };
    Ok(quote!(if #condition { #body } #otherwise))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn child_blocks_parse_to_ordinary_rust_with_control_flow() {
        let runtime: Path = syn::parse_quote!(ui);
        let output = (|input: ParseStream| node(input, &runtime))
            .parse2(quote! {
                VStack(8.0) {
                    Text("Title");
                    let shown: bool = true;
                    if shown { HStack(3.0) { Text("A") Text("B") } }
                    else if other { Text("Other") } else { Text("Empty") }
                    for (index, item) in items.iter().enumerate() { Text(item).id(index as u64) }
                    match kind {
                        0 => { Text("Zero") },
                        1 => Text("One").padding(2.0),
                        2 => previous.clone(),
                        _ if shown => { Text("Shown") },
                        _ => {}
                    }
                }.padding(4.0).environment::<Theme>(theme)
            })
            .unwrap();
        syn::parse2::<Expr>(output).unwrap();
    }
}
