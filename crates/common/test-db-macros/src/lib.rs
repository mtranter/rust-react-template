//! `#[app_test_db::test(TEMPLATE)]`: see `app-test-db`.

use proc_macro::TokenStream;
use quote::quote;
use syn::{Expr, ItemFn, parse_macro_input};

#[proc_macro_attribute]
pub fn test(args: TokenStream, item: TokenStream) -> TokenStream {
    let template = parse_macro_input!(args as Expr);
    let test = parse_macro_input!(item as ItemFn);
    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = test;
    let name = &sig.ident;
    let inputs = &sig.inputs;
    let output = &sig.output;
    quote! {
        #[::core::prelude::v1::test]
        #(#attrs)*
        #vis fn #name() #output {
            async fn #name(#inputs) #output #block
            ::app_test_db::run(
                ::core::concat!(::core::module_path!(), "::", ::core::stringify!(#name)),
                &#template,
                #name,
            )
        }
    }
    .into()
}
