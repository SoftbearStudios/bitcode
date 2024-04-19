use crate::{err, error};
use alloc::format;
use core::str::FromStr;
use proc_macro2::TokenStream;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{parse2, Attribute, Expr, ExprLit, Lit, Meta, Path, Result, Token, Type};

enum BitcodeAttr {
    BoundType(Type),
}

impl BitcodeAttr {
    fn new(nested: &Meta) -> Result<Self> {
        let path = path_ident_string(nested.path(), &nested)?;
        match path.as_str() {
            "bound_type" => match nested {
                Meta::NameValue(name_value) => {
                    let expr = &name_value.value;
                    let str_lit = match expr {
                        Expr::Lit(ExprLit {
                            lit: Lit::Str(v), ..
                        }) => v,
                        _ => return err(&expr, "expected string e.g. \"T\""),
                    };

                    let value = TokenStream::from_str(&str_lit.value()).unwrap();
                    Ok(Self::BoundType(
                        parse2(value).map_err(|e| error(str_lit, &format!("{e}")))?,
                    ))
                }
                _ => err(&nested, "expected name value"),
            },
            _ => err(&nested, "unknown attribute"),
        }
    }

    fn apply(self, attrs: &mut BitcodeAttrs, nested: &Meta) -> Result<()> {
        match self {
            Self::BoundType(bound_type) => {
                if let AttrType::Field { bound_type: b, .. } = &mut attrs.attr_type {
                    if b.is_some() {
                        return err(nested, "duplicate");
                    }
                    *b = Some(bound_type);
                    Ok(())
                } else {
                    err(nested, "can only apply bound to fields")
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct BitcodeAttrs {
    attr_type: AttrType,
}

#[derive(Clone)]
enum AttrType {
    Derive,
    Variant,
    Field { bound_type: Option<Type> },
}

impl BitcodeAttrs {
    fn new(attr_type: AttrType) -> Self {
        Self { attr_type }
    }

    pub fn bound_type(&self) -> Option<Type> {
        match &self.attr_type {
            AttrType::Field { bound_type, .. } => bound_type.as_ref().cloned(),
            _ => unreachable!(),
        }
    }

    pub fn parse_derive(attrs: &[Attribute]) -> Result<Self> {
        let mut ret = Self::new(AttrType::Derive);
        ret.parse_inner(attrs)?;
        Ok(ret)
    }

    pub fn parse_variant(attrs: &[Attribute], _derive_attrs: &Self) -> Result<Self> {
        let mut ret = Self::new(AttrType::Variant);
        ret.parse_inner(attrs)?;
        Ok(ret)
    }

    pub fn parse_field(attrs: &[Attribute], _parent_attrs: &Self) -> Result<Self> {
        let mut ret = Self::new(AttrType::Field { bound_type: None });
        ret.parse_inner(attrs)?;
        Ok(ret)
    }

    fn parse_inner(&mut self, attrs: &[Attribute]) -> Result<()> {
        for attr in attrs {
            let path = path_ident_string(attr.path(), attr)?;
            if path.as_str() != "bitcode" {
                continue; // Ignore all other attributes.
            }

            let nested = attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
            for nested in nested {
                BitcodeAttr::new(&nested)?.apply(self, &nested)?;
            }
        }
        Ok(())
    }
}

fn path_ident_string(path: &Path, spanned: &impl Spanned) -> Result<String> {
    if let Some(path) = path.get_ident() {
        Ok(path.to_string())
    } else {
        err(spanned, "expected ident")
    }
}
