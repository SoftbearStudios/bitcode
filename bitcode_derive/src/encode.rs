use crate::derive::{unwrap_encoding, Derive};
use crate::private;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_quote, Path, Type};

pub struct Encode;

impl Derive for Encode {
    fn serde_impl(&self) -> (TokenStream, Path) {
        let private = private();
        (
            quote! {
                #private::serialize_compat(self, encoding, writer)?;
            },
            parse_quote!(#private::Serialize),
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
                // Field is using serde making ENCODE_MAX unknown so we flush the current register
                // buffer and write directly to the writer. See optimized_enc! macro in code.rs.
                quote! {
                    #private::serialize_compat(#field_name, #encoding, flush!())?;
                },
                parse_quote!(#private::Serialize),
            )
        } else if let Some(encoding) = encoding {
            (
                // Field has an encoding making ENCODE_MAX unknown so we flush the current register
                // buffer and write directly to the writer. See optimized_enc! macro in code.rs.
                quote! {
                    #private::Encode::encode(#field_name, #encoding, flush!())?;
                },
                parse_quote!(#private::Encode),
            )
        } else {
            (
                // Field has a known ENCODE_MAX. enc! will evaluate if it can fit within the current
                // register buffer. See optimized_enc! macro in code.rs.
                quote! {
                    enc!(#field_name, #field_type);
                },
                parse_quote!(#private::Encode),
            )
        }
    }

    fn struct_impl(&self, destructure_fields: TokenStream, do_fields: TokenStream) -> TokenStream {
        let private = private();
        quote! {
            let Self #destructure_fields = self;
            #private::optimized_enc!(encoding, writer);
            #do_fields
            end_enc!();
        }
    }

    fn variant_impl(
        &self,
        before_fields: TokenStream,
        field_impls: TokenStream,
        destructure_variant: TokenStream,
    ) -> TokenStream {
        let private = private();
        quote! {
            #destructure_variant => {
                #private::optimized_enc!(encoding, writer);
                #before_fields
                #field_impls
                end_enc!();
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

    fn min_bits(&self) -> TokenStream {
        quote! { ENCODE_MIN }
    }

    fn max_bits(&self) -> TokenStream {
        quote! { ENCODE_MAX }
    }

    fn trait_fn_impl(&self, body: TokenStream) -> TokenStream {
        let private = private();
        quote! {
            #[allow(clippy::all)]
            #[cfg_attr(not(debug_assertions), inline(always))]
            fn encode(&self, encoding: impl #private::Encoding, writer: &mut impl #private::Write) -> #private::Result<()> {
                #body
                Ok(())
            }
        }
    }
}
