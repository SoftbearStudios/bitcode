use crate::attribute::BitcodeAttrs;
use crate::private;
use crate::shared::{remove_lifetimes, replace_lifetimes, variant_index};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{
    parse_quote, GenericParam, Generics, Lifetime, LifetimeParam, Path, PredicateLifetime, Type,
    WherePredicate,
};

const DE_LIFETIME: &str = "__de";
fn de_lifetime() -> Lifetime {
    parse_quote!('__de) // Must match DE_LIFETIME.
}

#[derive(Copy, Clone)]
pub enum Item {
    Type,
    Default,
    Populate,
    Decode,
    DecodeInPlace,
}

impl Item {
    const ALL: [Self; 4] = [
        Self::Type,
        Self::Default,
        Self::Populate,
        // No Self::Decode since it's only used for enum variants, not top level struct/enum.
        Self::DecodeInPlace,
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
        field_attrs: &BitcodeAttrs,
    ) -> TokenStream {
        match self {
            Self::Type => {
                let mut de_type = replace_lifetimes(field_type, DE_LIFETIME).to_token_stream();
                if field_attrs.skip {
                    de_type = quote! { ::core::marker::PhantomData<#de_type> };
                }
                let private = private(crate_name);
                let de = de_lifetime();
                quote! {
                    #global_field_name: <#de_type as #private::Decode<#de>>::Decoder,
                }
            }
            Self::Default => quote! {
                #global_field_name: Default::default(),
            },
            Self::Populate => quote! {
                self.#global_field_name.populate(input, __length)?;
            },
            // Only used by enum variants.
            Self::Decode => {
                let value = if field_attrs.skip {
                    quote! {
                        Default::default()
                    }
                } else {
                    quote! {
                        self.#global_field_name.decode()
                    }
                };
                quote! {
                    let #field_name = #value;
                }
            }
            Self::DecodeInPlace => {
                let de_type = replace_lifetimes(field_type, DE_LIFETIME);
                let private = private(crate_name);
                let target = quote! {
                    #private::uninit_field!(out.#real_field_name: #de_type)
                };
                if field_attrs.skip {
                    quote! {{
                        (#target).write(Default::default());
                    }}
                } else {
                    quote! {
                        self.#global_field_name.decode_in_place(#target);
                    }
                }
            }
        }
    }

    fn struct_impl(
        self,
        _ident: &Ident,
        _destructure_fields: &TokenStream,
        do_fields: &TokenStream,
    ) -> TokenStream {
        match self {
            Self::Decode => unimplemented!(),
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
        // if variant_count is 0 or 1 variants don't have to be decoded.
        let decode_variants = variant_count > 1;
        let never = variant_count == 0;

        match self {
            Self::Type => {
                let de = de_lifetime();
                let inners: TokenStream = (0..variant_count).map(|i| inner(self, i)).collect();
                let variants = decode_variants
                    .then(|| {
                        let private = private(crate_name);
                        let c_style = inners.is_empty();
                        quote! { variants: #private::VariantDecoder<#de, #variant_count, #c_style>, }
                    })
                    .unwrap_or_default();
                quote! {
                    #variants
                    #inners
                }
            }
            Self::Default => {
                let variants = decode_variants
                    .then(|| quote! { variants: Default::default(), })
                    .unwrap_or_default();
                let inners: TokenStream = (0..variant_count).map(|i| inner(self, i)).collect();
                quote! {
                    #variants
                    #inners
                }
            }
            Self::Populate => {
                if never {
                    let private = private(crate_name);
                    return quote! {
                        if __length != 0 {
                            return #private::invalid_enum_variant();
                        }
                    };
                }

                let variants = decode_variants
                    .then(|| {
                        quote! { self.variants.populate(input, __length)?; }
                    })
                    .unwrap_or_default();
                let inners: TokenStream = (0..variant_count)
                    .map(|i| {
                        let inner = inner(self, i);
                        if inner.is_empty() {
                            quote! {}
                        } else {
                            let i = variant_index(i);
                            let length = decode_variants
                                .then(|| {
                                    quote! {
                                        let __length = self.variants.length(#i);
                                    }
                                })
                                .unwrap_or_default();
                            quote! {
                                #length
                                #inner
                            }
                        }
                    })
                    .collect();
                quote! {
                    #variants
                    #inners
                }
            }
            Self::Decode => unimplemented!(),
            Self::DecodeInPlace => {
                if never {
                    return quote! {
                        // Safety: View::populate will error on length != 0 so decode won't be called.
                        unsafe { ::core::hint::unreachable_unchecked() }
                    };
                }
                let pattern = |i: usize| {
                    let pattern = pattern(i);
                    quote! {
                        out.write(#pattern);
                    }
                };
                let inner = |i: usize| {
                    inner(Self::Decode, i) // DecodeInPlace doesn't work on enums.
                };

                decode_variants
                    .then(|| {
                        let variants: TokenStream = (0..variant_count)
                            .map(|i| {
                                let inner = inner(i);
                                let pattern = pattern(i);
                                let i = variant_index(i);
                                quote! {
                                    #i => {
                                        #inner
                                        #pattern
                                    },
                                }
                            })
                            .collect();
                        quote! {
                            match self.variants.decode() {
                                #variants
                                // Safety: VariantDecoder<N, _>::decode outputs numbers less than N.
                                _ => unsafe { ::core::hint::unreachable_unchecked() }
                            }
                        }
                    })
                    .or_else(|| {
                        (variant_count == 1).then(|| {
                            let inner = inner(0);
                            let pattern = pattern(0);
                            quote! {
                                #inner
                                #pattern
                            }
                        })
                    })
                    .unwrap_or_default()
            }
        }
    }
}

pub struct Decode;
impl crate::shared::Derive<{ Item::COUNT }> for Decode {
    type Item = Item;
    const ALL: [Self::Item; Item::COUNT] = Item::ALL;

    fn bound(&self, crate_name: &Path) -> Path {
        let private = private(crate_name);
        let de = de_lifetime();
        parse_quote!(#private::Decode<#de>)
    }

    fn skip_bound(&self) -> Option<Path> {
        Some(parse_quote!(Default))
    }

    fn derive_impl(
        &self,
        crate_name: &Path,
        output: [TokenStream; Item::COUNT],
        ident: Ident,
        mut generics: Generics,
        any_static_borrow: bool,
    ) -> TokenStream {
        let input_generics = generics.clone();
        let (_, input_generics, _) = input_generics.split_for_impl();
        let input_ty = quote! { #ident #input_generics };

        // Add 'de lifetime after isolating input_generics.
        let de = de_lifetime();
        let de_where_predicate = WherePredicate::Lifetime(PredicateLifetime {
            lifetime: de.clone(),
            colon_token: parse_quote!(:),
            bounds: generics
                .params
                .iter()
                .filter_map(|p| {
                    if let GenericParam::Lifetime(p) = p {
                        Some(p.lifetime.clone())
                    } else {
                        None
                    }
                })
                .chain(any_static_borrow.then(|| Lifetime::new("'static", Span::call_site())))
                .collect(),
        });

        // Push de_param after bounding 'de: 'a.
        let de_param = GenericParam::Lifetime(LifetimeParam::new(de.clone()));
        generics.params.push(de_param.clone());
        generics
            .make_where_clause()
            .predicates
            .push(de_where_predicate);

        let combined_generics = generics.clone();
        let (impl_generics, _, where_clause) = combined_generics.split_for_impl();

        // Decoder can't contain any lifetimes from input (which would limit reuse of decoder).
        remove_lifetimes(&mut generics);
        generics.params.push(de_param); // Re-add de_param since remove_lifetimes removed it.
        let (decoder_impl_generics, decoder_generics, decoder_where_clause) =
            generics.split_for_impl();

        let [mut type_body, mut default_body, populate_body, decode_in_place_body] = output;
        if type_body.is_empty() {
            type_body = quote! { __spooky: ::core::marker::PhantomData<&#de ()>, };
        }
        if default_body.is_empty() {
            default_body = quote! { __spooky: Default::default(), };
        }

        let decoder_ident = Ident::new(&format!("{ident}Decoder"), Span::call_site());
        let decoder_ty = quote! { #decoder_ident #decoder_generics };
        let private = private(crate_name);

        quote! {
            const _: () = {
                impl #impl_generics #private::Decode<#de> for #input_ty #where_clause {
                    type Decoder = #decoder_ty;
                }

                #[allow(non_snake_case)]
                pub struct #decoder_ident #decoder_impl_generics #decoder_where_clause {
                    #type_body
                }

                // Avoids bounding #impl_generics: Default.
                impl #decoder_impl_generics ::core::default::Default for #decoder_ty #decoder_where_clause {
                    fn default() -> Self {
                        Self {
                            #default_body
                        }
                    }
                }

                impl #decoder_impl_generics #private::View<#de> for #decoder_ty #decoder_where_clause {
                    fn populate(&mut self, input: &mut &#de [u8], __length: usize) -> #private::Result<()> {
                        #populate_body
                        Ok(())
                    }
                }

                impl #impl_generics #private::Decoder<#de, #input_ty> for #decoder_ty #where_clause {
                    #[cfg_attr(not(debug_assertions), inline(always))]
                    fn decode_in_place(&mut self, out: &mut ::core::mem::MaybeUninit<#input_ty>) {
                        #decode_in_place_body
                    }
                }
            };
        }
    }
}
