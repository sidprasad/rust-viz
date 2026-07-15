# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - TBD

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
