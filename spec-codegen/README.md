# spec-codegen

Generates [`macros/src/spec_tables.rs`](../macros/src/spec_tables.rs) from
[`templates/vendor/spytial-language.json`](../templates/vendor/spytial-language.json),
spytial-core's machine-readable description of the layout-spec language.

```bash
cargo run --manifest-path spec-codegen/Cargo.toml
```

Run that after bumping the vendored spytial-core version, and commit the
regenerated tables. `cargo test --manifest-path spec-codegen/Cargo.toml` fails
if the checked-in file is stale, and CI runs it.

## Why

`#[derive(SpytialDecorators)]` has to know which keys each attribute accepts and
which values are legal, because spytial-core mostly *doesn't* complain: a leaf
outside a closed vocabulary is dropped silently and the diagram just renders
wrong. The macro is the only place a typo can fail loudly, so its tables have to
be right.

Transcribing them by hand did not hold up. When this crate was written, the
derive macro had drifted from the engine in five ways — including a
`#[projection(sig = ...)]` attribute that *no* released spytial-core has ever
had a parser for, and a bare `#[flag]` that defaulted to `important`, which is
not one of the two values the engine recognizes.

## What is and isn't generated

Generated, because the manifest states it outright: the key list for every
attribute, closed vocabularies, numeric bounds, required-ness, `orientation`'s
mutually-exclusive direction rules, the style blocks and their leaves, and which
forms spytial-core has deprecated.

Hand-written, in `macros/src/lib.rs`, because the manifest has no opinion about
it: the literal-aware token scanner, the `negated` ⇄ `hold: never` ergonomics,
legacy-form desugaring, the `DecoProbe` field walk, and the mapping onto
`SpytialDecoratorsBuilder` methods.

The seam between them is the policy table at the top of [`src/lib.rs`](src/lib.rs)
— which Rust attribute is authored from which manifest item, how a YAML field is
spelled in Rust, and which fields are deliberately not exposed. That table is
the hand-maintained part, and it is deliberately in one place.

## Layout

Its own workspace root, like `eval-corpus`. Neither `spytial` nor
`spytial_export_macros` may depend on it, so building or packaging them never
needs this directory. The generated tables are checked in rather than produced
by a `build.rs`, for three reasons: `macros/` publishes to crates.io separately
and would otherwise have to ship the manifest and take a `serde_json`
build-dependency; the generated file is reviewable as a diff when upstream
changes; and the published crates stay free of build-time machinery.
