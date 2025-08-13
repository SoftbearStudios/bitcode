use crate::attribute::BitcodeAttrs;
use crate::bound::FieldBounds;
use crate::err;
use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, ToTokens};
use syn::visit_mut::VisitMut;
use syn::{
    Data, DataStruct, DeriveInput, Field, Fields, GenericParam, Generics, Index, Lifetime, Path,
    Result, Type, WherePredicate,
};

type VariantIndex = u8;
pub fn variant_index(i: usize) -> VariantIndex {
    i.try_into().unwrap()
}

pub trait Item: Copy + Sized {
    fn field_impl(
        self,
        crate_name: &Path,
        field_name: TokenStream,
        global_field_name: TokenStream,
        real_field_name: TokenStream,
        field_type: &Type,
        field_attrs: &BitcodeAttrs,
    ) -> TokenStream;

    fn struct_impl(
        self,
        ident: &Ident,
        destructure_fields: &TokenStream,
        do_fields: &TokenStream,
    ) -> TokenStream;

    fn enum_impl(
        self,
        crate_name: &Path,
        variant_count: usize,
        pattern: impl Fn(usize) -> TokenStream,
        inner: impl Fn(Self, usize) -> TokenStream,
    ) -> TokenStream;

    fn field_impls(
        self,
        crate_name: &Path,
        global_prefix: Option<&str>,
        fields: &Fields,
        attrs: &Vec<BitcodeAttrs>,
    ) -> TokenStream {
        fields
            .iter()
            .enumerate()
            .map(move |(i, field)| {
                let attrs = &attrs[i];
                let name = field_name(i, field, false);
                let real_name = field_name(i, field, true);
                let global_name = global_prefix
                    .map(|global_prefix| {
                        let ident =
                            Ident::new(&format!("{global_prefix}{name}"), Span::call_site());
                        quote! { #ident }
                    })
                    .unwrap_or_else(|| name.clone());

                self.field_impl(crate_name, name, global_name, real_name, &field.ty, attrs)
            })
            .collect()
    }
}

pub trait Derive<const ITEM_COUNT: usize> {
    type Item: Item;
    const ALL: [Self::Item; ITEM_COUNT];

    /// `Encode` in `T: Encode`.
    fn bound(&self, crate_name: &Path) -> Path;

    /// Bound for skipped fields, e.g. `Default`
    fn skip_bound(&self) -> Option<Path>;

    /// Generates the derive implementation.
    fn derive_impl(
        &self,
        crate_name: &Path,
        output: [TokenStream; ITEM_COUNT],
        ident: Ident,
        generics: Generics,
        any_static_borrow: bool,
    ) -> TokenStream;

    fn field_attrs(
        &self,
        crate_name: &Path,
        fields: &Fields,
        attrs: &BitcodeAttrs,
        bounds: &mut FieldBounds,
    ) -> Result<Vec<BitcodeAttrs>> {
        fields
            .iter()
            .map(|field| {
                let field_attrs = BitcodeAttrs::parse_field(&field.attrs, attrs)?;
                let bound = if field_attrs.skip {
                    self.skip_bound()
                } else {
                    Some(self.bound(crate_name))
                };
                if let Some(bound) = bound {
                    bounds.add_bound_type(field.clone(), &field_attrs, bound);
                }
                Ok(field_attrs)
            })
            .collect()
    }

    fn derive(&self, mut input: DeriveInput) -> Result<TokenStream> {
        let attrs = BitcodeAttrs::parse_derive(&input.attrs)?;
        let ident = input.ident;
        syn::visit_mut::visit_data_mut(&mut ReplaceSelves(&ident), &mut input.data);
        let mut bounds = FieldBounds::default();

        let output = match input.data {
            Data::Struct(DataStruct { ref fields, .. }) => {
                // Used for adding `bounds` and skipping fields. Would be used by `#[bitcode(with_serde)]`.
                let field_attrs =
                    self.field_attrs(&attrs.crate_name, fields, &attrs, &mut bounds)?;

                let destructure_fields = &destructure_fields(fields);
                Self::ALL.map(|item| {
                    let field_impls =
                        item.field_impls(&attrs.crate_name, None, fields, &field_attrs);
                    item.struct_impl(&ident, destructure_fields, &field_impls)
                })
            }
            Data::Enum(data_enum) => {
                let max_variants = VariantIndex::MAX as usize + 1;
                if data_enum.variants.len() > max_variants {
                    return err(
                        &ident,
                        &format!("enums with more than {max_variants} variants are not supported"),
                    );
                }

                // Used for adding `bounds` and skipping fields. Would be used by `#[bitcode(with_serde)]`.
                let variant_attrs = data_enum
                    .variants
                    .iter()
                    .map(|variant| {
                        let attrs = BitcodeAttrs::parse_variant(&variant.attrs, &attrs)?;
                        self.field_attrs(&attrs.crate_name, &variant.fields, &attrs, &mut bounds)
                    })
                    .collect::<Result<Vec<_>>>()?;

                Self::ALL.map(|item| {
                    item.enum_impl(
                        &attrs.crate_name,
                        data_enum.variants.len(),
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
                            let variant_attrs = &variant_attrs[i];
                            let global_prefix = format!("{}_", &variant.ident);
                            item.field_impls(
                                &attrs.crate_name,
                                Some(&global_prefix),
                                &variant.fields,
                                variant_attrs,
                            )
                        },
                    )
                })
            }
            Data::Union(_) => err(&ident, "unions are not supported")?,
        };
        let (generics, any_static_borrow) = bounds.added_to(input.generics);
        Ok(self.derive_impl(
            &attrs.crate_name,
            output,
            ident,
            generics,
            any_static_borrow,
        ))
    }
}

fn destructure_fields(fields: &Fields) -> TokenStream {
    let field_names = fields
        .iter()
        .enumerate()
        .map(|(i, f)| field_name(i, f, false));
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

fn field_name(i: usize, field: &Field, real: bool) -> TokenStream {
    field
        .ident
        .as_ref()
        .map(|id| quote! {#id})
        .unwrap_or_else(|| {
            if real {
                Index::from(i).to_token_stream()
            } else {
                Ident::new(&format!("f{i}"), Span::call_site()).to_token_stream()
            }
        })
}

pub fn remove_lifetimes(generics: &mut Generics) {
    generics.params = std::mem::take(&mut generics.params)
        .into_iter()
        .filter(|param| !matches!(param, GenericParam::Lifetime(_)))
        .collect();
    if let Some(where_clause) = &mut generics.where_clause {
        where_clause.predicates = std::mem::take(&mut where_clause.predicates)
            .into_iter()
            .filter(|predicate| !matches!(predicate, WherePredicate::Lifetime(_)))
            .collect()
    }
}

#[must_use]
pub fn replace_lifetimes(t: &Type, s: &str) -> Type {
    let mut t = t.clone();
    syn::visit_mut::visit_type_mut(&mut ReplaceLifetimes(s), &mut t);
    t
}

struct ReplaceLifetimes<'a>(&'a str);
impl VisitMut for ReplaceLifetimes<'_> {
    fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
        lifetime.ident = Ident::new(self.0, lifetime.ident.span());
    }
}

struct ReplaceSelves<'a>(pub &'a Ident);
impl VisitMut for ReplaceSelves<'_> {
    fn visit_ident_mut(&mut self, ident: &mut Ident) {
        if ident == "Self" {
            *ident = self.0.clone();
        }
    }
}
