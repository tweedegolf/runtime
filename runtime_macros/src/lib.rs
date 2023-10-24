use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, ItemFn, ReturnType, Signature, Visibility};

#[proc_macro_attribute]
pub fn main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let ItemFn {
        block, sig, vis, ..
    } = parse_macro_input!(item as ItemFn);

    assert_eq!(
        vis,
        Visibility::Inherited,
        "the main function should not be public"
    );
    assert_eq!(
        sig,
        Signature {
            constness: None,
            asyncness: Some(Default::default()),
            unsafety: None,
            abi: None,
            fn_token: Default::default(),
            ident: format_ident!("main"),
            generics: Default::default(),
            paren_token: Default::default(),
            inputs: Default::default(),
            variadic: None,
            output: ReturnType::Default,
        },
        "the main function's signature should be `fn main() {{ todo!() }}`"
    );

    quote! {
        fn main() {
            let (executor, spawner) = ::runtime::executor::new_executor_spawner();

            spawner.spawn(async #block);

            drop(spawner);

            executor.run();
        }
    }
    .into()
}
