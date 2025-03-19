use proc_macro::TokenStream;
use quote::quote;

#[proc_macro_attribute]
pub fn module(_attr: TokenStream, item: TokenStream) -> TokenStream {
  let input = syn::parse_macro_input!(item as syn::ItemStruct);
  let struct_name = &input.ident;

  let expanded = quote! {
    #input

    impl McModule for #struct_name {
      fn name(&self) -> &'static str {
        stringify!(#struct_name)
      }

      fn on_peer_packet(&mut self, source: &PeerId, packet: &Packet, ctx: &mut McContext) {

      }

      fn on_frontend_event(&mut self, packet: &Packet, ctx: &mut McContext) {

      }
    }

    ::inventory::submit! {
        crate::ModuleRegistration {
            constructor: || Arc::new(#struct_name::new()),
        }
    }
  };
  TokenStream::from(expanded)
}
