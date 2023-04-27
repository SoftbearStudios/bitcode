use crate::huffman::huffman;
use crate::{err, error, private};
use proc_macro2::TokenStream;
use quote::quote;
use std::str::FromStr;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Attribute, DataEnum, Expr, Lit, Meta, Path, Result, Token};

#[derive(Copy, Clone, Debug)]
enum BitcodeAttr {
    Encoding(Encoding),
    Frequency(f64),
    WithSerde,
}

impl BitcodeAttr {
    fn new(nested: &Meta, is_hint: bool) -> Result<Self> {
        let path = path_ident_string(nested.path(), &nested)?;
        match path.as_str() {
            _ if is_hint => match nested {
                Meta::Path(p) => {
                    let encoding = match path.as_str() {
                        "fixed" => Encoding::Fixed,
                        "gamma" => Encoding::Gamma,
                        _ => return err(p, "unknown hint"),
                    };
                    Ok(Self::Encoding(encoding))
                }
                Meta::NameValue(name_value) => {
                    let expr = &name_value.value;
                    let expr_lit = match expr {
                        Expr::Lit(expr_lit) => expr_lit,
                        _ => return err(&expr, "expected literal"),
                    };

                    match path.as_str() {
                        "frequency" => {
                            let frequency: f64 = match &expr_lit.lit {
                                Lit::Float(float_lit) => float_lit.base10_parse::<f64>().unwrap(),
                                Lit::Int(int_lit) => int_lit.base10_parse::<f64>().unwrap(),
                                _ => return err(expr_lit, "expected number"),
                            };
                            Ok(Self::Frequency(frequency))
                        }
                        "expected_range" => Ok(BitcodeAttr::Encoding(match &expr_lit.lit {
                            Lit::Str(str_lit) => {
                                let range = str_lit.value();
                                parse_expected_range(&range).map_err(|s| error(expr_lit, s))?
                            }
                            _ => return err(expr_lit, "expected str"),
                        })),
                        _ => return err(&name_value, "unknown hint"),
                    }
                }
                _ => err(&nested, "unknown hint"),
            },
            "with_serde" if matches!(nested, Meta::Path(_)) => Ok(Self::WithSerde),
            _ => err(&nested, "unknown attribute"),
        }
    }

    fn apply(&self, attrs: &mut BitcodeAttrs, nested: &Meta) -> Result<()> {
        match *self {
            Self::Encoding(encoding) => {
                if attrs.encoding.is_some() {
                    return err(nested, "duplicate");
                }
                attrs.encoding = Some(encoding);
            }
            Self::Frequency(w) => {
                if let AttrType::Variant { frequency, .. } = &mut attrs.attr_type {
                    if frequency.is_some() {
                        return err(nested, "duplicate");
                    }
                    *frequency = Some(w);
                } else {
                    return err(nested, "can only apply frequency to enum variants");
                }
            }
            Self::WithSerde => {
                if attrs.with_serde {
                    return err(nested, "duplicate");
                }
                attrs.with_serde = true;
            }
        }
        Ok(())
    }
}

#[derive(Copy, Clone, Debug)]
enum Encoding {
    Fixed,
    ExpectNormalizedFloat,
    ExpectedRangeU64 { min: u64, max: u64 },
    Gamma,
}

