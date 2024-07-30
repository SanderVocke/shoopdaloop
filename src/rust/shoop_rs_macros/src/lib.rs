use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, Signature, FnArg, Expr};

fn panic_with_message(message : &str) {
    panic!("{message}");
}

fn thread_check(args : TokenStream, input : TokenStream, should_match : bool) -> TokenStream {
    let input_fn = parse_macro_input!(input as ItemFn);

    let fn_sig : &Signature = &input_fn.sig;
    let identifier = &fn_sig.ident;
    let block = &input_fn.block;
    let args_str = args.to_string();
    let fallback : TokenStream = "".parse().unwrap();
    let should_panic = args_str.is_empty();

    if !matches!(fn_sig.inputs.first(), Some(FnArg::Receiver(_))) {
        panic!("Thread protection macros can only be annotated on object-associated methods");
    }

    if !should_panic {
        let args_expr : Expr = syn::parse(args).unwrap();
        let expanded : TokenStream = quote! {
            #fn_sig {
                if #should_match != is_called_from_qobj_thread(self) {
                    let msg = format!("Method '{}' was called from {} thread while not allowed",
                                      stringify!(#identifier),
                                      if #should_match { "other" } else { "object's own" });
                    #args_expr(&msg);
                }
                #block
            }
        }.into();
        return TokenStream::from(expanded);
    } else {
        let expanded : TokenStream = quote! {
            #fn_sig {
                if #should_match != is_called_from_qobj_thread(self) {
                    let msg = format!("Method '{}' was called from {} thread while not allowed",
                                      stringify!(#identifier),
                                      if #should_match { "other" } else { "object's own" });
                    panic!("{msg}");
                }
                #block
            }
        }.into();
        return TokenStream::from(expanded);
    }
}

#[proc_macro_attribute]
pub fn allow_own_thread_only(args: TokenStream, input: TokenStream) -> TokenStream { return thread_check(args, input, true); }

#[proc_macro_attribute]
pub fn allow_other_thread_only(args: TokenStream, input: TokenStream) -> TokenStream { return thread_check(args, input, false); }

