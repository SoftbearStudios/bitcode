#[cfg(feature = "arrayvec")]
mod arrayvec;
#[cfg(feature = "glam")]
#[rustfmt::skip] // Makes impl_struct! calls way longer.
mod glam;

#[allow(unused)]
macro_rules! impl_struct {
    ($t:ident, $new:ident, $($f:ident, $ft:ty),+) => {
        const _: () = {
            #[derive(Default)]
            pub struct StructEncoder {
                $(
                    $f: <$ft as crate::Encode>::Encoder,
                )+
            }
            impl crate::coder::Encoder<$t> for StructEncoder {
                #[inline(always)]
                fn encode(&mut self, t: &$t) {
                    $(
                        self.$f.encode(&t.$f);
                    )+
                }
            }
            impl crate::coder::Buffer for StructEncoder {
                fn collect_into(&mut self, out: &mut Vec<u8>) {
                    $(
                        self.$f.collect_into(out);
                    )+
                }

                fn reserve(&mut self, additional: std::num::NonZeroUsize) {
                    $(
                        self.$f.reserve(additional);
                    )+
                }
            }
            impl crate::Encode for $t {
                type Encoder = StructEncoder;
            }

            #[derive(Default)]
            pub struct StructDecoder<'a> {
                $(
                    $f: <$ft as crate::Decode<'a>>::Decoder,
                )+
            }
            impl<'a> crate::coder::View<'a> for StructDecoder<'a> {
                fn populate(&mut self, input: &mut &'a [u8], length: usize) -> crate::coder::Result<()> {
                    $(
                        self.$f.populate(input, length)?;
                    )+
                    Ok(())
                }
            }
            impl<'a> crate::coder::Decoder<'a, $t> for StructDecoder<'a> {
                #[inline(always)]
                fn decode(&mut self) -> $t {
                    $t::$new($(self.$f.decode()),+)
                }
            }
            impl<'a> crate::Decode<'a> for $t {
                type Decoder = StructDecoder<'a>;
            }
        };
    }
}
#[allow(unused)]
pub(crate) use impl_struct;
