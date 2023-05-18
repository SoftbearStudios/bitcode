use crate::attribute::{BitcodeAttrs, VariantEncoding};
use crate::bound::FieldBounds;
use crate::{err, private};
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::{parse_quote, Data, DeriveInput, Field, Fields, Generics, Path, Result, Type};

struct Output {
    body: TokenStream,
    bit_bounds: BitBounds,
}

pub struct BitBounds {
    min: TokenStream,
    max: TokenStream,
}

impl BitBounds {
    fn zero() -> Self {
        Self::new(0, 0)
    }

    fn unbounded() -> Self {
        Self::new(1, usize::MAX)
    }

    fn new(min: usize, max: usize) -> Self {
        Self {
            min: quote! { #min },
            max: quote! { #max },
        }
    }

    fn add(&mut self, other: Self) {
        let a_min = &self.min;
        let a_max = &self.max;
        let b_min = &other.min;
        let b_max = &other.max;

        *self = Self {
            min: quote! { #a_min + #b_min },
            max: quote! { (#a_max).saturating_add(#b_max) },
        };
    }

    fn or(&mut self, other: Self) {
        let a_min = &self.min;
        let a_max = &self.max;
        let b_min = &other.min;
        let b_max = &other.max;
        let private = private();

        *self = Self {
            min: quote! { #private::min(#a_min, #b_min) },
            max: quote! { #private::max(#a_max, #b_max) },
        };
    }
}

pub fn unwrap_encoding(encoding: Option<TokenStream>) -> TokenStream {
    encoding.unwrap_or_else(|| quote! { encoding })
}

/// Derive code shared between Encode and Decode.
pub trait Derive {
    /// Returns (serde_impl, bound).
    fn serde_impl(&self) -> (TokenStream, Path);

    /// Returns (field_impl, bound).
    fn field_impl(
        &self,
        with_serde: bool,
        field_name: TokenStream,
        field_type: &Type,
        encoding: Option<TokenStream>,
    ) -> (TokenStream, Path);

    fn struct_impl(&self, destructure_fields: TokenStream, do_fields: TokenStream) -> TokenStream;

    fn variant_impl(
        &self,
        before_fields: TokenStream,
        field_impls: TokenStream,
        destructure_variant: TokenStream,
    ) -> TokenStream;

    fn is_encode(&self) -> bool;

    fn stream_trait_ident(&self) -> TokenStream;

    fn trait_ident(&self) -> TokenStream;

    fn min_bits(&self) -> TokenStream;

    fn max_bits(&self) -> TokenStream;

    fn trait_fn_impl(&self, body: TokenStream) -> TokenStream;

    fn field_impls(
        &self,
        fields: &Fields,
        parent_attrs: &BitcodeAttrs,
        bounds: &mut FieldBounds,
    ) -> Result<TokenStream> {
        fields
            .iter()
            .enumerate()
            .map(move |(i, field)| {
                let field_attrs = BitcodeAttrs::parse_field(&field.attrs, parent_attrs)?;
                let encoding = field_attrs.get_encoding();

                let field_name = field_name(i, field);
                let field_type = &field.ty;

                let (field_impl, bound) =
                    self.field_impl(field_attrs.with_serde(), field_name, field_type, encoding);
                bounds.add_bound_type(field.clone(), &field_attrs, bound);
                Ok(field_impl)
            })
            .collect()
    }

    fn field_bit_bounds(&self, fields: &Fields, parent_attrs: &BitcodeAttrs) -> BitBounds {
        let mut recursive_max = quote! { 0usize };
        let min: TokenStream = fields
            .iter()
            .map(|field| {
                let ty = &field.ty;
                let field_attrs = BitcodeAttrs::parse_field(&field.attrs, parent_attrs).unwrap();

                // Encodings can make our bounds inaccurate and serde types can't give us bounds.
                let unknown_bounds =
                    field_attrs.get_encoding().is_some() || field_attrs.with_serde();
                let BitBounds { min, max } = if unknown_bounds {
                    BitBounds::unbounded()
                } else {
                    let max = if field_attrs.is_recursive() {
                        quote! { usize::MAX }
                    } else {
                        let max_bits = self.max_bits();
                        quote! { <#ty>::#max_bits }
                    };

                    let min_bits = self.min_bits();
                    BitBounds {
                        min: quote! {<#ty>::#min_bits },
                        max,
                    }
                };

                recursive_max = quote! { #recursive_max.saturating_add(#max) };
                let min = quote! { #min + };
                min
            })
            .collect();

        let min = quote! { #min 0 };
        let max = quote! { #recursive_max };

        BitBounds { min, max }
    }

    fn derive_impl(&self, input: DeriveInput) -> Result<TokenStream> {
        let attrs = BitcodeAttrs::parse_derive(&input.attrs)?;
        let mut generics = input.generics;
        let mut bounds = FieldBounds::default();

        let ident = input.ident;
        let output = match input.data {
            _ if attrs.with_serde() => {
                let (body, bound) = self.serde_impl();
                add_type_bound(&mut generics, parse_quote!(Self), bound);

                Output {
                    body,
                    bit_bounds: BitBounds::unbounded(),
                }
            }
            Data::Struct(data_struct) => {
                let destructure_fields = destructure_fields(&data_struct.fields);
                let do_fields = self.field_impls(&data_struct.fields, &attrs, &mut bounds)?;
                let body = self.struct_impl(destructure_fields, do_fields);
                let bit_bounds = self.field_bit_bounds(&data_struct.fields, &attrs);

                Output { body, bit_bounds }
            }
            Data::Enum(data_enum) => {
                let variant_encoding = VariantEncoding::parse_data_enum(&data_enum, &attrs)?;
                let mut enum_bit_bounds: Option<BitBounds> = None;

                let variant_impls = (if self.is_encode() {
                    VariantEncoding::encode_variants
                } else {
                    VariantEncoding::decode_variants
                })(
                    &variant_encoding,
                    |variant_index, before_fields, bits| {
                        let variant = &data_enum.variants[variant_index];
                        let attrs = BitcodeAttrs::parse_variant(&variant.attrs, &attrs).unwrap();
                        let variant_name = &variant.ident;

                        let destructure_fields = destructure_fields(&variant.fields);
                        let field_impls = self.field_impls(&variant.fields, &attrs, &mut bounds)?;

                        let destructure_variant = quote! {
                            Self::#variant_name #destructure_fields
                        };

                        let body =
                            self.variant_impl(before_fields, field_impls, destructure_variant);
                        let mut variant_bit_bounds = self.field_bit_bounds(&variant.fields, &attrs);

                        // Bits to encode variant index.
                        variant_bit_bounds.add(BitBounds::new(bits, bits));

                        if let Some(enum_bit_bounds) = &mut enum_bit_bounds {
                            enum_bit_bounds.or(variant_bit_bounds);
                        } else {
                            enum_bit_bounds = Some(variant_bit_bounds)
                        }

                        Ok(body)
                    },
                )?;

                let stream_trait = self.stream_trait_ident();
                let body = quote! {
                    use #stream_trait as _;
                    #variant_impls
                };

                Output {
                    body,
                    bit_bounds: enum_bit_bounds.unwrap_or_else(BitBounds::zero),
                }
            }
            Data::Union(u) => err(&u.union_token, "unions are not supported")?,
        };

        bounds.apply_to_generics(&mut generics);

        let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
        let trait_ident = self.trait_ident();

        let BitBounds { min, max } = output.bit_bounds;
        let min_bits = self.min_bits();
        let max_bits = self.max_bits();

        let bit_bounds = quote! {
            const #min_bits: usize = #min;
            const #max_bits: usize = #max;
        };

        let impl_trait_fn = self.trait_fn_impl(output.body);
        Ok(quote! {
            impl #impl_generics #trait_ident for #ident #ty_generics #where_clause {
                #bit_bounds
                #impl_trait_fn
            }
        })
    }
}

fn add_type_bound(generics: &mut Generics, typ: Type, bound: Path) {
    generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#typ: #bound));
}

fn destructure_fields(fields: &Fields) -> TokenStream {
    let field_names = fields.iter().enumerate().map(|(i, f)| field_name(i, f));
    match fields {
        Fields::Named(_) => quote! {
            {#(#field_names),*}
        },
        Fields::Unnamed(_) => quote! {
            (#(#field_names),*)
        },
        Fields::Unit => quote! {},
    }
}

fn field_name(i: usize, field: &Field) -> TokenStream {
    field
        .ident
        .as_ref()
        .map(|id| quote! {#id})
        .unwrap_or_else(|| {
            let name = format!("f{i}");
            let ident = Ident::new(&name, Span::call_site());
            quote! {#ident}
        })
}
