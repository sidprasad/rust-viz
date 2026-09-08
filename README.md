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

Your terminal output is unchanged; one browser viewer opens for the process
and collects every subsequent capture in execution order. Select a capture to
see its expression, source location, thread, and diagram. The layout is
controlled by declarative decorators on your types, and all viewer assets are
bundled for offline use.

```rust
spytial::dbg!(&state); // opens the viewer and adds capture 1
step();
spytial::dbg!(&state); // same viewer, capture 2
```

Set `SPYTIAL_NO_OPEN=1` for a fully headless run. The complete session is
continuously written to a self-contained HTML file in the OS temp directory;
set `SPYTIAL_OUTPUT_PATH=/path/to/session.html` to choose that file.

## Install

```toml
[dependencies]
spytial = "0.1"
serde = { version = "1", features = ["derive"] }
```

Diagrammed types need `Debug`, `Serialize`, and `#[derive(SpytialDecorators)]`.

## Docs

- Guide: <https://sidprasad.github.io/spytial-rust/>
- API reference: <https://docs.rs/spytial>

## License

MIT or Apache-2.0, at your option.
