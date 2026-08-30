use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, Attribute, Ident, ItemFn, LitStr, Token};

#[derive(Default)]
struct ShoopTestOptions {
    no_wasm: Option<LitStr>,
    no_trace: Option<LitStr>,
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
                "no_trace" => &mut options.no_trace,
                "wasm_only" => &mut options.wasm_only,
                _ => {
                    return Err(syn::Error::new(
                        name.span(),
                        "unknown shoop_test modifier; expected no_wasm, no_trace, or wasm_only",
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
        if options.no_wasm.is_some() && options.wasm_only.is_some() {
            return Err(syn::Error::new(
                options.wasm_only.as_ref().unwrap().span(),
                "no_wasm and wasm_only cannot be combined",
            ));
        }
        Ok(options)
    }
}

fn returns_result(function: &ItemFn) -> bool {
    let syn::ReturnType::Type(_, output) = &function.sig.output else {
        return false;
    };
    let syn::Type::Path(path) = output.as_ref() else {
        return false;
    };
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "Result")
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

/// Mark one test body for native nextest with Perfetto capture and wasm-bindgen-test.
///
/// `no_wasm`, `no_trace`, and `wasm_only` opt out of defaults and require a
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
    let native_returns_result = returns_result(&native);
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
    } else {
        if options.no_trace.is_none() {
            let traced_body = native.block;
            native.block = if native_returns_result {
                Box::new(syn::parse_quote!({
                    ::shoop_wasm_test_support::run_test_result(|| #traced_body)
                }))
            } else {
                Box::new(syn::parse_quote!({
                    ::shoop_wasm_test_support::run_test(|| #traced_body)
                }))
            };
        }
        quote! {
            #[cfg(not(target_arch = "wasm32"))]
            #[test]
            #native
        }
    };
    let wasm_test = if options.no_wasm.is_some() {
        quote!()
    } else {
        let mut wasm = function;
        if options.no_trace.is_none() {
            let test_name = wasm.sig.ident.clone();
            let body = wasm.block;
            wasm.block = if wasm.sig.asyncness.is_some() {
                Box::new(syn::parse_quote!({
                    ::shoop_wasm_test_support::wasm_test_trace_begin(
                        module_path!(),
                        stringify!(#test_name),
                    );
                    let output = (async move #body).await;
                    ::shoop_wasm_test_support::wasm_test_trace_finish();
                    output
                }))
            } else {
                Box::new(syn::parse_quote!({
                    ::shoop_wasm_test_support::wasm_test_trace_begin(
                        module_path!(),
                        stringify!(#test_name),
                    );
                    let output = (|| #body)();
                    ::shoop_wasm_test_support::wasm_test_trace_finish();
                    output
                }))
            };
        }
        quote! {
            #[cfg(target_arch = "wasm32")]
            #[::shoop_wasm_test_support::wasm_bindgen_test]
            #wasm
        }
    };

    quote! {
        #native_test
        #wasm_test
    }
    .into()
}
