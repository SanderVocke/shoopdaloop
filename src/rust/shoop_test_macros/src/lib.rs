use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Attribute, ItemFn, LitStr};

fn panic_expectation(attributes: &[Attribute]) -> syn::Result<Option<Option<String>>> {
    let Some(attribute) = attributes
        .iter()
        .find(|attribute| attribute.path().is_ident("should_panic"))
    else {
        return Ok(None);
    };
    let mut expected = None;
    if let syn::Meta::List(_) = &attribute.meta {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("expected") {
                expected = Some(meta.value()?.parse::<LitStr>()?.value());
                Ok(())
            } else {
                Err(meta.error("unsupported should_panic option"))
            }
        })?;
    }
    Ok(Some(expected))
}

/// Mark one test body for native nextest and wasm-bindgen-test.
///
/// Ordinary test attributes such as `ignore` and `should_panic` belong below
/// this attribute and are retained in both expansions.
#[proc_macro_attribute]
pub fn shoop_test(arguments: TokenStream, item: TokenStream) -> TokenStream {
    if !arguments.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "shoop_test takes no arguments; use ordinary test attributes",
        )
        .to_compile_error()
        .into();
    }

    let function = parse_macro_input!(item as ItemFn);
    let panic = match panic_expectation(&function.attrs) {
        Ok(panic) => panic,
        Err(error) => return error.to_compile_error().into(),
    };
    let mut native = function.clone();
    native
        .attrs
        .retain(|attribute| !attribute.path().is_ident("should_panic"));

    let body = native.block;
    let execution = if native.sig.asyncness.take().is_some() {
        quote!(::shoop_wasm_test_support::block_on(async move #body))
    } else {
        quote!((|| #body)())
    };
    native.block = if let Some(expected) = panic {
        let expected = expected
            .as_deref()
            .map(|value| quote!(Some(#value)))
            .unwrap_or_else(|| quote!(None));
        Box::new(syn::parse_quote!({
            ::shoop_wasm_test_support::assert_panics(|| #execution, #expected)
        }))
    } else {
        Box::new(syn::parse_quote!({ #execution }))
    };

    quote! {
        #[cfg(not(target_arch = "wasm32"))]
        #[::shoop_wasm_test_support::tracy_capture_test]
        #native

        #[cfg(target_arch = "wasm32")]
        #[::shoop_wasm_test_support::wasm_bindgen_test]
        #function
    }
    .into()
}
