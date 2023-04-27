use crate::derive::Derive;
use crate::private;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_quote, Path};

pub struct Encode;

impl Derive for Encode {
    fn serde_impl(&self) -> (TokenStream, Path) {
        let private = private();
        (
            quote! {
                #private::serialize_compat(encoding, writer, self)?;
            },
            parse_quote!(#private::Serialize),
        )
    }

    fn field_impl(
        &self,
        with_serde: bool,
        field_name: TokenStream,
        encoding: TokenStream,
    ) -> (TokenStream, Path) {
        let private = private();
        if with_serde {
            (
                quote! {
                    #private::serialize_compat(#encoding, writer, #field_name)?;
                },
                parse_quote!(#private::Serialize),
            )
        } else {
            (
                quote! {
                    #private::Encode::encode(#field_name, #encoding, writer)?;
                },
                parse_quote!(#private::Encode),
            )
        }
    }

    fn struct_impl(&self, destructure_fields: TokenStream, do_fields: TokenStream) -> TokenStream {
        quote! {
            let Self #destructure_fields = self;
            #do_fields
        }
    }

    fn variant_impl(
        &self,
        before_fields: TokenStream,
        field_impls: TokenStream,
        destructure_variant: TokenStream,
    ) -> TokenStream {
        quote! {
            #destructure_variant => {
                #before_fields
                #field_impls
            },
        }
    }

    fn is_encode(&self) -> bool {
        true
    }

    fn stream_trait_ident(&self) -> TokenStream {
        let private = private();
        quote! { #private::Write }
    }

    fn trait_ident(&self) -> TokenStream {
        let private = private();
        quote! { #private::Encode }
    }

    fn trait_fn_impl(&self, body: TokenStream) -> TokenStream {
        let private = private();
        quote! {
            #[inline]
            fn encode(&self, encoding: impl #private::Encoding, writer: &mut impl #private::Write) -> #private::Result<()> {
                #body
                Ok(())
            }
        }
    }
}
