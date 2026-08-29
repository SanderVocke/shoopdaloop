use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, Attribute, Ident, ItemFn, LitStr, Token};

#[derive(Default)]
struct ShoopTestOptions {
    no_wasm: Option<LitStr>,
    no_tracy: Option<LitStr>,
    wasm_only: Option<LitStr>,
}

impl Parse for ShoopTestOptions {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut options = Self::default();
        while !input.is_empty() {
            let name = input.parse::<Ident>()?;
            input.parse::<Token![=]>()?;
            let reason = input.parse::<LitStr>()?;
            if reason.value().trim().is_empty() {
                return Err(syn::Error::new(
                    reason.span(),
                    "test modifier reason is empty",
                ));
            }
            let slot = match name.to_string().as_str() {
                "no_wasm" => &mut options.no_wasm,
                "no_tracy" => &mut options.no_tracy,
                "wasm_only" => &mut options.wasm_only,
                _ => {
                    return Err(syn::Error::new(
                        name.span(),
                        "unknown shoop_test modifier; expected no_wasm, no_tracy, or wasm_only",
                    ));
                }
            };
            if slot.replace(reason).is_some() {
                return Err(syn::Error::new(
                    name.span(),
                    "duplicate shoop_test modifier",
                ));
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        if let (Some(_), Some(wasm_only)) = (&options.no_wasm, &options.wasm_only) {
            return Err(syn::Error::new(
                wasm_only.span(),
                "no_wasm and wasm_only cannot be combined",
            ));
        }
        Ok(options)
    }
}

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

/// Mark one test body for native nextest with Tracy capture and wasm-bindgen-test.
///
/// `no_wasm`, `no_tracy`, and `wasm_only` opt out of defaults and require a
/// non-empty reason. Ordinary test attributes such as `ignore` and
/// `should_panic` belong below this attribute and are retained.
#[proc_macro_attribute]
pub fn shoop_test(arguments: TokenStream, item: TokenStream) -> TokenStream {
    let options = parse_macro_input!(arguments as ShoopTestOptions);
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

    let native_test = if options.wasm_only.is_some() {
        quote!()
    } else if options.no_tracy.is_some() {
        quote! {
            #[cfg(not(target_arch = "wasm32"))]
            #[test]
            #native
        }
    } else {
        quote! {
            #[cfg(not(target_arch = "wasm32"))]
            #[::shoop_wasm_test_support::tracy_capture_test]
            #native
        }
    };
    let wasm_test = if options.no_wasm.is_some() {
        quote!()
    } else {
        quote! {
            #[cfg(target_arch = "wasm32")]
            #[::shoop_wasm_test_support::wasm_bindgen_test]
            #function
        }
    };

    quote! {
        #native_test
        #wasm_test
    }
    .into()
}
