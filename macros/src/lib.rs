use proc_macro::TokenStream;
use quote::quote;

/// Lifted from actix-web-codegen
#[proc_macro_attribute]
pub fn main(_: TokenStream, item: TokenStream) -> TokenStream {
    let mut output: TokenStream = (quote! {
        #[::drogue_cloud_service_api::webapp::rt::main(system = "::drogue_cloud_service_api::webapp::rt::System")]
    })
    .into();

    output.extend(item);
    output
}
