use super::*;

fn parse(source: &str) -> syn::Result<Model> {
    Model::parse(syn::parse_str(source)?)
}

fn error(source: &str) -> String {
    match parse(source) {
        Ok(_) => panic!("expected a diagnostic"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn diagnostics_identify_invalid_declarations() {
    assert!(error("struct Tuple(u8);").contains("named fields"));
    assert!(error("struct M { #[observation(other)] value: u8 }").contains("expected `ignore`"));
    assert!(error("struct M { #[observation(ignore, ignore)] value: u8 }").contains("duplicate"));
    assert!(error("struct M { #[observation()] value: u8 }").contains("observation(ignore)"));
    assert!(
        error("struct M { #[observation(ignore = false)] value: u8 }")
            .contains("does not take a value")
    );
    assert!(error("#[derive(Copy)] struct M {}").contains("use Clone, not Copy"));
    assert!(error("#[derive(Ord)] struct M {}").contains("order field values"));
}

#[test]
fn generated_items_parse_with_generics_derives_visibility_and_attributes() {
    let model = parse(
        r#"
        #[derive(Clone, Eq, PartialEq, Debug, Default, Hash)]
        #[cfg(any())]
        pub(crate) struct Example<T: Send = u8, const N: usize = 2>
        where T: Default {
            /// Payload.
            pub(crate) value: [T; N],
            #[cfg(any())]
            optional: DoesNotExist,
            #[observation(ignore)]
            generation: usize,
        }
    "#,
    )
    .unwrap();
    let output = model.expand(&syn::parse_quote!(::renamed_runtime));
    let file: syn::File = syn::parse2(output).unwrap();
    assert_eq!(file.items.len(), 10);
    assert!(model.debug && model.default && model.hash);
    assert!(model.fields[2].ignored);
    assert!(!model.fields[0].ignored);
    assert_eq!(model.fields[1].conditions.len(), 1);
}

#[test]
fn self_in_a_property_means_the_model_not_the_generated_storage() {
    let model = parse("struct Chain<T> { item: T, next: Option<Self> }").unwrap();
    assert_eq!(model.fields[1].ty, syn::parse_quote!(Option<Chain<T>>));
}

#[test]
fn raw_names_and_qualified_builtin_derives_are_supported() {
    let model =
        parse("#[derive(::std::clone::Clone, ::core::fmt::Debug)] struct r#Type { r#type: u32 }")
            .unwrap();
    assert_eq!(model.storage.to_string(), "__TypeFields");
    assert!(model.debug);
    syn::parse2::<syn::File>(model.expand(&syn::parse_quote!(crate::runtime))).unwrap();
}

#[test]
fn options_report_unknown_and_duplicate_entries() {
    let input = quote!(
        struct Model {}
    );
    let error = super::super::expand(quote!(runtime = x), input.clone()).unwrap_err();
    assert!(error.to_string().contains("crate = path"));
    let error = super::super::expand(quote!(crate = x, crate = y), input).unwrap_err();
    assert!(error.to_string().contains("duplicate"));
}
