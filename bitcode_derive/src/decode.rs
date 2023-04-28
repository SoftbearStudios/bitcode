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
                #private::deserialize_compat(encoding, reader)
            },
            parse_quote!(#private::DeserializeOwned),
        )
    }

    fn field_impl(
        &self,
        with_serde: bool,
        field_name: TokenStream,
        _field_type: &Type,
        encoding: Option<TokenStream>,
    ) -> (TokenStream, Path) {
        let private = private();
        let encoding = unwrap_encoding(encoding);
        if with_serde {
            (
                quote! {
                    let #field_name = #private::deserialize_compat(#encoding, reader)?;
                },
                parse_quote!(#private::DeserializeOwned),
            )
        } else {
            (
                quote! {
                    let #field_name = #private::Decode::decode(#encoding, reader)?;
                },
                parse_quote!(#private::Decode),
            )
        }
    }

    fn struct_impl(&self, destructure_fields: TokenStream, do_fields: TokenStream) -> TokenStream {
        quote! {
            #do_fields
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

    fn trait_fn_impl(&self, body: TokenStream) -> TokenStream {
        let private = private();
        quote! {
            fn decode(encoding: impl #private::Encoding, reader: &mut impl #private::Read) -> #private::Result<Self> {
                #body
            }
        }
    }
}
