use crate::decode::Decode;
use crate::derive::Derive;
use crate::encode::Encode;
use proc_macro::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{parse_macro_input, DeriveInput};

mod attribute;
mod bound;
mod decode;
mod derive;
mod encode;
mod huffman;

#[proc_macro_derive(Encode, attributes(bitcode, bitcode_hint))]
pub fn derive_encode(input: TokenStream) -> TokenStream {
    derive(Encode, input)
}

#[proc_macro_derive(Decode, attributes(bitcode, bitcode_hint))]
pub fn derive_decode(input: TokenStream) -> TokenStream {
    derive(Decode, input)
}

fn derive(derive: impl Derive, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive
        .derive_impl(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

pub(crate) fn error(spanned: &impl Spanned, s: &str) -> syn::Error {
    syn::Error::new(spanned.span(), s.to_owned())
}

pub(crate) fn err<T>(spanned: &impl Spanned, s: &str) -> Result<T, syn::Error> {
    Err(error(spanned, s))
}

pub(crate) fn private() -> proc_macro2::TokenStream {
    quote! { bitcode::__private }
}
