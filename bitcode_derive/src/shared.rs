use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, ToTokens};
use syn::visit_mut::VisitMut;
use syn::{Field, Fields, GenericParam, Generics, Index, Lifetime, Type, WherePredicate};

pub fn destructure_fields(fields: &Fields) -> TokenStream {
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

pub fn field_name(i: usize, field: &Field, real: bool) -> TokenStream {
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

pub struct ReplaceSelves<'a>(pub &'a Ident);
impl VisitMut for ReplaceSelves<'_> {
    fn visit_ident_mut(&mut self, ident: &mut Ident) {
        if ident == "Self" {
            *ident = self.0.clone();
        }
    }
}
