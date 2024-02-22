use crate::attribute::BitcodeAttrs;
use crate::bound::FieldBounds;
use crate::shared::{
    destructure_fields, field_name, remove_lifetimes, replace_lifetimes, ReplaceSelves,
};
use crate::{err, private};
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::{parse_quote, Data, DeriveInput, Fields, Path, Result, Type};

#[derive(Copy, Clone)]
enum Item {
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

    fn field_impl(
        self,
        field_name: TokenStream,
        global_field_name: TokenStream,
        real_field_name: TokenStream,
        field_type: &Type,
    ) -> TokenStream {
        match self {
            Self::Type => {
                let static_type = replace_lifetimes(field_type, "static");
                let private = private();
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
                        unsafe { std::mem::transmute::<&#underscore_type, &#static_type>(#field_name) }
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

    pub fn variant_impls(
        self,
        variant_count: usize,
        mut pattern: impl FnMut(usize) -> TokenStream,
        mut inner: impl FnMut(Self, usize) -> TokenStream,
    ) -> TokenStream {
        // if variant_count is 0 or 1 variants don't have to be encoded.
        let encode_variants = variant_count > 1;
        match self {
            Self::Type => {
                let variants = encode_variants
                    .then(|| {
                        let private = private();
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
                                let i: u8 = i
                                    .try_into()
                                    .expect("enums with more than 256 variants are not supported"); // TODO don't panic.
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
                        // We don't know the exact number of this variant since there are more than one so we have to
                        // reserve one at a time.
                        let reserve = encode_variants
                            .then(|| {
                                let reserve = inner(Self::Reserve, i);
                                quote! {
                                    let __additional = std::num::NonZeroUsize::MIN;
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
            Self::EncodeVectored => unimplemented!(), // TODO encode enum vectored.
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
                let bound: Path = parse_quote!(#private::Encode);
                bounds.add_bound_type(field.clone(), &field_attrs, bound);
                Ok(field_impl)
            })
            .collect()
    }
}

struct Output([TokenStream; Item::COUNT]);

pub fn derive_impl(mut input: DeriveInput) -> Result<TokenStream> {
    let attrs = BitcodeAttrs::parse_derive(&input.attrs)?;
    let mut generics = input.generics;
    let mut bounds = FieldBounds::default();

    let ident = input.ident;
    syn::visit_mut::visit_data_mut(&mut ReplaceSelves(&ident), &mut input.data);

    let (output, is_encode_vectored) = match input.data {
        Data::Struct(data_struct) => {
            let destructure_fields = &destructure_fields(&data_struct.fields);
            (
                Output(Item::ALL.map(|item| {
                    let field_impls = item
                        .field_impls(None, &data_struct.fields, &attrs, &mut bounds)
                        .unwrap(); // TODO don't unwrap
                    item.struct_impl(&ident, destructure_fields, &field_impls)
                })),
                true,
            )
        }
        Data::Enum(data_enum) => {
            let variant_count = data_enum.variants.len();
            (
                Output(Item::ALL.map(|item| {
                    if matches!(item, Item::EncodeVectored) {
                        return Default::default(); // Unimplemented for now.
                    }

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
                            let attrs =
                                BitcodeAttrs::parse_variant(&variant.attrs, &attrs).unwrap(); // TODO don't unwrap.
                            item.field_impls(
                                Some(&global_prefix),
                                &variant.fields,
                                &attrs,
                                &mut bounds,
                            )
                            .unwrap() // TODO don't unwrap.
                        },
                    )
                })),
                false,
            )
        }
        Data::Union(u) => err(&u.union_token, "unions are not supported")?,
    };

    bounds.apply_to_generics(&mut generics);
    let input_generics = generics.clone();
    let (impl_generics, input_generics, where_clause) = input_generics.split_for_impl();
    let input_ty = quote! { #ident #input_generics };

    // Encoder can't contain any lifetimes from input (which would limit reuse of encoder).
    remove_lifetimes(&mut generics);
    let (encoder_impl_generics, encoder_generics, encoder_where_clause) = generics.split_for_impl();

    let Output(
        [type_body, default_body, encode_body, encode_vectored_body, collect_into_body, reserve_body],
    ) = output;
    let encoder_ident = Ident::new(&format!("{ident}Encoder"), Span::call_site());
    let encoder_ty = quote! { #encoder_ident #encoder_generics };
    let private = private();

    let encode_vectored = is_encode_vectored.then(|| quote! {
        // #[cfg_attr(not(debug_assertions), inline(always))]
        // #[inline(never)]
        fn encode_vectored<'__v>(&mut self, i: impl Iterator<Item = &'__v #input_ty> + Clone) where #input_ty: '__v {
            #[allow(unused_imports)]
            use #private::Buffer as _;
            #encode_vectored_body
        }
    });

    let ret = quote! {
        const _: () = {
            impl #impl_generics #private::Encode for #input_ty #where_clause {
                type Encoder = #encoder_ty;
            }

            #[allow(non_snake_case)]
            pub struct #encoder_ident #encoder_impl_generics #encoder_where_clause {
                #type_body
            }

            // Avoids bounding #impl_generics: Default.
            impl #encoder_impl_generics std::default::Default for #encoder_ty #encoder_where_clause {
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
                #encode_vectored
            }

            impl #encoder_impl_generics #private::Buffer for #encoder_ty #encoder_where_clause {
                fn collect_into(&mut self, out: &mut Vec<u8>) {
                    #collect_into_body
                }

                fn reserve(&mut self, __additional: std::num::NonZeroUsize) {
                    #reserve_body
                }
            }
        };
    };
    // panic!("{ret}");
    Ok(ret)
}
