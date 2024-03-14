# Bitcode
[![Documentation](https://docs.rs/bitcode/badge.svg)](https://docs.rs/bitcode)
[![crates.io](https://img.shields.io/crates/v/bitcode.svg)](https://crates.io/crates/bitcode)
[![Build](https://github.com/SoftbearStudios/bitcode/actions/workflows/build.yml/badge.svg)](https://github.com/SoftbearStudios/bitcode/actions/workflows/build.yml)

A binary encoder/decoder with the following goals:
- 🔥 Blazingly fast
- 🐁 Tiny serialized size
- 💎 Highly compressible by Deflate/LZ4/Zstd

In contrast, these are non-goals:
- Stable format across major versions
- Self describing format
- Compatibility with languages other than Rust

See [rust_serialization_benchmark](https://github.com/djkoloski/rust_serialization_benchmark) for benchmarks.

## Example
```rust
use bitcode::{Encode, Decode};

#[derive(Encode, Decode, PartialEq, Debug)]
struct Foo<'a> {
    x: u32,
    y: &'a str,
}

let original = Foo {
    x: 10,
    y: "abc",
};

let encoded: Vec<u8> = bitcode::encode(&original); // No error
let decoded: Foo<'_> = bitcode::decode(&encoded).unwrap();
assert_eq!(original, decoded);
```

## Library Example

Add bitcode to libraries without specifying the major version so binary crates can use any version.
This is a minimal stable subset of the bitcode API so avoid using any other functionality.
```toml
bitcode = { version = "0", features = ["derive"], default-features = false, optional = true }
```
```rust
#[cfg_attr(feature = "bitcode", derive(bitcode::Encode, bitcode::Decode))]
pub struct Vec2 {
    x: f32,
    y: f32,
}
```

## Tuple vs Array
If you have multiple values of the same type:
- Use a tuple or struct when the values are semantically different: `x: u32, y: u32`
- Use an array when all values are semantically similar: `pixels: [u8; 16]`

## Implementation Details
- Heavily inspired by <https://github.com/That3Percent/tree-buf>
- All instances of each field are grouped together making compression easier
- Uses smaller integers where possible all the way down to 1 bit
- Validation is performed up front on typed vectors before deserialization
- Code is designed to be auto-vectorized by LLVM

## License
Licensed under either of
* Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
* MIT license
  ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Contribution
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
