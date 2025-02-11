use crate::private;
use crate::shared::{remove_lifetimes, replace_lifetimes, variant_index};
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::{parse_quote, Generics, Path, Type};

#[derive(Copy, Clone)]
pub enum Item {
    Type,
    Default,
    Encode,
    EncodeVectored,
    CollectInto,
    Reserve,
}
impl Item {
    const ALL: [Self; 6] = [
        Self::Type,
        Self::Default,
        Self::Encode,
        Self::EncodeVectored,
        Self::CollectInto,
        Self::Reserve,
    ];
    const COUNT: usize = Self::ALL.len();
}
impl crate::shared::Item for Item {
    fn field_impl(
        self,
        crate_name: &Path,
        field_name: TokenStream,
        global_field_name: TokenStream,
        real_field_name: TokenStream,
        field_type: &Type,
    ) -> TokenStream {
        match self {
            Self::Type => {
                let static_type = replace_lifetimes(field_type, "static");
                let private = private(crate_name);
                quote! {
                    #global_field_name: <#static_type as #private::Encode>::Encoder,
                }
            }
            Self::Default => quote! {
                #global_field_name: Default::default(),
            },
            Self::Encode | Self::EncodeVectored => {
                let static_type = replace_lifetimes(field_type, "static");
                let value = if &static_type != field_type {
                    let underscore_type = replace_lifetimes(field_type, "_");

                    // HACK: Since encoders don't have lifetimes we can't reference <T<'a> as Encode>::Encoder since 'a
                    // does not exist. Instead we replace this with <T<'static> as Encode>::Encoder and transmute it to
                    // T<'a>. No encoder actually encodes T<'static> any differently from T<'a> so this is sound.
                    quote! {
                        unsafe { ::core::mem::transmute::<&#underscore_type, &#static_type>(#field_name) }
                    }
                } else {
                    quote! { #field_name }
                };

                if matches!(self, Self::EncodeVectored) {
                    quote! {
                        self.#global_field_name.encode_vectored(i.clone().map(|me| {
                            let #field_name = &me.#real_field_name;
                            #value
                        }));
                    }
                } else {
                    quote! {
                        self.#global_field_name.encode(#value);
                    }
                }
            }
            Self::CollectInto => quote! {
                self.#global_field_name.collect_into(out);
            },
            Self::Reserve => quote! {
                self.#global_field_name.reserve(__additional);
            },
        }
    }

    fn struct_impl(
        self,
        ident: &Ident,
        destructure_fields: &TokenStream,
        do_fields: &TokenStream,
    ) -> TokenStream {
        match self {
            Self::Encode => {
                quote! {
                    let #ident #destructure_fields = v;
                    #do_fields
                }
            }
            _ => quote! { #do_fields },
        }
    }

    fn enum_impl(
        self,
        crate_name: &Path,
        variant_count: usize,
        pattern: impl Fn(usize) -> TokenStream,
        inner: impl Fn(Self, usize) -> TokenStream,
    ) -> TokenStream {
        // if variant_count is 0 or 1 variants don't have to be encoded.
        let encode_variants = variant_count > 1;
        match self {
            Self::Type => {
                let variants = encode_variants
                    .then(|| {
                        let private = private(crate_name);
                        quote! { variants: #private::VariantEncoder<#variant_count>, }
                    })
                    .unwrap_or_default();
                let inners: TokenStream = (0..variant_count).map(|i| inner(self, i)).collect();
                quote! {
                    #variants
                    #inners
                }
            }
            Self::Default => {
                let variants = encode_variants
                    .then(|| quote! { variants: Default::default(), })
                    .unwrap_or_default();
                let inners: TokenStream = (0..variant_count).map(|i| inner(self, i)).collect();
                quote! {
                    #variants
                    #inners
                }
            }
            Self::Encode => {
                let variants = encode_variants
                    .then(|| {
                        let variants: TokenStream = (0..variant_count)
                            .map(|i| {
                                let pattern = pattern(i);
                                let i = variant_index(i);
                                quote! {
                                    #pattern => #i,
                                }
                            })
                            .collect();
                        quote! {
                            #[allow(unused_variables)]
                            self.variants.encode(&match v {
                                #variants
                            });
                        }
                    })
                    .unwrap_or_default();
                let inners: TokenStream = (0..variant_count)
                    .map(|i| {
                        // We don't know the exact number of this variant since there is more than
                        // one, so we have to reserve one at a time.
                        let reserve = encode_variants
                            .then(|| {
                                let reserve = inner(Self::Reserve, i);
                                quote! {
                                    let __additional = ::core::num::NonZeroUsize::MIN;
                                    #reserve
                                }
                            })
                            .unwrap_or_default();
                        let inner = inner(self, i);
                        let pattern = pattern(i);
                        quote! {
                            #pattern => {
                                #reserve
                                #inner
                            }
                        }
                    })
                    .collect();
                (variant_count != 0)
                    .then(|| {
                        quote! {
                            #variants
                            match v {
                                #inners
                            }
                        }
                    })
                    .unwrap_or_default()
            }
            // This is a copy of Encode::encode_vectored's default impl (which provides no speedup).
            // TODO optimize enum encode_vectored.
            Self::EncodeVectored => quote! {
                for t in i {
                    self.encode(t);
                }
            },
            Self::CollectInto => {
                let variants = encode_variants
                    .then(|| {
                        quote! { self.variants.collect_into(out); }
                    })
                    .unwrap_or_default();
                let inners: TokenStream = (0..variant_count).map(|i| inner(self, i)).collect();
                quote! {
                    #variants
                    #inners
                }
            }
            Self::Reserve => {
                encode_variants
                    .then(|| {
                        quote! { self.variants.reserve(__additional); }
                    })
                    .or_else(|| {
                        (variant_count == 1).then(|| {
                            // We know the exact number of this variant since it's the only one so we can reserve it.
                            inner(self, 0)
                        })
                    })
                    .unwrap_or_default()
            }
        }
    }
}

