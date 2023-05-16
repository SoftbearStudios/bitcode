use crate::derive::{unwrap_encoding, Derive};
use crate::private;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_quote, Path, Type};

pub struct Decode;

impl Derive for Decode {
    fn serde_impl(&self) -> (TokenStream, Path) {
        let private = private();
        (
            quote! {
                end_dec!();
                #private::deserialize_compat(encoding, reader)
            },
            parse_quote!(#private::DeserializeOwned),
        )
    }

    fn field_impl(
        &self,
        with_serde: bool,
        field_name: TokenStream,
        field_type: &Type,
        encoding: Option<TokenStream>,
    ) -> (TokenStream, Path) {
        let private = private();
        if with_serde {
            let encoding = unwrap_encoding(encoding);
            (
                // Field is using serde making DECODE_MAX unknown so we flush the current register
                // buffer and read directly from the reader. See optimized_dec! macro in code.rs.
                quote! {
                    let #field_name = #private::deserialize_compat(#encoding, flush!())?;
                },
                parse_quote!(#private::DeserializeOwned),
            )
        } else if let Some(encoding) = encoding {
            (
                // Field has an encoding making DECODE_MAX unknown so we flush the current register
                // buffer and read directly from the reader. See optimized_dec! macro in code.rs.
                quote! {
                    let #field_name = #private::Decode::decode(#encoding, flush!())?;
                },
                parse_quote!(#private::Decode),
            )
        } else {
            (
                // Field has a known DECODE_MAX. dec! will evaluate if it can read from the current
                // register buffer. See optimized_dec! macro in code.rs.
                quote! {
                    dec!(#field_name, #field_type);
                },
                parse_quote!(#private::Decode),
            )
        }
    }

    fn struct_impl(&self, destructure_fields: TokenStream, do_fields: TokenStream) -> TokenStream {
        quote! {
            #do_fields
            end_dec!();
            Ok(Self #destructure_fields)
        }
    }

    fn variant_impl(
        &self,
        before_fields: TokenStream,
        field_impls: TokenStream,
        destructure_variant: TokenStream,
    ) -> TokenStream {
        quote! {
            #before_fields
            #field_impls
            end_dec!();
            #destructure_variant
        }
    }

    fn is_encode(&self) -> bool {
        false
    }

    fn stream_trait_ident(&self) -> TokenStream {
        let private = private();
        quote! { #private::Read }
    }

    fn trait_ident(&self) -> TokenStream {
        let private = private();
        quote! { #private::Decode }
    }

    fn min_bits(&self) -> TokenStream {
        quote! { DECODE_MIN }
    }

    fn max_bits(&self) -> TokenStream {
        quote! { DECODE_MAX }
    }

    fn trait_fn_impl(&self, body: TokenStream) -> TokenStream {
        let private = private();
        quote! {
            #[allow(clippy::all)]
            #[cfg_attr(not(debug_assertions), inline(always))]
            fn decode(encoding: impl #private::Encoding, reader: &mut impl #private::Read) -> #private::Result<Self> {
                #private::optimized_dec!(encoding, reader);
                #body
            }
        }
    }
}
