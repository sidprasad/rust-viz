# Spytial

[![crates.io](https://img.shields.io/crates/v/spytial.svg)](https://crates.io/crates/spytial)
[![docs.rs](https://img.shields.io/docsrs/spytial)](https://docs.rs/spytial)
[![License](https://img.shields.io/crates/l/spytial.svg)](#license)

A drop-in replacement for `std::dbg!` that opens an interactive diagram of a
Rust value in your browser instead of printing nested text:

```diff
- std::dbg!(tree)
+ spytial::dbg!(tree)
```

Your terminal output is unchanged; a browser tab also opens with a structural
diagram of the value. Optional declarative decorators refine its layout and
styling.

## Install

```toml
[dependencies]
spytial = "0.1"
serde = { version = "1", features = ["derive"] }
```

`spytial::dbg!` accepts any `Debug + Serialize` value, including direct
standard-library collections:

```rust
spytial::dbg!(&vec![1, 2, 3]);
```

`diagram(&value)` only needs `Serialize`. Add `#[derive(SpytialDecorators)]`
when you want type-specific layout or styling; it is an enrichment, not a
requirement.

## Docs

- Guide: <https://sidprasad.github.io/spytial-rust/>
- API reference: <https://docs.rs/spytial>

## License

MIT or Apache-2.0, at your option.