impl Encoding {
    fn to_tokens(&self) -> TokenStream {
        let private = private();
        match self {
            Self::Fixed => quote! { #private::Fixed },
            Self::ExpectNormalizedFloat => quote! { #private::ExpectNormalizedFloat },
            Self::ExpectedRangeU64 { min, max } => {
                quote! {
                    #private::ExpectedRangeU64::<#min, #max>
                }
            }
            Self::Gamma => quote! { #private::Gamma },
        }
    }
}

#[derive(Clone, Default)]
pub struct BitcodeAttrs {
    attr_type: AttrType,
    encoding: Option<Encoding>,
    with_serde: bool,
}

#[derive(Clone, Default)]
enum AttrType {
    #[default]
    Derive,
    Variant {
        derive_attrs: Box<BitcodeAttrs>,
        frequency: Option<f64>,
    },
    Field {
        parent_attrs: Box<BitcodeAttrs>,
    },
}

impl BitcodeAttrs {
    fn parent(&self) -> Option<&Self> {
        match &self.attr_type {
            AttrType::Derive => None,
            AttrType::Variant { derive_attrs, .. } => Some(derive_attrs),
            AttrType::Field { parent_attrs, .. } => Some(parent_attrs),
        }
    }

    pub fn with_serde(&self) -> bool {
        if self.with_serde {
            return true;
        }
        if let Some(parent) = self.parent() {
            parent.with_serde()
        } else {
            false
        }
    }

    // Gets the most specific encoding. For example field encoding overrides variant encoding which
    // overrides enum encoding.
    fn most_specific_encoding(&self) -> Option<Encoding> {
        self.encoding
            .or_else(|| self.parent().and_then(|p| p.most_specific_encoding()))
    }

    pub fn get_encoding(&self) -> TokenStream {
        let encoding = self.most_specific_encoding();
        if let Some(e) = encoding {
            let encoding = e.to_tokens();
            quote! { #encoding }
        } else {
            quote! { encoding }
        }
    }

    pub fn parse_derive(attrs: &[Attribute]) -> Result<Self> {
        let mut ret = Self::default();
        ret.parse_inner(attrs)?;
        Ok(ret)
    }

    pub fn parse_variant(attrs: &[Attribute], derive_attrs: &Self) -> Result<Self> {
        let mut ret = Self {
            attr_type: AttrType::Variant {
                derive_attrs: Box::new(derive_attrs.clone()),
                frequency: None,
            },
            ..Default::default()
        };
        ret.parse_inner(attrs)?;
        Ok(ret)
    }

    /// `parent_attrs` is either derive or variant attrs.
    pub fn parse_field(attrs: &[Attribute], parent_attrs: &Self) -> Result<Self> {
        let mut ret = Self {
            attr_type: AttrType::Field {
                parent_attrs: Box::new(parent_attrs.clone()),
            },
            ..Default::default()
        };
        ret.parse_inner(attrs)?;
        Ok(ret)
    }

    fn parse_inner(&mut self, attrs: &[Attribute]) -> Result<()> {
        for attr in attrs {
            let path = path_ident_string(attr.path(), attr)?;
            let is_hint = match path.as_str() {
                "bitcode" => false,
                "bitcode_hint" => true,
                _ => continue, // Ignore all other attributes.
            };

            let nested = attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
            for nested in nested {
                BitcodeAttr::new(&nested, is_hint)?.apply(self, &nested)?;
            }
        }
        Ok(())
    }
}

#[derive(Copy, Clone, Debug)]
pub struct PrefixCode {
    pub value: u32,
    pub bits: usize,
}

impl PrefixCode {
    fn format_code(&self) -> TokenStream {
        // TODO leading zeros up to bits.
        let binary = format!("{:#b}", self.value);
        TokenStream::from_str(&binary).unwrap()
    }

    fn format_mask(&self) -> TokenStream {
        let mask = (1u64 << self.bits) - 1;
        let binary = format!("{:#b}", mask);
        TokenStream::from_str(&binary).unwrap()
    }
}

pub struct VariantEncoding {
    variant_count: u32,
    codes: Option<Vec<PrefixCode>>,
}

impl VariantEncoding {
    pub fn parse_data_enum(data_enum: &DataEnum, attrs: &BitcodeAttrs) -> Result<Self> {
        let variant_count = data_enum.variants.len() as u32;

        let codes = if variant_count >= 2 {
            let frequencies: Result<Vec<_>> = data_enum
                .variants
                .iter()
                .map(|variant| {
                    if let AttrType::Variant { frequency, .. } =
                        BitcodeAttrs::parse_variant(&variant.attrs, &attrs)?.attr_type
                    {
                        Ok(frequency.unwrap_or(1.0))
                    } else {
                        unreachable!()
                    }
                })
                .collect();

            let frequencies = frequencies?;

            Some(huffman(&frequencies, 32))
        } else {
            None
        };

        Ok(Self {
            variant_count,
            codes,
        })
    }

    fn iter_codes(&self) -> impl Iterator<Item = (usize, &PrefixCode)> + '_ {
        self.codes.as_ref().unwrap().iter().enumerate()
    }

    pub fn encode_variants(
        &self,
        mut encode: impl FnMut(usize, TokenStream) -> Result<TokenStream>,
    ) -> Result<TokenStream> {
        // if variant_count is 0 or 1 no encoding is required.
        Ok(match self.variant_count {
            0 => quote! {},
            1 => {
                let encode_variant = encode(0, quote! {})?;
                quote! {
                    match self {
                        #encode_variant
                    }
                }
            }
            _ => {
                let variants: Result<TokenStream> = self
                    .iter_codes()
                    .map(|(i, prefix_code)| {
                        let code = prefix_code.format_code();
                        let bits = prefix_code.bits;

                        encode(
                            i,
                            quote! {
                                writer.write_bits(#code, #bits);
                            },
                        )
                    })
                    .collect();
                let variants = variants?;

                quote! {
                    match self {
                        #variants
                    }
                }
            }
        })
    }

    pub fn decode_variants(
        &self,
        mut decode: impl FnMut(usize, TokenStream) -> Result<TokenStream>,
    ) -> Result<TokenStream> {
        // if variant_count is 0 or 1 no encoding is required.
        Ok(match self.variant_count {
            0 => {
                let private = private();
                quote! {
                    Err(#private::invalid_variant())
                }
            }
            1 => {
                let decode_variant = decode(0, quote! {})?;
                quote! {
                    Ok({#decode_variant})
                }
            }
            _ => {
                let variants: Result<TokenStream> = self
                    .iter_codes()
                    .map(|(i, prefix_code)| {
                        let mask = prefix_code.format_mask();
                        let code = prefix_code.format_code();
                        let bits = prefix_code.bits;
                        let decode_variant = decode(i, quote! {})?;

                        Ok(quote! {
                            b if b & #mask == #code => {
                                reader.advance(#bits)?;
                                #decode_variant
                            }
                        })
                    })
                    .collect();
                let variants = variants?;

                quote! {
                    Ok(match reader.peek_bits()? {
                        #variants,
                        _ => unreachable!(),
                    })
                }
            }
        })
    }
}

fn path_ident_string(path: &Path, spanned: &impl Spanned) -> Result<String> {
    if let Some(path) = path.get_ident() {
        Ok(path.to_string())
    } else {
        err(spanned, "expected ident")
    }
}

type Result2<T> = std::result::Result<T, &'static str>;

fn parse_expected_range(range: &str) -> Result2<Encoding> {
    range
        .split_once("..")
        .and_then(|(min, max)| {
            parse_expected_range_u64(min, max)
                .or_else(|| parse_expected_range_i64(min, max))
                .or_else(|| parse_expected_range_f64(min, max))
        })
        .unwrap_or(Err("not a range, e.g. 0..1"))
}

fn parse_expected_range_u64(min: &str, max: &str) -> Option<Result2<Encoding>> {
    let min = u64::from_str(min).ok()?;
    let max = u64::from_str(max).ok()?;
    Some(if min >= max {
        Err("the lower bound must be less than the higher bound")
    } else {
        Ok(Encoding::ExpectedRangeU64 { min, max })
    })
}

fn parse_expected_range_i64(min: &str, max: &str) -> Option<Result2<Encoding>> {
    let min = i64::from_str(min).ok()?;
    let max = i64::from_str(max).ok()?;
    Some(if min >= max {
        Err("the lower bound must be less than the higher bound")
    } else {
        Err("signed integer ranges are not yet supported")
    })
}

fn parse_expected_range_f64(min: &str, max: &str) -> Option<Result2<Encoding>> {
    let either_int = i64::from_str(min).is_ok() || i64::from_str(max).is_ok();

    let min = f64::from_str(min).ok()?;
    let max = f64::from_str(max).ok()?;

    Some(if either_int {
        Err("both bounds must be floats or ints")
    } else if min >= max {
        Err("the start bound must be less than the end bound")
    } else if (min..max) != (0.0..1.0) {
        Err("float ranges other than 0.0..1.0 are not yet supported")
    } else {
        Ok(Encoding::ExpectNormalizedFloat)
    })
}
