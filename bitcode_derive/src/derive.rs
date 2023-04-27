use crate::attribute::{BitcodeAttrs, VariantEncoding};
use crate::bound::FieldBounds;
use crate::err;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::{parse_quote, Data, DeriveInput, Field, Fields, Generics, Path, Result, Type};

/// Derive code shared between Encode and Decode.
pub trait Derive {
    /// Returns (serde_impl, bound).
    fn serde_impl(&self) -> (TokenStream, Path);

    /// Returns (field_impl, bound).
    fn field_impl(
        &self,
        with_serde: bool,
        field_name: TokenStream,
        encoding: TokenStream,
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

                let field_name = field_name(i, &field);

                let (field_impl, bound) =
                    self.field_impl(field_attrs.with_serde(), field_name, encoding);
                bounds.add_field_bound(field.clone(), bound);
                Ok(field_impl)
            })
            .collect()
    }

    fn derive_impl(&self, input: DeriveInput) -> Result<TokenStream> {
        let attrs = BitcodeAttrs::parse_derive(&input.attrs)?;
        let mut generics = input.generics;
        let mut bounds = FieldBounds::default();

        let ident = input.ident;
        let body = match input.data {
            _ if attrs.with_serde() => {
                let (with_serde_impl, bound) = self.serde_impl();
                add_type_bound(&mut generics, parse_quote!(Self), bound);
                with_serde_impl
            }
            Data::Struct(data_struct) => {
                let destructure_fields = destructure_fields(&data_struct.fields);
                let do_fields = self.field_impls(&data_struct.fields, &attrs, &mut bounds)?;
                self.struct_impl(destructure_fields, do_fields)
            }
            Data::Enum(data_enum) => {
                let variant_encoding = VariantEncoding::parse_data_enum(&data_enum, &attrs)?;
                let variant_impls =
                    (if self.is_encode() {
                        VariantEncoding::encode_variants
                    } else {
                        VariantEncoding::decode_variants
                    })(&variant_encoding, |variant_index, before_fields| {
                        let variant = &data_enum.variants[variant_index];
                        let attrs = BitcodeAttrs::parse_variant(&variant.attrs, &attrs).unwrap();
                        let variant_name = &variant.ident;

                        let destructure_fields = destructure_fields(&variant.fields);
                        let field_impls = self.field_impls(&variant.fields, &attrs, &mut bounds)?;

                        let destructure_variant = quote! {
                            Self::#variant_name #destructure_fields
                        };

                        Ok(self.variant_impl(before_fields, field_impls, destructure_variant))
                    })?;

                let stream_trait = self.stream_trait_ident();
                quote! {
                    use #stream_trait as _;
                    #variant_impls
                }
            }
            Data::Union(u) => err(&u.union_token, "unions are not supported")?,
        };

        bounds.apply_to_generics(&mut generics);

        let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
        let trait_ident = self.trait_ident();
        let impl_trait_fn = self.trait_fn_impl(body);
        Ok(quote! {
            impl #impl_generics #trait_ident for #ident #ty_generics #where_clause {
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
