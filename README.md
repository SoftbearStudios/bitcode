# Bitcode

[![Build](https://github.com/SoftbearStudios/bitcode/actions/workflows/build.yml/badge.svg)](https://github.com/SoftbearStudios/bitcode/actions/workflows/build.yml)

A bitwise encoder/decoder similar to [bincode](https://github.com/bincode-org/bincode).

## Motivation

We noticed relatively low entropy in data serialized with [bincode](https://github.com/bincode-org/bincode). This library attempts to shrink the serialized size without sacrificing too much speed (as would be the case with compression).

Bitcode does not attempt to have a stable format, so we are free to optimize it.

## Comparison with [bincode](https://github.com/bincode-org/bincode)

### Features

- Bitwise serialization
- [Gamma](https://en.wikipedia.org/wiki/Elias_gamma_coding) encoded lengths and enum variant indices
- Implemented in 100% safe Rust

### Limitations

- Doesn't support streaming APIs
- Format is unstable between versions
- Currently slow on big endian

## Benchmarks vs. [bincode](https://github.com/bincode-org/bincode) and [postcard](https://github.com/jamesmunns/postcard)

### Speed

Aims to be no more than twice as slow as [bincode](https://github.com/bincode-org/bincode) or [postcard](https://github.com/jamesmunns/postcard).

### Size (bits)

| Type                 | Bitcode Default | Bitcode Full | Bitcode Bounded | Bincode | Bincode (Varint) | Postcard |
|----------------------|-----------------|--------------|-----------------|---------|------------------|----------|
| bool                 | 1               | 1            | 1               | 8       | 8                | 8        |
| u8                   | 8               | 1-17         | 3-9             | 8       | 8                | 8        |
| u16                  | 16              | 1-33         | 3-17            | 16      | 8-24             | 8-24     |
| u32                  | 32              | 1-63         | 3-33            | 32      | 8-40             | 8-40     |
| u64                  | 64              | 1-127        | 3-65            | 64      | 8-72             | 8-80     |
| i8                   | 8               | 1-15         | 3-9             | 8       | 8                | 8        |
| i16                  | 16              | 1-31         | 3-17            | 16      | 8-24             | 8-24     |
| i32                  | 32              | 1-63         | 3-33            | 32      | 8-40             | 8-40     |
| i64                  | 64              | 64           | 64              | 64      | 8-72             | 8-80     |
| f32                  | 32              | 32           | 32              | 32      | 32               | 32       |
| f64                  | 64              | 64           | 64              | 64      | 64               | 64       |
| char                 | 8-32            | 8-32         | 8-32            | 8-32    | 8-32             | 16-40    |
| Option<()>           | 1               | 1            | 1               | 8       | 8                | 8        |
| Result<(), ()>       | 1-3             | 1-3          | 3               | 32      | 8                | 8        |

| Type                 | Bitcode Default | Bitcode Full | Bitcode Bounded | Bincode | Bincode (Varint) | Postcard |
|----------------------|-----------------|--------------|-----------------|---------|------------------|----------|
| [true; 4]            | 4               | 4            | 4               | 32      | 32               | 32       |
| vec![(); 0]          | 1               | 1            | 3               | 64      | 8                | 8        |
| vec![(); 1]          | 3               | 3            | 3               | 64      | 8                | 8        |
| vec![(); 256]        | 17              | 17           | 17              | 64      | 24               | 16       |
| vec![(); 65536]      | 33              | 33           | 33              | 64      | 40               | 24       |
| vec![1234u64; 0]     | 1               | 1            | 3               | 64      | 8                | 8        |
| vec![1234u64; 1]     | 67              | 24           | 24              | 128     | 32               | 24       |
| vec![1234u64; 256]   | 16401           | 5393         | 5393            | 16448   | 6168             | 4112     |
| vec![1234u64; 65536] | 4194337         | 1376289      | 1376289         | 4194368 | 1572904          | 1048600  |
| ""                   | 1               | 1            | 3               | 64      | 8                | 8        |
| "abcd"               | 37              | 37           | 37              | 96      | 40               | 40       |
| "abcd1234"           | 71              | 71           | 71              | 128     | 72               | 72       |

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
| Bitcode                | 6.7          | 0.19%      |
| Bitcode Full           | 8.5          | 13.1%      |
| Bitcode Bounded        | 7.2          | 0.12%      |
| Bincode                | 20.3         | 65.9%      |
| Bincode (Varint)       | 10.9         | 27.7%      |
| Bincode (LZ4)          | 9.9          | 13.9%      |
| Bincode (Deflate Fast) | 8.4          | 0.88%      |
| Bincode (Deflate Best) | 7.8          | 0.29%      |
| Postcard               | 10.7         | 28.3%      |
| ideal (max entropy)    |              | 0.39%      |

### A note on enums

Enum variants are currently encoded such that variants declared
earlier will occupy fewer bits. It is advantageous to sort variant
declarations from most common to least common.

We hope to allow further customization in the future with a custom derive macro.

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