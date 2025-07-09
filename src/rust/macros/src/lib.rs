use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, FnArg, ItemFn, Signature};

fn thread_check(_args: TokenStream, input: TokenStream, should_match: bool) -> TokenStream {
    let input_fn = parse_macro_input!(input as ItemFn);

    let fn_sig: &Signature = &input_fn.sig;
    let identifier = &fn_sig.ident;
    let block = &input_fn.block;

    if !matches!(fn_sig.inputs.first(), Some(FnArg::Receiver(_))) {
        panic!("Thread protection macros can only be annotated on object-associated methods");
    }

    let expanded: TokenStream = quote! {
        #fn_sig {
            if #should_match != self.am_i_on_object_thread() {
                let msg = format!("Method '{}' was called from {} thread while not allowed",
                                    stringify!(#identifier),
                                    if #should_match { "other" } else { "object's own" });
                panic!("{msg}");
            }
            #block
        }
    }
    .into();
    return TokenStream::from(expanded);
}

#[proc_macro_attribute]
pub fn deny_calls_not_on_object_thread(args: TokenStream, input: TokenStream) -> TokenStream {
    return thread_check(args, input, true);
}

#[proc_macro_attribute]
pub fn deny_calls_on_object_thread(args: TokenStream, input: TokenStream) -> TokenStream {
    return thread_check(args, input, false);
}
