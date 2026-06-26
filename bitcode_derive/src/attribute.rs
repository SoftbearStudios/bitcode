use crate::{err, error};
use proc_macro2::TokenStream;
use std::str::FromStr;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{parse2, Attribute, Expr, ExprLit, Lit, Meta, Path, Result, Token, Type};

enum BitcodeAttr {
    BoundType(Type),
    CrateAlias(Path),
    Skip,
    /// `#[bitcode(with = "LocalType")]`, shorthand for both `encode_with` and `decode_with`.
    With(Type),
    /// `#[bitcode(encode_with = "LocalType")]`.
    EncodeWith(Type),
    /// `#[bitcode(decode_with = "LocalType")]`.
    DecodeWith(Type),
}

/// Parses a `#[bitcode(name = "Type")]` string literal value into a [`Type`].
fn parse_type_attr(nested: &Meta) -> Result<Type> {
    match nested {
        Meta::NameValue(name_value) => {
            let expr = &name_value.value;
            let str_lit = match expr {
                Expr::Lit(ExprLit {
                    lit: Lit::Str(v), ..
                }) => v,
                _ => return err(&expr, "expected string e.g. \"LocalType\""),
            };
            let value = TokenStream::from_str(&str_lit.value()).unwrap();
            parse2(value).map_err(|e| error(str_lit, &format!("{e}")))
        }
        _ => err(&nested, "expected name value"),
    }
}

impl BitcodeAttr {
    fn new(nested: &Meta) -> Result<Self> {
        let path = path_ident_string(nested.path(), &nested)?;
        match path.as_str() {
            "bound_type" => Ok(Self::BoundType(parse_type_attr(nested)?)),
            "with" => Ok(Self::With(parse_type_attr(nested)?)),
            "encode_with" => Ok(Self::EncodeWith(parse_type_attr(nested)?)),
            "decode_with" => Ok(Self::DecodeWith(parse_type_attr(nested)?)),
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

                    Ok(Self::CrateAlias(path))
                }
                _ => err(&nested, "expected name value"),
            },
            "skip" => Ok(Self::Skip),
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
            Self::CrateAlias(crate_name) => {
                if let AttrType::Derive = attrs.attr_type {
                    attrs.crate_name = crate_name;
                    Ok(())
                } else {
                    err(nested, "can only apply crate rename to derives")
                }
            }
            Self::Skip => {
                if let AttrType::Field { .. } = &attrs.attr_type {
                    attrs.skip = true;
                    Ok(())
                } else {
                    err(nested, "can only apply skip to fields")
                }
            }
            Self::With(ty) => {
                Self::set_with(attrs, nested, ty.clone(), true)?;
                Self::set_with(attrs, nested, ty, false)
            }
            Self::EncodeWith(ty) => Self::set_with(attrs, nested, ty, true),
            Self::DecodeWith(ty) => Self::set_with(attrs, nested, ty, false),
        }
    }

    /// Sets `encode_with` (`encode == true`) or `decode_with` (`encode == false`) on a field.
    fn set_with(attrs: &mut BitcodeAttrs, nested: &Meta, ty: Type, encode: bool) -> Result<()> {
        if let AttrType::Field {
            encode_with,
            decode_with,
            ..
        } = &mut attrs.attr_type
        {
            let slot = if encode { encode_with } else { decode_with };
            if slot.is_some() {
                return err(nested, "duplicate with/encode_with/decode_with");
            }
            *slot = Some(ty);
            Ok(())
        } else {
            err(
                nested,
                "can only apply with/encode_with/decode_with to fields",
            )
        }
    }
}

#[derive(Clone)]
pub struct BitcodeAttrs {
    attr_type: AttrType,
    /// The crate name to use for the generated code, defaults to "bitcode".
    pub crate_name: Path,
    /// Whether to skip this field during (de)serialisation.
    pub skip: bool,
}

#[derive(Clone)]
enum AttrType {
    Derive,
    Variant,
    Field {
        bound_type: Option<Type>,
        /// Encode this field as a different (local) type via `From`/`Into`.
        encode_with: Option<Type>,
        /// Decode this field as a different (local) type via `From`/`Into`.
        decode_with: Option<Type>,
    },
}

impl BitcodeAttrs {
    fn new(attr_type: AttrType) -> Self {
        Self {
            attr_type,
            crate_name: syn::parse_str("bitcode").expect("invalid crate name"),
            skip: false,
        }
    }

    pub fn bound_type(&self) -> Option<Type> {
        match &self.attr_type {
            AttrType::Field { bound_type, .. } => bound_type.as_ref().cloned(),
            _ => unreachable!(),
        }
    }

    /// The local type to encode this field as, if `#[bitcode(with)]`/`#[bitcode(encode_with)]` was set.
    pub fn encode_with(&self) -> Option<&Type> {
        match &self.attr_type {
            AttrType::Field { encode_with, .. } => encode_with.as_ref(),
            _ => None,
        }
    }

    /// The local type to decode this field as, if `#[bitcode(with)]`/`#[bitcode(decode_with)]` was set.
    pub fn decode_with(&self) -> Option<&Type> {
        match &self.attr_type {
            AttrType::Field { decode_with, .. } => decode_with.as_ref(),
            _ => None,
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
        let mut ret = Self::new(AttrType::Field {
            bound_type: None,
            encode_with: None,
            decode_with: None,
        });
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
