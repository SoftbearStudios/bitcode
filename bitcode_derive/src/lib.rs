use crate::decode::Decode;
use crate::encode::Encode;
use crate::shared::Derive;
use proc_macro::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{parse_macro_input, DeriveInput, Error};

mod attribute;
mod bound;
mod decode;
mod encode;
mod shared;

macro_rules! derive {
    ($fn_name:ident, $trait_:ident) => {
        #[proc_macro_derive($trait_, attributes(bitcode))]
        pub fn $fn_name(input: TokenStream) -> TokenStream {
            $trait_
                .derive(parse_macro_input!(input as DeriveInput))
                .unwrap_or_else(Error::into_compile_error)
                .into()
        }
    };
}
derive!(derive_encode, Encode);
derive!(derive_decode, Decode);

pub(crate) fn error(spanned: &impl Spanned, s: &str) -> Error {
    Error::new(spanned.span(), s.to_owned())
}

pub(crate) fn err<T>(spanned: &impl Spanned, s: &str) -> Result<T, Error> {
    Err(error(spanned, s))
}

pub(crate) fn private(crate_name: &syn::Path) -> proc_macro2::TokenStream {
    quote! { #crate_name::__private }
}
