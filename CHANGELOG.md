# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - TBD

Speaks the spytial-core 4.0 directive contract:

- Vendored spytial-core bumped 3.1.0 -> 4.1.0.
- **Breaking** (selectors): 4.1.0's simple-graph-query 3.0 reads a name that
  resolves to nothing as the empty relation rather than as a string, so string
  comparands must be quoted — `@:(x.color) = \"Red\"` inside a Rust selector
  string, or the raw `r#"@:(x.color) = "Red""#`. An unquoted name doesn't
  error; the comparison is simply false, so the rule silently stops applying.
  The bundled example and the decorators doc are migrated. spytial-core
  surfaces such names as a `⚠ n selector warnings` bar on the diagram (also on
  `layout.warnings`, and emitted as a `layout-warnings` event).
- Selector strings may carry quotes either way, escaped or raw: the derive
  macro reads whole string literals of both shapes rather than stopping at the
  first `"` it meets. Previously the escaped form was truncated mid-selector
  and the raw form was dropped entirely, leaving a rule that matched every
  atom. Attribute keys are now also matched on an identifier boundary and only
  outside literals, so `key = "..."` text inside a selector is content.
- New `draw = "<end> -> <end>"` on `#[inferred_edge(...)]` (spytial-core 3.2):
  each end is `_` (the tuple's own atom) or a `group` constraint's name, in
  which case that end attaches to the group's hull — group-to-group and
  node-to-group edges. Malformed forms are compile errors, including the
  redundant `"_ -> _"` (spytial-core silently drops it). New
  `InferredEdgeDraw` / `DrawEnd` types and a
  `SpytialDecoratorsBuilder::inferred_edge_drawn` method; the existing
  `inferred_edge`/`inferred_edge_styled` methods and the attribute form
  without `draw` are unchanged.
- spytial-core 4.0's breaking changes don't reach this crate. They split
  `SQLEvaluator` and `<spytial-explorer>` out of the CDN main global into
  `spytial-core-sql.global.js` / `spytial-core-explorer.global.js`; the
  generated page uses neither, so those scripts are deliberately not vendored
  and every page gets ~0.3 MB smaller. The APIs the template does call —
  `JSONDataInstance`, `SGraphQueryEvaluator`, `parseLayoutSpec`,
  `LayoutInstance`, `<webcola-cnd-graph>` — are all still on the main global,
  and the React error-modal bundle is unchanged.
- `spytial-core.css` drops from 42 KB to 15 KB: the removed rules are all
  spec-editor (`.spytial-ed-*`), which the generated page never renders.
- `JSONDataInstance` now infers missing relation and tuple type signatures.
  The exporter has always emitted fully-specified types, so it takes the same
  untouched fast path as before.

Also in this release (landed after the 0.2.0 tag):

- `i128`/`u128` values now export instead of failing with "i128 is not
  supported", and `from_datum`/`replit` can read back the `bytes` atoms the
  exporter was already emitting. Both turn errors into working output; no
  existing diagram changes.
- The serde data model — all 29 categories a `Serialize` impl can express — is
  now covered by a corpus that round-trips 74 values through both `from_datum`
  and `replit`, so "the whole model survives export" is a checked claim.
- The eval corpus is its own workspace and no longer a dev-dependency of
  `spytial`, so it never enters the crate's build graph. Packaged crate
  contents are unaffected; running it needs its own `cargo test` (see
  CONTRIBUTING).

## [0.2.0] - 2026-07-15

Speaks the spytial-core 3.1 directive contract:

- Vendored spytial-core bumped 2.6.2 -> 3.1.0.
- Styling uses the 3.x block system, written as nested attribute groups that
  mirror the YAML 1:1: `line_style(...)`, `text_style(...)`,
  `border_style(...)`, `fill_style(...)`; new `#[atom_style(...)]` attribute
  and `edgeStyle` wire key; `inferred_edge`/`attribute`/`tag` take style
  blocks; selector `group` takes `add_edge` (bare direction or styled block)
  and a label `text_style`.
- Legacy 2.x flat forms still compile and rewrite onto the blocks:
  `#[atom_color(selector, value)]` -> `atomStyle` with `value` as the border
  colour; `#[edge_style(field, value, style, weight, ...)]` ->
  `lineStyle{color, pattern, weight}`.
- Pattern/size/direction typos and non-positive weights are compile errors
  (spytial-core silently drops invalid leaves, so the macro is the gate).
- **Breaking** (Rust API): `Directive::AtomColor` is replaced by
  `Directive::AtomStyle`, and `EdgeStyleParams` carries `line_style` /
  `text_style` blocks instead of the flat `value`/`style`/`weight` fields.
  The attribute and builder authoring forms remain source-compatible.
- Note: spytial-core 3.0 raises `StyleCollisionError` when two rules set the
  same style property of the same edge/atom to different values (2.x silently
  kept the first).

## [0.1.0]

First public release.

Note: 0.0.1 was an internal pre-release; not on crates.io.
