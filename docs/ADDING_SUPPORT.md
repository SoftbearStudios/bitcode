# Introduction

This document outlines the process for adding support for 3rd party crates, such as `glam` or `arrayvec`.

## Process

1. Find a library you want `bitcode` to have support for.
2. If the library doesn't support encoding crates except possibly `serde`, skip to step 6.
3. If `#[derive(Encode, Decode)]` is bad, skip to step 6.
  - It may be bad for speed, because it's possible that the internal structure of a 3rd party type doesn't match [`bitcode` principles](https://github.com/SoftbearStudios/bitcode?tab=readme-ov-file#tuple-vs-array).
  - It may be bad for size, because it's possible that 3rd party types can be converted to a more efficient form.
  - It may be bad for security, because it's possible that 3rd party types have invariants to maintain.
4. Submit a PR to the library, following [these instructions](#library-example).
5. If the PR is accepted, you're done! otherwise, continue to step 6.
6. Submit an issue to `bitcode` for thoughts and suggestions, unless you're completely sure what you're doing. If we agree with adding support, we may decide to implement support ourselves or ask you to continue to step 7.
7. Submit a PR to `bitcode`.
8. We will review the PR.
  - We will attempt to optimize it as much as possible for size and speed.
  - If the library is not very popular (e.g. relative to already-supported libraries), we might put the PR on hold.
  - If the PR requires too much additional `unsafe` code, we might put the PR on hold.
9. If all goes well, we will merge the PR it into `bitcode`.


## Library Example

Add bitcode to libraries without specifying the major version so binary crates can pick the version.
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