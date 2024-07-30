use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn log_entry_exit(_args: TokenStream, input: TokenStream) -> TokenStream {
    // Parse the input as a function
    let input_fn = parse_macro_input!(input as ItemFn);

    // Extract the function name, inputs, and block
    let fn_sig = &input_fn.sig;
    // let inputs = &input_fn.sig.inputs;
    let block = &input_fn.block;

    // Generate the expanded code with logging statements
    let expanded = quote! {
        #fn_sig {
            println!("Entering function");
            #block
            println!("Exiting function");
        }
    };

    TokenStream::from(expanded)
}
