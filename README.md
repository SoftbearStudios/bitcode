# Bitcode

[![Build](https://github.com/SoftbearStudios/bitcode/actions/workflows/build.yml/badge.svg)](https://github.com/SoftbearStudios/bitcode/actions/workflows/build.yml)
[![Documentation](https://docs.rs/bitcode/badge.svg)](https://docs.rs/bitcode)

A bitwise encoder/decoder similar to [bincode](https://github.com/bincode-org/bincode).

## Motivation

We noticed relatively low entropy in data serialized with [bincode](https://github.com/bincode-org/bincode). This library attempts to shrink the serialized size without sacrificing too much speed (as would be the case with compression).

Bitcode does not attempt to have a stable format, so we are free to optimize it.

## Comparison with [bincode](https://github.com/bincode-org/bincode)

### Features (serde)

- Bitwise serialization
- [Gamma](https://en.wikipedia.org/wiki/Elias_gamma_coding) encoded lengths and enum variant indices
- Implemented in 100% safe Rust

### Additional features with `#[derive(bitcode::Encode, bitcode::Decode)]`

- Enums use the fewest possible bits, e.g. an enum with 4 variants uses 2 bits
- Specify frequency of enum variants with `#[bincode_hint(frequency = 123)` to use [Huffman](https://en.wikipedia.org/wiki/Huffman_coding) coding
- Specify expected range of integers with `#[bitcode_hint(expected_range = "50..100"]`
- Opt into [Gamma](https://en.wikipedia.org/wiki/Elias_gamma_coding) encoded integers with `#[bitcode_hint(gamma)]`
- Fall back to serde on specific fields with `#[bitcode(with_serde)]`

### Limitations

- Doesn't support streaming APIs
- Format is unstable between versions
- When using `#[feature = "derive"]` structs/enums that are recursive must be labeled with `#[bitcode(recursive)]` or you will get a compile error

## Benchmarks vs. [bincode](https://github.com/bincode-org/bincode) and [postcard](https://github.com/jamesmunns/postcard)

### Speed

Aims to be no more than twice as slow as [bincode](https://github.com/bincode-org/bincode) or [postcard](https://github.com/jamesmunns/postcard).
See [rust serialization benchmark](https://github.com/djkoloski/rust_serialization_benchmark) for benchmarks.

### Size (bits)

| Type                | Bitcode (derive) | Bitcode (serde) | Bincode | Bincode (varint) | Postcard |
|---------------------|------------------|-----------------|---------|------------------|----------|
| bool                | 1                | 1               | 8       | 8                | 8        |
| u8                  | 8                | 8               | 8       | 8                | 8        |
| i8                  | 8                | 8               | 8       | 8                | 8        |
| u16                 | 16               | 16              | 16      | 8-24             | 8-24     |
| i16                 | 16               | 16              | 16      | 8-24             | 8-24     |
| u32                 | 32               | 32              | 32      | 8-40             | 8-40     |
| i32                 | 32               | 32              | 32      | 8-40             | 8-40     |
| u64                 | 64               | 64              | 64      | 8-72             | 8-80     |
| i64                 | 64               | 64              | 64      | 8-72             | 8-80     |
| f32                 | 32               | 32              | 32      | 32               | 32       |
| f64                 | 64               | 64              | 64      | 64               | 64       |
| char                | 8-32             | 8-32            | 8-32    | 8-32             | 16-40    |
| Option<()>          | 1                | 1               | 8       | 8                | 8        |
| Result<(), ()>      | 1                | 1-3             | 32      | 8                | 8        |
| enum { A, B, C, D } | 2                | 1-5             | 32      | 8                | 8        |

| Value               | Bitcode (derive) | Bitcode (serde) | Bincode | Bincode (varint) | Postcard |
|---------------------|------------------|-----------------|---------|------------------|----------|
| [true; 4]           | 4                | 4               | 32      | 32               | 32       |
| vec![(); 0]         | 1                | 1               | 64      | 8                | 8        |
| vec![(); 1]         | 3                | 3               | 64      | 8                | 8        |
| vec![(); 256]       | 17               | 17              | 64      | 24               | 16       |
| vec![(); 65536]     | 33               | 33              | 64      | 40               | 24       |
| ""                  | 1                | 1               | 64      | 8                | 8        |
| "abcd"              | 37               | 37              | 96      | 40               | 40       |
| "abcd1234"          | 71               | 71              | 128     | 72               | 72       |

### Random Struct Benchmark

The following data structure was used for benchmarking.
```rust
struct Data {
    x: Option<f32>,
    y: Option<i8>,
    z: u16,
    s: String,
    e: DataEnum,
}

enum DataEnum {
    Bar,
    Baz(String),
    Foo(Option<u8>),
}
```
In the table below, **Size (bytes)** is the average size of a randomly generated `Data` struct.
**Zero Bytes** are the percentage of bytes that are 0 in the output.
If the result contains a large percentage of zero bytes, that is a sign that it could be compressed more.

| Format                 | Size (bytes) | Zero Bytes |
|------------------------|--------------|------------|
| Bitcode (derive)       | 6.5          | 0.28%      |
| Bitcode (serde)        | 6.7          | 0.19%      |
| Bincode                | 20.3         | 65.9%      |
| Bincode (varint)       | 10.9         | 27.7%      |
| Bincode (LZ4)          | 9.9          | 13.9%      |
| Bincode (Deflate Fast) | 8.4          | 0.88%      |
| Bincode (Deflate Best) | 7.8          | 0.29%      |
| Postcard               | 10.7         | 28.3%      |
| ideal (max entropy)    |              | 0.39%      |

### A note on enums

When using serde to serialize enums. Enum variants are encoded such that variants declared earlier will occupy fewer
bits. It is advantageous to sort variant declarations from most common to least common.

This limitation can be avoided by using bitcode's derive macros.

## Testing

### Fuzzing

```
cargo install cargo-fuzz
cargo fuzz run fuzz
```

### 32-bit

```
sudo apt install gcc-multilib
rustup target add i686-unknown-linux-gnu
cargo test --target i686-unknown-linux-gnu
```

## Acknowledgement

Some test cases were derived from [bincode](https://github.com/bincode-org/bincode) (see comment in `tests.rs`).

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.