pub struct Encode;
impl crate::shared::Derive<{ Item::COUNT }> for Encode {
    type Item = Item;
    const ALL: [Self::Item; Item::COUNT] = Item::ALL;

    fn bound(&self, crate_name: &Path) -> Path {
        let private = private(crate_name);
        parse_quote!(#private::Encode)
    }

    fn derive_impl(
        &self,
        crate_name: &Path,
        output: [TokenStream; Item::COUNT],
        ident: Ident,
        mut generics: Generics,
    ) -> TokenStream {
        let input_generics = generics.clone();
        let (impl_generics, input_generics, where_clause) = input_generics.split_for_impl();
        let input_ty = quote! { #ident #input_generics };

        // Encoder can't contain any lifetimes from input (which would limit reuse of encoder).
        remove_lifetimes(&mut generics);
        let (encoder_impl_generics, encoder_generics, encoder_where_clause) =
            generics.split_for_impl();

        let [type_body, default_body, encode_body, encode_vectored_body, collect_into_body, reserve_body] =
            output;
        let encoder_ident = Ident::new(&format!("{ident}Encoder"), Span::call_site());
        let encoder_ty = quote! { #encoder_ident #encoder_generics };
        let private = private(crate_name);

        quote! {
            const _: () = {
                impl #impl_generics #private::Encode for #input_ty #where_clause {
                    type Encoder = #encoder_ty;
                }

                #[allow(non_snake_case)]
                pub struct #encoder_ident #encoder_impl_generics #encoder_where_clause {
                    #type_body
                }

                // Avoids bounding #impl_generics: Default.
                impl #encoder_impl_generics ::core::default::Default for #encoder_ty #encoder_where_clause {
                    fn default() -> Self {
                        Self {
                            #default_body
                        }
                    }
                }

                impl #impl_generics #private::Encoder<#input_ty> for #encoder_ty #where_clause {
                    #[cfg_attr(not(debug_assertions), inline(always))]
                    fn encode(&mut self, v: &#input_ty) {
                        #[allow(unused_imports)]
                        use #private::Buffer as _;
                        #encode_body
                    }

                    // #[cfg_attr(not(debug_assertions), inline(always))]
                    // #[inline(never)]
                    fn encode_vectored<'__v>(&mut self, i: impl Iterator<Item = &'__v #input_ty> + Clone) where #input_ty: '__v {
                        #[allow(unused_imports)]
                        use #private::Buffer as _;
                        #encode_vectored_body
                    }
                }

                impl #encoder_impl_generics #private::Buffer for #encoder_ty #encoder_where_clause {
                    fn collect_into(&mut self, out: &mut #private::Vec<u8>) {
                        #collect_into_body
                    }

                    fn reserve(&mut self, __additional: ::core::num::NonZeroUsize) {
                        #reserve_body
                    }
                }
            };
        }
    }
}
