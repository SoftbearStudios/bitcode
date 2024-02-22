use crate::attribute::BitcodeAttrs;
use crate::bound::FieldBounds;
use crate::shared::{
    destructure_fields, field_name, remove_lifetimes, replace_lifetimes, ReplaceSelves,
};
use crate::{err, private};
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::{
    parse_quote, Data, DeriveInput, Fields, GenericParam, Lifetime, LifetimeParam, Path,
    PredicateLifetime, Result, Type, WherePredicate,
};

const DE_LIFETIME: &str = "__de";
fn de_lifetime() -> Lifetime {
    parse_quote!('__de) // Must match DE_LIFETIME.
}

#[derive(Copy, Clone)]
#[repr(u8)]
enum Item {
    Type,
    Default,
    Populate,
    Decode,
    DecodeInPlace,
}

impl Item {
    const ALL: [Self; 5] = [
        Self::Type,
        Self::Default,
        Self::Populate,
        Self::Decode,
        Self::DecodeInPlace,
    ];
    const COUNT: usize = Self::ALL.len();

    fn field_impl(
        self,
        field_name: TokenStream,
        global_field_name: TokenStream,
        real_field_name: TokenStream,
        field_type: &Type,
    ) -> TokenStream {
        match self {
            Self::Type => {
                let de_type = replace_lifetimes(field_type, DE_LIFETIME);
                let private = private();
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
            Self::Decode => quote! {
                let #field_name = self.#global_field_name.decode();
            },
            Self::DecodeInPlace => {
                let de_type = replace_lifetimes(field_type, DE_LIFETIME);
                let private = private();
                quote! {
                    self.#global_field_name.decode_in_place(#private::uninit_field!(out.#real_field_name: #de_type));
                }
            }
        }
    }

    fn struct_impl(
        self,
        ident: &Ident,
        destructure_fields: &TokenStream,
        do_fields: &TokenStream,
    ) -> TokenStream {
        match self {
            Self::Decode => {
                quote! {
                    #do_fields
                    #ident #destructure_fields
                }
            }
            _ => quote! { #do_fields },
        }
    }

    pub fn variant_impls(
        self,
        variant_count: usize,
        mut pattern: impl FnMut(usize) -> TokenStream,
        mut inner: impl FnMut(Self, usize) -> TokenStream,
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
                        let private = private();
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
                    let private = private();
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
                            let i: u8 = i
                                .try_into()
                                .expect("enums with more than 256 variants are not supported"); // TODO don't panic.
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
            Self::Decode | Self::DecodeInPlace => {
                if never {
                    return quote! {
                        // Safety: View::populate will error on length != 0 so decode won't be called.
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                }
                let mut pattern = |i: usize| {
                    let pattern = pattern(i);
                    matches!(self, Self::DecodeInPlace)
                        .then(|| {
                            quote! {
                                out.write(#pattern);
                            }
                        })
                        .unwrap_or(pattern)
                };
                let item = Self::Decode; // DecodeInPlace doesn't work on enums.

                decode_variants
                    .then(|| {
                        let variants: TokenStream = (0..variant_count)
                            .map(|i| {
                                let inner = inner(item, i);
                                let pattern = pattern(i);
                                let i: u8 = i.try_into().unwrap(); // Already checked in reserve impl.
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
                                _ => unsafe { std::hint::unreachable_unchecked() }
                            }
                        }
                    })
                    .or_else(|| {
                        (variant_count == 1).then(|| {
                            let inner = inner(item, 0);
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

    // TODO dedup with encode.rs
    fn field_impls(
        self,
        global_prefix: Option<&str>,
        fields: &Fields,
        parent_attrs: &BitcodeAttrs,
        bounds: &mut FieldBounds,
    ) -> Result<TokenStream> {
        fields
            .iter()
            .enumerate()
            .map(move |(i, field)| {
                let field_attrs = BitcodeAttrs::parse_field(&field.attrs, parent_attrs)?;

                let name = field_name(i, field, false);
                let real_name = field_name(i, field, true);

                let global_name = global_prefix
                    .map(|global_prefix| {
                        let ident =
                            Ident::new(&format!("{global_prefix}{name}"), Span::call_site());
                        quote! { #ident }
                    })
                    .unwrap_or_else(|| name.clone());

                let field_impl = self.field_impl(name, global_name, real_name, &field.ty);

                let private = private();
                let de = de_lifetime();
                let bound: Path = parse_quote!(#private::Decode<#de>);
                bounds.add_bound_type(field.clone(), &field_attrs, bound);
                Ok(field_impl)
            })
            .collect()
    }
}

struct Output([TokenStream; Item::COUNT]);

impl Output {
    fn make_ghost(mut self) -> Self {
        let type_ = &mut self.0[Item::Type as usize];
        if type_.is_empty() {
            let de = de_lifetime();
            *type_ = quote! { __spooky: std::marker::PhantomData<&#de ()>, };
        }
        let default = &mut self.0[Item::Default as usize];
        if default.is_empty() {
            *default = quote! { __spooky: Default::default(), };
        }
        self
    }
}

pub fn derive_impl(mut input: DeriveInput) -> Result<TokenStream> {
    let attrs = BitcodeAttrs::parse_derive(&input.attrs)?;
    let mut generics = input.generics;
    let mut bounds = FieldBounds::default();

    let ident = input.ident;
    syn::visit_mut::visit_data_mut(&mut ReplaceSelves(&ident), &mut input.data);
    let output = (match input.data {
        Data::Struct(data_struct) => {
            let destructure_fields = &destructure_fields(&data_struct.fields);
            Output(Item::ALL.map(|item| {
                let field_impls = item
                    .field_impls(None, &data_struct.fields, &attrs, &mut bounds)
                    .unwrap(); // TODO don't unwrap
                item.struct_impl(&ident, destructure_fields, &field_impls)
            }))
        }
        Data::Enum(data_enum) => {
            let variant_count = data_enum.variants.len();
            Output(Item::ALL.map(|item| {
                item.variant_impls(
                    variant_count,
                    |i| {
                        let variant = &data_enum.variants[i];
                        let variant_name = &variant.ident;
                        let destructure_fields = destructure_fields(&variant.fields);
                        quote! {
                            #ident::#variant_name #destructure_fields
                        }
                    },
                    |item, i| {
                        let variant = &data_enum.variants[i];
                        let global_prefix = format!("{}_", &variant.ident);
                        let attrs = BitcodeAttrs::parse_variant(&variant.attrs, &attrs).unwrap(); // TODO don't unwrap.
                        item.field_impls(Some(&global_prefix), &variant.fields, &attrs, &mut bounds)
                            .unwrap() // TODO don't unwrap.
                    },
                )
            }))
        }
        Data::Union(u) => err(&u.union_token, "unions are not supported")?,
    })
    .make_ghost();

    bounds.apply_to_generics(&mut generics);
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
            .collect(),
    });

    // Push de_param after bounding 'de: 'a.
    let de_param = GenericParam::Lifetime(LifetimeParam::new(de.clone()));
    generics.params.push(de_param.clone()); // TODO bound to other lifetimes.
    generics
        .make_where_clause()
        .predicates
        .push(de_where_predicate);

    let combined_generics = generics.clone();
    let (impl_generics, _, where_clause) = combined_generics.split_for_impl();

    // Decoder can't contain any lifetimes from input (which would limit reuse of decoder).
    remove_lifetimes(&mut generics);
    generics.params.push(de_param); // Re-add de_param since remove_lifetimes removed it.
    let (decoder_impl_generics, decoder_generics, decoder_where_clause) = generics.split_for_impl();

    let Output([type_body, default_body, populate_body, decode_body, decode_in_place_body]) =
        output;
    let decoder_ident = Ident::new(&format!("{ident}Decoder"), Span::call_site());
    let decoder_ty = quote! { #decoder_ident #decoder_generics };
    let private = private();

    let ret = quote! {
        const _: () = {
            impl #impl_generics #private::Decode<#de> for #input_ty #where_clause {
                type Decoder = #decoder_ty;
            }

            #[allow(non_snake_case)]
            pub struct #decoder_ident #decoder_impl_generics #decoder_where_clause {
                #type_body
            }

            // Avoids bounding #impl_generics: Default.
            impl #decoder_impl_generics std::default::Default for #decoder_ty #decoder_where_clause {
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
                fn decode(&mut self) -> #input_ty {
                    #decode_body
                }

                #[cfg_attr(not(debug_assertions), inline(always))]
                fn decode_in_place(&mut self, out: &mut std::mem::MaybeUninit<#input_ty>) {
                    #decode_in_place_body
                }
            }
        };
    };
    // panic!("{ret}");
    Ok(ret)
}
