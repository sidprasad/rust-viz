# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- Changed: `spytial::dbg!` now accepts any `Debug + Serialize` value and
  `diagram()` accepts any `Serialize` value; `SpytialDecorators` is optional
  enrichment rather than a gate (#91). Derives submit a qualified Rust type
  identity and their rules to a link-time registry, so existing decorated
  types still apply root and transitive nested decorators automatically while
  direct `Vec`, `HashMap`, third-party, and undecorated user values render with
  the default structural layout. The qualified root is resolved before Serde can
  erase it for `transparent` or `untagged` representations; unqualified Serde
  names are used only when they identify one registration, preventing
  same-named types in different modules from mixing decorators. Serde-renamed
  types are registered under both their Rust and serialized names. The public
  runtime registry continues to report only explicit registrations. The
  runtime remains offline and
  best-effort, and the public `dbg!` evaluation/return/stderr behavior is
  unchanged. Compatibility note: automatic discovery is emitted by the derive;
  a hand-written `HasSpytialDecorators` implementation is no longer sufficient
  by itself for `diagram()` and should pass `T::decorators()` through
  `diagram_with_spec`.

- Fixed: a relation's type signature is no longer frozen by whichever tuple
  arrived first (#79). Relations are keyed by name in one flat namespace, so
  two structs with a same-named field share one relation; its header `types`
  is now the position-wise join of every tuple's types — positions all tuples
  agree on keep their concrete type, positions that vary widen to `"atom"`.
  The join is order-independent. When a user field shares its name with a
  built-in relation of different arity (a field literally named `idx` or
  `map_entry`), the header joins the common prefix and keeps the longest
  arity seen, and the relation's tuples are ordered longest-first — vendored
  spytial-core's normalizer keeps a header only when its length matches the
  first tuple's arity, so without the ordering the joined header would not
  survive `JSONDataInstance` construction. Per-tuple `ITuple.types` remains
  exact in all cases, and round-trips through `reify` are unaffected. Also
  corrected the `IRelation`
  docs, which showed a concrete target type (`name(Person, string)`) that the
  exporter never emits — the target position is always the literal `"atom"`.

- Added: `tests/conformance.rs`, which tests what a `SpytialDecorators` spec
  *entails* rather than where anything is drawn (#87). Each case hands
  spytial-core's conformance harness a datum, the spec, and the spatial facts
  that should follow; `must.rightOf(a)` means "in every layout the spec
  permits", so a case is deterministic and needs no browser. Covers
  orientation transitivity along a linked list, the fact that a per-child tree
  spec does *not* entail the whole left subtree is left of the root, that
  sharing a value through two fields yields two atoms (no pointer identity —
  only `bool`/`None`/`()`/unit structs/unit variants are interned), and that
  `None` is interned across every empty slot. Also covers three decorator
  families beyond orientation: `cyclic` (fragment membership is symmetric, and
  includes the `None` terminator, since `next` relates to it like any other
  target), `size` (the selected atom gets exactly the dimensions asked for, and
  the unsized field atom does not), and `hide_atom` (the atom is both reported
  by `hidden()` and gone from `nodes()`, so recording the directive without
  applying it would not pass). The harness resolves Node via `SPYTIAL_NODE` then
  `PATH`, and skips rather than fails when Node or the vendored bundle is absent.

- Known issue, found by the above and filed as #88: the four `idx` emitters in
  `export.rs` write the position with `self.index.to_string()` and use it as a
  tuple atom id, but never emit an atom for it. Every `Vec`/array/slice, tuple,
  tuple struct and tuple-like enum variant export therefore names atoms `"0"`,
  `"1"`, … that are absent from `atoms` (`datum/dangling-tuple-atom`). Rendering
  is unaffected — `JSONDataInstance` keeps the tuples as written, layout
  generation succeeds, and every node and edge is present, because the index
  sits in a ternary tuple's middle position which is never drawn as a node. What
  it does break is selectors: the `index` type has no atoms, so nothing can join
  through `idx` and a decorator relating a container to its elements silently
  does nothing. Pinned from both sides in `tests/conformance.rs` — the graph
  connectivity as a passing case, the selector gap as an `#[ignore]`d one that
  passes once the index atoms are emitted.

- Changed: vendored spytial-core 4.3.0 → 4.4.2. The CLI first shipped in 4.4.1,
  and 4.4.2 added the `cyclic()`, `sized()` and `hidden()` spatial queries, which
  is what lets the conformance tests cover the `cyclic`, `size` and `hide_atom`
  decorators at all. The layout-spec language itself did not move across either
  release (still dated 2026-07-29), so `macros/src/spec_tables.rs` changed only
  in its version stamp and the derive macro's accepted keys are unchanged. The
  harness is vendored alongside the browser assets so it moves with the same
  `VERSION.txt` pin — a harness from one release checking specs written against
  another is the failure it exists to prevent — but it is the one file in
  `templates/vendor/` excluded from the published crate, since it is 3.2 MB of
  test-only code that would near-double the `.crate`.

Repo hygiene, no behaviour change:

- The derive macro's doc example never compiled. It needs `spytial`, which
  `spytial_export_macros` cannot depend on normally, and it went unnoticed
  because nothing ever ran that crate's doc tests. It compiles and is tested
  now, via a dev-dependency cycle — which Cargo permits, and which is stripped
  from the published manifest, so the release order is unchanged.
- `macros` is now a workspace member. A path dependency is not one on its own,
  so `cargo test` and `cargo test --workspace` both skipped the derive macro
  entirely; CI runs `--workspace` for tests and doc tests, and
  `tests/workspace.rs` fails if the membership goes away again.
- Nine clippy warnings in `macros` cleaned up (`unwrap_or_else(|| "".into())`
  to `unwrap_or_default()`). They were never reported before, for the same
  reason.
- The workspace now excludes `.claude/worktrees`, where Claude Code nests its
  git worktrees inside the checkout. A worktree on a branch predating the
  `[workspace]` section has none of its own, so cargo walked up, hit this
  checkout's manifest, and refused to build the worktree ("current package
  believes it's in a workspace when it's not"). This also required dropping
  `.` from `members`: exclusion is "under an excluded path and not under a
  member path", both prefix checks, so a literal `.` made every path in the
  checkout a member prefix and silently defeated `exclude`. The root package
  is a member regardless — it hosts the `[workspace]` section — and
  `tests/workspace.rs` now guards the exclusion with a throwaway nested
  package, alongside the membership guard.
- The attribute list in the derive's docs was checked against the generated
  spec tables: every attribute and key matches, nothing is documented that the
  macro does not accept.

Deprecated forms now warn at compile time:

- Every form spytial-core marks deprecated produces a `deprecated` warning
  naming the replacement and how its fields map across. The text is generated
  from the manifest's `deprecations[]`, so it moves when upstream's does.
  Nothing stops compiling and no behaviour changes — the deprecated forms are
  still parsed and rewritten exactly as before.
- The warning is keyed on the form, not the attribute, because two of them are
  a deprecated *shape* of an attribute that is otherwise current:
  `#[group(field = ...)]` warns and `#[group(selector = ...)]` does not;
  `#[edge_style(value = ...)]` warns and the `line_style(...)` block form does
  not. A bare `#[edge_style(field = "x")]` also stays quiet: it carries no
  deprecated key, and the legacy path it takes is this crate's own blue
  default rather than something the user asked for.
- `#[allow(deprecated)]` on the type silences it. The expansion copies the
  type's `allow`/`expect` attributes onto the generated marker, because that
  marker is a sibling item — without the copy the only way to quiet one legacy
  attribute would be `#![allow(deprecated)]` over the whole module.
- This closes a gap in the tables added below: `AttrSpec::deprecated_for` was
  generated correctly and never read, so the deprecation data was extracted
  from the manifest and then dropped. `#[group(field = ...)]` had no signal at
  all, because deprecation was recorded per attribute and `#[group]` merges a
  deprecated manifest item with a current one.
- spec-codegen fails the build if spytial-core deprecates something new that is
  neither warned about nor listed in `DEPRECATIONS_NOT_APPLICABLE` with a
  reason, and if a listed exemption goes stale. Wire-section placements and the
  inline `inferredEdge` fields are exempt: the macro never offered them.
- CI now runs the derive macro's own unit tests. It is a path dependency rather
  than a workspace member, so `cargo test` at the root never reached it and
  `--workspace` does not either; it needs its own `--manifest-path` step, like
  `eval-corpus` and `spec-codegen`.

The derive macro's accepted keys and compile-time validation are now generated
from spytial-core's own language manifest instead of transcribed by hand:

- Vendored spytial-core bumped 4.1.0 -> 4.3.0, which is the first release to
  ship `docs/spytial-language.json` — a machine-readable description of every
  constraint and directive, its fields, their closed vocabularies and numeric
  bounds, and whether the engine *rejects* a bad value or silently ignores it.
  The manifest is vendored alongside the browser assets.
- New `spec-codegen` crate (its own workspace, unpublished, like `eval-corpus`)
  generates `macros/src/spec_tables.rs` from that manifest. Its tests fail if
  the checked-in tables have drifted, and CI runs them.
- New `scripts/update-spytial-core.sh <version>` re-vendors and regenerates in
  one step, so a version bump can't silently leave the macro describing the
  previous language. The manual copy-the-files procedure it replaces is how the
  drift below accumulated.
- **Breaking**: `#[projection(sig = "...")]` is removed, along with
  `Directive::Projection`, `ProjectionDirective`, `ProjectionParams`, and
  `SpytialDecoratorsBuilder::projection`. No released spytial-core has ever had
  a parser for a `projection:` directive — it serialized into the spec and was
  dropped on the floor.
- **Breaking**: `#[flag]` now requires `name`, and rejects anything outside
  `hideDisconnected` / `hideDisconnectedBuiltIns`. The previous default was
  `important`, which the engine does not recognize, so a bare `#[flag]` emitted
  a directive that did nothing.
- **Breaking**: `#[orientation]` now requires `directions`. The previous
  default was `["up", "down"]`, neither of which is an orientation direction,
  so the constraint matched nothing. Values are checked against the vocabulary,
  and contradictory sets (`above` with `below`, a `directly*` variant with
  anything but its own plain counterpart) are compile errors, matching what
  spytial-core rejects at parse time.
- `#[cyclic]`'s `direction` now defaults to `clockwise` (the manifest's own
  default) rather than `up`, which is not a cycle direction. `align` and
  `cyclic` directions are now checked against their vocabularies.
- Unknown leaves inside a style block are compile errors, so
  `line_style(colour = "red")` fails instead of rendering unstyled. `size`
  dimensions must be greater than 0.
- `#[attribute]` gains `selector` and `filter`, and `#[hide_field]` gains
  `filter`. `AttributeParams.selector` already existed on the wire but the
  macro hardcoded it to `None`.
- New in spytial-core 4.2, now exposed: the `icon_style(path, placement,
  opacity)` block and `atom_style`'s independent `show_label`.
  **Breaking**: `#[icon]` is deprecated upstream and now rewrites onto
  `atom_style` — its one `show_labels` boolean splits into
  `icon_style(placement = ...)` and `show_label` — so `Directive::Icon`,
  `IconDirective`, and `IconParams` are removed, as `atomColor` and `edgeColor`
  already were.
- `SpytialDecoratorsBuilder` signature changes follow from the above:
  `atom_style` takes `icon_style` and `show_label`, `attribute_styled` and
  `hide_field` take `filter`.
- **Breaking** (reading, not just writing): removing `Directive::Icon` and
  `Directive::Projection` also removes the ability to *deserialize* a spec
  containing `icon:` or `projection:`. `Directive` is `#[serde(untagged)]`, so
  such a document now fails with `data did not match any variant of untagged
  enum Directive`, which names no key. `icon:` is deprecated upstream but
  spytial-core 4.3 still parses it, so a hand-written spec can legitimately
  contain one. This crate only ever writes decorator sets, so nothing in it is
  affected; a consumer round-tripping YAML through `SpytialDecorators` is.
- Legacy `edge_style` flat keys are validated like their block replacements:
  `style` against solid/dashed/dotted (after the same trim-and-lowercase
  spytial-core applies, so `"Dotted"` still parses) and `weight` for
  positivity. Both previously reached the runtime, which dropped them with a
  note on stderr.
- `add_edge(...)` is checked like the other blocks. A typo'd leaf
  (`add_edge(pointz = "togroup")`) silently fell back to `points: none`,
  drawing no connector at all.
- `#[icon]`'s `show_labels` defaults to `false`, matching the manifest. It
  defaulted to `true`, which inverted the whole rewrite for a bare `#[icon]`:
  a corner badge with the label on, where the engine draws a full-box icon with
  the label off.
- **Breaking** (wire format): `size` and `hideAtom` are emitted under
  `constraints:` rather than `directives:`, which is where the manifest says
  they belong. spytial-core still parses the directives placement but warns,
  and drops deprecated forms in a major release. `Directive::Size` and
  `Directive::HideAtom` are now `Constraint::Size` and `Constraint::HideAtom`,
  and `SizeDirective`/`HideAtomDirective` are renamed to
  `SizeConstraint`/`HideAtomConstraint`. The authoring surface is unchanged —
  `#[size(...)]` and `#[hide_atom(...)]` are written exactly as before, since
  the constraint/directive split is a wire-format detail Rust users never touch.
  A new test in `spec-codegen` checks every form's section against the manifest,
  because nothing in the authoring surface or the generated tables could.

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
- Every attribute key is read by one scan, so numbers, bools, arrays and the
  `group` shape test agree with strings about what a key is. Text inside a
  selector that reads like a key used to be taken as one, and the fallback was
  silent: `#[size(selector = "...width = 3...", width = 88)]` emitted the
  default width of 30, a real `negated = true` was dropped by a selector
  mentioning `negated = `, and a selector-based `#[group]` whose selector
  mentioned `field = ` was rewritten into an entirely different field-based
  group.
- A raw-string selector keeps its own whitespace. Token text is no longer
  flattened before extraction, which had rewritten the newlines a raw string
  can legitimately carry — including inside a quoted comparand, where
  `"a\nb"` silently became `"a b"`.
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
