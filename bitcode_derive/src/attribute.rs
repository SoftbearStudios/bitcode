use crate::{err, error};
use proc_macro2::TokenStream;
use quote::quote;
use std::str::FromStr;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{parse2, Attribute, Expr, ExprLit, Lit, Meta, Path, Result, Token, Type};

enum BitcodeAttr {
    BoundType(Type),
    CrateName(Path),
    Skip,
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
            "crate" => match nested {
                Meta::NameValue(name_value) => {
                    let expr = &name_value.value;
                    let str_lit = match expr {
                        Expr::Lit(ExprLit {
                            lit: Lit::Str(v), ..
                        }) => v,
                        _ => return err(&expr, "expected path string e.g. \"my_crate::bitcode\""),
                    };

                    let path = syn::parse_str::<Path>(&str_lit.value())
                        .map_err(|e| error(str_lit, &e.to_string()))?;

                    // previus: ensure there's a leading `::`
                    // removed: https://github.com/SoftbearStudios/bitcode/pull/28#issuecomment-2227465515
                    // path.leading_colon = Some(Token![::](str_lit.span()));

                    Ok(Self::CrateName(path))
                }
                _ => err(&nested, "expected name value"),
            },
            "skip" => Ok(Self::Skip),
            _ => err(&nested, "unknown attribute"),
        }
    }

    fn apply(self, attrs: &mut BitcodeAnyAttrs<'_, '_>, nested: &Meta) -> Result<()> {
        fn set_if_not_duplicate<T: Default + PartialEq>(
            v: &mut T,
            new_value: T,
            nested: &Meta,
        ) -> Result<()> {
            if new_value == Default::default() {
                return err(nested, "cannot set to default value");
            }
            if &*v != &Default::default() {
                return err(nested, "duplicate");
            }
            *v = new_value;
            Ok(())
        }

        match self {
            Self::BoundType(bound_type) => {
                if let BitcodeAnyAttrs::Field(field) = attrs {
                    set_if_not_duplicate(&mut field.bound_type, Some(bound_type), nested)
                } else {
                    err(nested, r#"can only apply to fields"#)
                }
            }
            Self::CrateName(crate_name) => {
                if let BitcodeAnyAttrs::Derive(derive) = attrs {
                    set_if_not_duplicate(&mut derive.crate_name, Some(crate_name), nested)
                } else {
                    err(nested, r#"can only apply to struct/enum definition"#)
                }
            }
            Self::Skip => {
                if let BitcodeAnyAttrs::Field(field) = attrs {
                    set_if_not_duplicate(&mut field.skip, true, nested)
                } else {
                    err(nested, "can only apply to fields")
                }
            }
        }
    }
}

pub struct BitcodeDeriveAttrs {
    crate_name: Option<Path>,
    pub private: TokenStream,
}
impl BitcodeDeriveAttrs {
    pub fn parse(attrs: &[Attribute]) -> Result<Self> {
        let mut ret = Self {
            crate_name: Default::default(),
            private: quote! {},
        };
        BitcodeAnyAttrs::Derive(&mut ret).parse_inner(attrs)?;
        let crate_name = ret
            .crate_name
            .clone()
            .unwrap_or_else(|| syn::parse_str("bitcode").unwrap());

        ret.private = quote! { #crate_name::__private };
        Ok(ret)
    }
}

pub struct BitcodeVariantAttrs<'a> {
    parent: &'a BitcodeDeriveAttrs,
}
impl std::ops::Deref for BitcodeVariantAttrs<'_> {
    type Target = BitcodeDeriveAttrs;
    fn deref(&self) -> &Self::Target {
        self.parent
    }
}

impl<'a> BitcodeVariantAttrs<'a> {
    pub fn parse(attrs: &[Attribute], parent: &'a BitcodeDeriveAttrs) -> Result<Self> {
        let mut ret = Self { parent };
        BitcodeAnyAttrs::Variant { _unused: &mut ret }.parse_inner(attrs)?;
        Ok(ret)
    }
}

#[derive(Copy, Clone)]
pub enum BitcodeDeriveOrVariantAttrs<'a> {
    Derive(&'a BitcodeDeriveAttrs),
    Variant(&'a BitcodeVariantAttrs<'a>),
}
impl std::ops::Deref for BitcodeDeriveOrVariantAttrs<'_> {
    type Target = BitcodeDeriveAttrs;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Derive(v) => v,
            Self::Variant(v) => v.parent,
        }
    }
}

pub struct BitcodeFieldAttrs<'a> {
    parent: BitcodeDeriveOrVariantAttrs<'a>,
    pub bound_type: Option<Type>,
    pub skip: bool,
}
impl<'a> std::ops::Deref for BitcodeFieldAttrs<'a> {
    type Target = BitcodeDeriveOrVariantAttrs<'a>;
    fn deref(&self) -> &Self::Target {
        &self.parent
    }
}
impl<'a> BitcodeFieldAttrs<'a> {
    pub fn parse(attrs: &[Attribute], parent: BitcodeDeriveOrVariantAttrs<'a>) -> Result<Self> {
        let mut ret = Self {
            parent,
            bound_type: Default::default(),
            skip: Default::default(),
        };
        BitcodeAnyAttrs::Field(&mut ret).parse_inner(attrs)?;
        Ok(ret)
    }
}

enum BitcodeAnyAttrs<'a, 'b> {
    Derive(&'a mut BitcodeDeriveAttrs),
    Variant {
        _unused: &'a mut BitcodeVariantAttrs<'b>,
    },
    Field(&'a mut BitcodeFieldAttrs<'b>),
}
impl BitcodeAnyAttrs<'_, '_> {
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
