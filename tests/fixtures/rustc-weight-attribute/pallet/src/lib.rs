use proc_macro::TokenStream;

use quote::{format_ident, quote};
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn weight(attribute: TokenStream, item: TokenStream) -> TokenStream {
    let attribute = proc_macro2::TokenStream::from(attribute);
    let item = parse_macro_input!(item as ItemFn);
    let probe = format_ident!("__pallet_weight_probe_{}", item.sig.ident);

    quote! {
        #item

        #[allow(dead_code)]
        fn #probe() {
            let _ = { #attribute };
        }
    }
    .into()
}
