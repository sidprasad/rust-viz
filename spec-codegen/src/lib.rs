//! Generate `macros/src/spec_tables.rs` from spytial-core's `spytial-language.json`.
//!
//! The manifest is spytial-core's own machine-readable description of the
//! layout-spec language: every constraint and directive, its fields, their
//! closed vocabularies and numeric bounds, and — crucially — `enforcement`,
//! which says whether the engine *rejects* a bad value or silently ignores it.
//!
//! Everything the derive macro can know declaratively is derived from that file.
//! What stays hand-written in `macros/src/lib.rs` is the part the manifest has
//! no opinion about: the literal-aware token scanner, the `negated` ⇄ `hold`
//! ergonomics, legacy-form desugaring, and the mapping onto builder methods.
//!
//! The one hand-maintained thing *here* is the policy table below: which Rust
//! attribute is authored from which manifest item, and how a field's YAML name
//! is spelled in Rust. Keeping it in one place is the point — before this, the
//! same policy was implicit across sixteen `parse_*_args` functions, which is
//! how the crate ended up shipping a `projection` directive that spytial-core
//! has no parser for.

use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

/// Rust attribute name → the manifest item ids it is authored from.
///
/// Most are 1:1. One is not: `edge_style` merges `edgeStyle` and `edgeColor`,
/// because the legacy flat keys desugar onto the same attribute.
///
/// `projection` is deliberately absent: no version of spytial-core parses it.
/// `group.byField` is absent because spytial-core 5.1.0 removed it from the
/// language outright — it is not deprecated, it no longer parses — so the
/// derive rejects `#[group(field = ...)]` with an error naming the selector
/// form rather than carrying a shape the engine would refuse.
const ATTR_SOURCES: &[(&str, &[&str])] = &[
    ("attribute", &["attribute"]),
    ("flag", &["flag"]),
    ("orientation", &["orientation"]),
    ("align", &["align"]),
    ("cyclic", &["cyclic"]),
    ("group", &["group"]),
    ("atom_color", &["atomColor"]),
    ("atom_style", &["atomStyle"]),
    ("size", &["size"]),
    ("icon", &["icon"]),
    ("edge_style", &["edgeStyle", "edgeColor"]),
    ("hide_field", &["hideField"]),
    ("hide_atom", &["hideAtom"]),
    ("inferred_edge", &["inferredEdge"]),
    ("tag", &["tag"]),
];

/// Manifest block name → Rust attribute-group name.
const BLOCK_SOURCES: &[(&str, &str)] = &[
    ("lineStyle", "line_style"),
    ("textStyle", "text_style"),
    ("borderStyle", "border_style"),
    ("fillStyle", "fill_style"),
    ("iconStyle", "icon_style"),
];

/// Manifest items the derive macro deliberately does not offer, and why.
///
/// Empty today. It exists so that "the macro has no attribute for this" is a
/// recorded decision rather than an omission — see the coverage check in
/// [`generate`].
const UNMAPPED_ITEMS: &[(&str, &str)] = &[];

/// Blocks the manifest describes inside a field's `alternativeForm` rather than
/// in the top-level `blocks` list.
///
/// `group`'s `addEdge` is written either as a bare direction string or as a
/// block, and the manifest attaches the block's shape to the field. Without
/// this the block gets no spec, and a typo in `add_edge(pointz = ...)` silently
/// yields no connector.
///
/// (item id, field name, Rust attribute-group name)
const ALT_FORM_BLOCKS: &[(&str, &str, &str)] = &[("group", "addEdge", "add_edge")];

/// Fields whose Rust spelling is not just the snake_case of the YAML key.
///
/// `hold` is a tri-state string upstream (`always`/`never`/absent) but only
/// `never` has an effect, so Rust exposes the ergonomic `negated = true`.
/// The `flag` item's single field is named `flag`, which would make the
/// attribute read `#[flag(flag = "...")]`; Rust spells it `name`.
const FIELD_RENAMES: &[(&str, &str, &str)] = &[
    // (item id, YAML field, Rust key)
    ("*", "hold", "negated"),
    ("flag", "flag", "name"),
];

/// Fields present in the manifest that the Rust attribute deliberately does
/// not accept, with the reason. These are all deprecated inline forms whose
/// replacement the macro already offers as a block.
const FIELD_SKIPS: &[(&str, &str)] = &[
    ("inferredEdge", "color"),
    ("inferredEdge", "style"),
    ("inferredEdge", "weight"),
    ("inferredEdge", "highlight"),
    // The legacy `edgeColor` form is frozen at the trio the macro already
    // desugars (value/style/weight). `highlight` arrived on `edgeColor` after
    // `edgeStyle` superseded it; the migration is `line_style(highlight = ...)`,
    // which already works, so growing the deprecated form would be backwards.
    ("edgeColor", "highlight"),
    // Contributed by `edgeStyle` already; listing both would duplicate the key.
    ("edgeColor", "field"),
    ("edgeColor", "selector"),
    ("edgeColor", "filter"),
    ("edgeColor", "showLabel"),
    ("edgeColor", "hidden"),
];

/// How to tell a deprecated *shape* apart from a current one, when the manifest
/// does not say and the two share a Rust attribute.
///
/// `edgeColor` has no manifest discriminator, because in YAML it is its own key; it is only
/// a shape here because the Rust attribute merges it into `#[edge_style]`. The
/// keys below are exactly the legacy flat trio `parse_edge_style_args` reads.
///
/// A bare `#[edge_style(field = "x")]` deliberately does *not* list: it carries
/// no deprecated key, and the legacy path it takes is this crate's own blue
/// default, not something the user asked for.
///
/// (manifest deprecation id, Rust keys whose presence selects the deprecated shape)
const SHAPE_DISCRIMINATORS: &[(&str, &[&str])] = &[("edgeColor", &["value", "style", "weight"])];

/// Manifest deprecations that cannot reach the Rust authoring surface, and why.
///
/// Like [`UNMAPPED_ITEMS`], this exists so silence is never the answer: every
/// entry in the manifest's `deprecations[]` is either warned about or listed
/// here. A spytial-core release that deprecates something new fails the build
/// until someone decides which it is.
const DEPRECATIONS_NOT_APPLICABLE: &[(&str, &str)] = &[
    (
        "size@directives",
        "a wire-section placement, not an authoring form; the crate emits size \
         under constraints already",
    ),
    (
        "hideAtom@directives",
        "likewise a placement; hideAtom is emitted under constraints",
    ),
    (
        "inferredEdge.color",
        "in FIELD_SKIPS — the macro never accepted the inline key",
    ),
    (
        "inferredEdge.style",
        "in FIELD_SKIPS — the macro never accepted the inline key",
    ),
    (
        "inferredEdge.weight",
        "in FIELD_SKIPS — the macro never accepted the inline key",
    ),
    (
        "inferredEdge.highlight",
        "in FIELD_SKIPS — the macro never accepted the inline key",
    ),
];

fn camel_to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn rust_key(item_id: &str, yaml_field: &str) -> String {
    for (item, yaml, rust) in FIELD_RENAMES {
        if (*item == "*" || *item == item_id) && *yaml == yaml_field {
            return (*rust).to_string();
        }
    }
    camel_to_snake(yaml_field)
}

fn is_skipped(item_id: &str, yaml_field: &str) -> bool {
    FIELD_SKIPS
        .iter()
        .any(|(item, field)| *item == item_id && *field == yaml_field)
}

/// A field's validation rules, as the generated table will carry them.
struct Rule {
    key: String,
    yaml: String,
    values: Option<Vec<String>>,
    exclusive_min: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
    required: bool,
    /// The manifest's `enforcement`: what spytial-core itself does with a bad
    /// value. `parse-error` means the engine rejects the whole spec;
    /// `value-ignored` means the leaf is silently dropped; `unchecked` means it
    /// is passed through and simply matches nothing.
    enforcement: Option<String>,
    default: Option<String>,
    /// The manifest's declared arity for a selector-typed field.
    arity: Option<String>,
    /// Every result shape the field accepts (spytial-core 5.1's `accepts`).
    accepts: Vec<Accepts>,
}

struct Accepts {
    arity: String,
    min_columns: Option<u64>,
    max_columns: Option<u64>,
    requires: Option<String>,
    meaning: String,
}

fn accepts_of(field: &Value) -> Vec<Accepts> {
    field["accepts"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|a| Accepts {
                    arity: a["arity"].as_str().unwrap_or_default().to_string(),
                    min_columns: a["minColumns"].as_u64(),
                    max_columns: a["maxColumns"].as_u64(),
                    requires: a["requires"].as_str().map(str::to_string),
                    meaning: a["meaning"].as_str().unwrap_or_default().to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn rules_for_fields(item_id: &str, fields: &[Value], out: &mut Vec<Rule>) {
    for f in fields {
        let yaml = f["name"].as_str().unwrap_or_default().to_string();
        if is_skipped(item_id, &yaml) {
            continue;
        }
        let key = rust_key(item_id, &yaml);
        if out.iter().any(|r| r.key == key) {
            continue; // merged item shapes can share a field (group.selector)
        }
        out.push(Rule {
            key,
            yaml: yaml.clone(),
            values: f["values"].as_array().map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            }),
            exclusive_min: f["exclusiveMinimum"].as_f64(),
            min: f["minimum"].as_f64(),
            max: f["maximum"].as_f64(),
            required: f["required"].as_bool().unwrap_or(false),
            enforcement: f["enforcement"].as_str().map(str::to_string),
            // Rendered as text whatever its JSON type: six of the manifest's
            // defaults are booleans or numbers (`showLabel: true`,
            // `icon.showLabels: false`, `iconStyle.opacity: 1`, …), and reading
            // only strings dropped every one of them — which is how `#[icon]`
            // came to default `show_labels` to the opposite of what the engine
            // assumes. Callers parse the text back to the type they want.
            default: match &f["default"] {
                Value::Null => None,
                Value::String(s) => Some(s.clone()),
                other => Some(other.to_string()),
            },
            arity: f["arity"].as_str().map(str::to_string),
            accepts: accepts_of(f),
        });
    }
}

fn opt_f64(v: Option<f64>) -> String {
    match v {
        Some(n) if n == n.trunc() && n.abs() < 1e15 => format!("Some({n:.1})"),
        Some(n) => format!("Some({n})"),
        None => "None".to_string(),
    }
}

fn opt_str(v: &Option<String>) -> String {
    match v {
        Some(s) => format!("Some({s:?})"),
        None => "None".to_string(),
    }
}

fn opt_u32(v: Option<u64>) -> String {
    match v {
        Some(n) => format!("Some({n})"),
        None => "None".to_string(),
    }
}

fn emit_rule(r: &Rule) -> String {
    let values = match &r.values {
        Some(vs) => {
            let items: Vec<String> = vs.iter().map(|v| format!("{v:?}")).collect();
            format!("Some(&[{}])", items.join(", "))
        }
        None => "None".to_string(),
    };
    let accepts: Vec<String> = r
        .accepts
        .iter()
        .map(|a| {
            format!(
                "SelectorArity {{ arity: {:?}, min_columns: {}, max_columns: {}, requires: {}, meaning: {:?} }}",
                a.arity,
                opt_u32(a.min_columns),
                opt_u32(a.max_columns),
                opt_str(&a.requires),
                a.meaning,
            )
        })
        .collect();
    format!(
        "FieldRule {{ key: {:?}, yaml: {:?}, values: {}, exclusive_min: {}, min: {}, max: {}, required: {}, enforcement: {}, default: {}, arity: {}, accepts: &[{}] }}",
        r.key,
        r.yaml,
        values,
        opt_f64(r.exclusive_min),
        opt_f64(r.min),
        opt_f64(r.max),
        r.required,
        opt_str(&r.enforcement),
        opt_str(&r.default),
        opt_str(&r.arity),
        accepts.join(", "),
    )
}

/// Turn the manifest into the source text of `macros/src/spec_tables.rs`.
pub fn generate(manifest_json: &str) -> Result<String, String> {
    let man: Value = serde_json::from_str(manifest_json).map_err(|e| e.to_string())?;
    let core_version = man["spytialCoreVersion"].as_str().unwrap_or("?");
    let lang_version = man["languageVersion"].as_str().unwrap_or("?");
    let items = man["items"].as_array().ok_or("manifest has no `items`")?;
    let blocks = man["blocks"].as_array().ok_or("manifest has no `blocks`")?;

    let find_item =
        |id: &str| -> Option<&Value> { items.iter().find(|i| i["id"].as_str() == Some(id)) };

    // Every item and block the manifest describes must be accounted for. Without
    // this, a spytial-core release that *adds* a directive re-vendors completely
    // green: the tables just omit it, and the drift test compares the derive's
    // attribute list against tables that are themselves fed by these policy
    // tables, so nothing downstream notices either. Anything deliberately not
    // exposed belongs in UNMAPPED_ITEMS with a reason, not in silence.
    for item in items {
        let id = item["id"].as_str().unwrap_or_default();
        let mapped = ATTR_SOURCES
            .iter()
            .any(|(_, sources)| sources.contains(&id))
            || UNMAPPED_ITEMS.iter().any(|(unmapped, _)| *unmapped == id);
        if !mapped {
            return Err(format!(
                "manifest item `{id}` is named by neither ATTR_SOURCES nor UNMAPPED_ITEMS in \
                 spec-codegen/src/lib.rs.\nspytial-core describes a form the derive macro does \
                 not offer: either map it to an attribute, or list it in UNMAPPED_ITEMS with \
                 the reason it is deliberately absent."
            ));
        }
    }
    for block in blocks {
        let name = block["name"].as_str().unwrap_or_default();
        if !BLOCK_SOURCES.iter().any(|(yaml, _)| *yaml == name) {
            return Err(format!(
                "manifest block `{name}` is not named by BLOCK_SOURCES in \
                 spec-codegen/src/lib.rs.\nspytial-core describes a style block the derive macro \
                 does not offer."
            ));
        }
    }

    let mut s = String::new();
    s.push_str(&format!(
        r#"// @generated by spec-codegen — do not edit by hand.
//
// Source: spytial-core {core_version}, layout-spec language {lang_version}
//         (templates/vendor/spytial-language.json)
// Regenerate: cargo run --manifest-path spec-codegen/Cargo.toml
//
// These tables are spytial-core's own description of the layout-spec language,
// lowered to Rust. They drive which keys `#[derive(SpytialDecorators)]` accepts
// and which values it rejects at compile time. The hand-written half of the
// macro — the literal-aware token scanner and the codegen — lives in lib.rs.

// The tables mirror the manifest in full, so some columns are carried for
// reference rather than read by the macro today: `yaml`/`yaml_key` record how
// spytial-core spells each name, `required` and `deprecated_for` record language
// facts the macro does not yet act on, and the version constants stamp the
// source. Dropping them would make the table a lossy view of the manifest and
// the next person's diff harder to read.
#![allow(dead_code)]

/// The spytial-core release these tables were generated from.
pub const SPYTIAL_CORE_VERSION: &str = {core_version:?};

/// The date the layout-spec language last changed, per the manifest. If this
/// has not moved since the tables were generated, nothing the macro emits needs
/// revisiting.
pub const LANGUAGE_VERSION: &str = {lang_version:?};

/// What spytial-core itself does with a value outside a field's vocabulary.
///
/// The macro rejects at compile time regardless — the point of these tables is
/// that a typo should fail loudly here rather than silently render nothing —
/// but the level explains *why* in the error message.
pub mod enforcement {{
    /// spytial-core refuses to parse the whole spec.
    pub const PARSE_ERROR: &str = "parse-error";
    /// spytial-core drops the leaf and renders with the default.
    pub const VALUE_IGNORED: &str = "value-ignored";
    /// spytial-core passes the value through; it simply matches nothing.
    pub const UNCHECKED: &str = "unchecked";
}}

/// One field's declarative rules.
#[derive(Debug)]
pub struct FieldRule {{
    /// The key as written in a Rust attribute (snake_case).
    pub key: &'static str,
    /// The key as spytial-core spells it in YAML.
    pub yaml: &'static str,
    /// Closed vocabulary, if the field has one.
    pub values: Option<&'static [&'static str]>,
    /// Exclusive lower bound (`weight`, `width`, …).
    pub exclusive_min: Option<f64>,
    /// Inclusive lower bound.
    pub min: Option<f64>,
    /// Inclusive upper bound.
    pub max: Option<f64>,
    /// Whether spytial-core requires the field.
    pub required: bool,
    /// See [`enforcement`].
    pub enforcement: Option<&'static str>,
    /// The value spytial-core assumes when the field is absent.
    pub default: Option<&'static str>,
    /// For a selector-typed field, the arity the manifest declares for it.
    pub arity: Option<&'static str>,
    /// Every result shape a selector-typed field accepts, and what each means
    /// (spytial-core 5.1). Empty for non-selector fields.
    pub accepts: &'static [SelectorArity],
}}

/// One result shape a selector-typed field accepts.
///
/// Not enforced by the macro — a selector is an opaque string until it is
/// evaluated against a datum — but carried so the derive's generated reference
/// can say what each selector may return, and so a future selector parser has
/// the rule to check against.
#[derive(Debug)]
pub struct SelectorArity {{
    /// `unary`, `binary`, or `n-ary`.
    pub arity: &'static str,
    /// Fewest columns this shape covers.
    pub min_columns: Option<u32>,
    /// Most columns this shape covers; `None` is unbounded.
    pub max_columns: Option<u32>,
    /// Another key that must be present for this shape to do anything.
    pub requires: Option<&'static str>,
    /// What the engine does with a result of this shape.
    pub meaning: &'static str,
}}

/// One authoring attribute, e.g. `#[orientation(...)]`.
#[derive(Debug)]
pub struct AttrSpec {{
    /// The Rust attribute name.
    pub attr: &'static str,
    /// The YAML key spytial-core reads it as.
    pub yaml_key: &'static str,
    /// Every key the attribute accepts, in manifest order.
    pub keys: &'static [&'static str],
    /// Per-field rules, in the same order as `keys`.
    pub rules: &'static [FieldRule],
    /// Set when spytial-core has deprecated this form, naming its replacement.
    pub deprecated_for: Option<&'static str>,
}}

impl AttrSpec {{
    /// The rule for `key`, if the attribute accepts it.
    pub fn rule(&self, key: &str) -> Option<&FieldRule> {{
        self.rules.iter().find(|r| r.key == key)
    }}
}}

/// A form spytial-core has deprecated, lowered to what the macro can warn about.
///
/// Not the same thing as [`AttrSpec::deprecated_for`]: that says the whole
/// attribute is deprecated, which is only true when the attribute has exactly
/// one manifest source. `#[group]` and `#[edge_style]` are current attributes
/// with one deprecated *shape* each, and the shape is chosen by which keys the
/// user wrote — so the warning has to be keyed on that, not on the attribute.
#[derive(Debug)]
pub struct DeprecationSpec {{
    /// The Rust authoring attribute the warning fires on.
    pub attr: &'static str,
    /// Rust keys whose presence selects the deprecated shape. Empty means the
    /// attribute is deprecated outright, whatever it is written with.
    pub when_any_key: &'static [&'static str],
    /// The replacement, spelled as the Rust attribute to reach for.
    pub replaced_by: &'static str,
    /// spytial-core's own reason and field mapping, for the warning note.
    pub note: &'static str,
}}

/// One nested style block, e.g. `line_style(...)`.
#[derive(Debug)]
pub struct BlockSpec {{
    /// The block name as written in a Rust attribute.
    pub block: &'static str,
    /// The block name as spytial-core spells it in YAML.
    pub yaml_key: &'static str,
    /// Per-leaf rules.
    pub rules: &'static [FieldRule],
}}

impl BlockSpec {{
    /// The rule for `key`, if the block accepts it.
    pub fn rule(&self, key: &str) -> Option<&FieldRule> {{
        self.rules.iter().find(|r| r.key == key)
    }}
}}

/// The spec for `attr`, or `None` if it is not an authoring attribute.
pub fn attr_spec(attr: &str) -> Option<&'static AttrSpec> {{
    ATTRS.iter().find(|a| a.attr == attr)
}}

/// The spec for a nested style block, e.g. `line_style`.
pub fn block_spec(block: &str) -> Option<&'static BlockSpec> {{
    BLOCKS.iter().find(|b| b.block == block)
}}

"#
    ));

    // ---- ATTRS ----
    let mut attr_entries = Vec::new();
    for (attr, source_ids) in ATTR_SOURCES {
        let mut rules: Vec<Rule> = Vec::new();
        let mut yaml_key = String::new();
        let mut deprecated_for: Option<String> = None;

        for id in *source_ids {
            let item = find_item(id).ok_or_else(|| {
                format!("manifest has no item `{id}` (needed for #[{attr}]); the language changed")
            })?;
            if yaml_key.is_empty() {
                yaml_key = item["yamlKey"].as_str().unwrap_or(id).to_string();
            }
            // A merged source's deprecation does not deprecate the Rust
            // attribute: `#[edge_style]` is current even though `edgeColor` is
            // not. Only a sole source's deprecation carries over.
            if source_ids.len() == 1 {
                if let Some(d) = item["deprecated"].as_object() {
                    deprecated_for = d
                        .get("replacedBy")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
            }
            if let Some(fields) = item["fields"].as_array() {
                rules_for_fields(id, fields, &mut rules);
            }
            if item["supportsHold"].as_bool() == Some(true)
                && !rules.iter().any(|r| r.key == "negated")
            {
                rules.push(Rule {
                    key: "negated".to_string(),
                    yaml: "hold".to_string(),
                    values: None,
                    exclusive_min: None,
                    min: None,
                    max: None,
                    required: false,
                    enforcement: None,
                    default: None,
                    arity: None,
                    accepts: Vec::new(),
                });
            }
        }

        let keys: Vec<String> = rules.iter().map(|r| format!("{:?}", r.key)).collect();
        let rule_lits: Vec<String> = rules.iter().map(emit_rule).collect();
        attr_entries.push(format!(
            "    AttrSpec {{ attr: {:?}, yaml_key: {:?}, keys: &[{}], rules: &[{}], deprecated_for: {} }},",
            attr,
            yaml_key,
            keys.join(", "),
            rule_lits.join(", "),
            opt_str(&deprecated_for),
        ));
    }
    s.push_str("/// Every authoring attribute the derive macro accepts.\npub static ATTRS: &[AttrSpec] = &[\n");
    s.push_str(&attr_entries.join("\n"));
    s.push_str("\n];\n\n");

    // ---- BLOCKS ----
    let mut block_entries = Vec::new();
    for (yaml_name, rust_name) in BLOCK_SOURCES {
        let block = blocks
            .iter()
            .find(|b| b["name"].as_str() == Some(yaml_name))
            .ok_or_else(|| format!("manifest has no block `{yaml_name}`; the language changed"))?;
        let mut rules = Vec::new();
        if let Some(fields) = block["fields"].as_array() {
            rules_for_fields(yaml_name, fields, &mut rules);
        }
        let rule_lits: Vec<String> = rules.iter().map(emit_rule).collect();
        block_entries.push(format!(
            "    BlockSpec {{ block: {:?}, yaml_key: {:?}, rules: &[{}] }},",
            rust_name,
            yaml_name,
            rule_lits.join(", "),
        ));
    }

    // Blocks the manifest hangs off a field's `alternativeForm`.
    for (item_id, field_name, rust_name) in ALT_FORM_BLOCKS {
        let item = find_item(item_id)
            .ok_or_else(|| format!("manifest has no item `{item_id}`; the language changed"))?;
        let alt = item["fields"]
            .as_array()
            .and_then(|fs| fs.iter().find(|f| f["name"].as_str() == Some(*field_name)))
            .map(|f| &f["alternativeForm"])
            .filter(|a| !a.is_null())
            .ok_or_else(|| {
                format!("`{item_id}.{field_name}` has no alternativeForm; the language changed")
            })?;
        let mut rules = Vec::new();
        if let Some(fields) = alt["fields"].as_array() {
            rules_for_fields(*field_name, fields, &mut rules);
        }
        let rule_lits: Vec<String> = rules.iter().map(emit_rule).collect();
        block_entries.push(format!(
            "    BlockSpec {{ block: {:?}, yaml_key: {:?}, rules: &[{}] }},",
            rust_name,
            field_name,
            rule_lits.join(", "),
        ));
    }

    s.push_str("/// Every nested style block the derive macro accepts.\npub static BLOCKS: &[BlockSpec] = &[\n");
    s.push_str(&block_entries.join("\n"));
    s.push_str("\n];\n\n");

    // ---- DEPRECATIONS ----
    //
    // Generated from the manifest's own `deprecations[]` so the note text moves
    // when spytial-core's does. Only `kind: "item"` entries can reach the Rust
    // authoring surface at all; placements and inline fields belong in
    // DEPRECATIONS_NOT_APPLICABLE with the reason.
    let no_deps = Vec::new();
    let deprecations = man["deprecations"].as_array().unwrap_or(&no_deps);
    let mut dep_entries = Vec::new();
    for dep in deprecations {
        let id = dep["id"].as_str().unwrap_or_default();
        if DEPRECATIONS_NOT_APPLICABLE.iter().any(|(d, _)| *d == id) {
            continue;
        }
        let kind = dep["kind"].as_str().unwrap_or("?");
        if kind != "item" {
            return Err(format!(
                "manifest deprecation `{id}` is a `{kind}`, which the derive macro has no way to \
                 warn about, and it is not listed in DEPRECATIONS_NOT_APPLICABLE in \
                 spec-codegen/src/lib.rs."
            ));
        }

        let Some((attr, sources)) = ATTR_SOURCES.iter().find(|(_, s)| s.contains(&id)) else {
            return Err(format!(
                "manifest deprecates item `{id}`, which no Rust attribute is authored from and \
                 which is not in DEPRECATIONS_NOT_APPLICABLE in spec-codegen/src/lib.rs."
            ));
        };
        let item = find_item(id)
            .ok_or_else(|| format!("manifest deprecates `{id}` but describes no such item"))?;

        // Which keys select the deprecated shape. The manifest's own
        // discriminator wins; SHAPE_DISCRIMINATORS covers the case where a Rust
        // attribute merges sources that YAML keeps apart.
        let when: Vec<String> = match item["discriminator"].as_object() {
            Some(d) if d.get("present").and_then(Value::as_bool) == Some(true) => {
                let field = d.get("field").and_then(Value::as_str).unwrap_or_default();
                vec![rust_key(id, field)]
            }
            Some(_) => Vec::new(),
            None => match SHAPE_DISCRIMINATORS.iter().find(|(d, _)| *d == id) {
                Some((_, keys)) => keys.iter().map(|k| (*k).to_string()).collect(),
                // Sole source: the whole attribute is deprecated.
                None if sources.len() == 1 => Vec::new(),
                None => {
                    return Err(format!(
                        "manifest deprecates `{id}`, which shares #[{attr}] with {} other manifest \
                         item(s), but neither the manifest nor SHAPE_DISCRIMINATORS says which keys \
                         select it. Warning unconditionally would fire on the current form too.",
                        sources.len() - 1,
                    ))
                }
            },
        };

        let replaced_id = dep["replacedBy"].as_str().unwrap_or_default();
        let replaced_by = ATTR_SOURCES
            .iter()
            .find(|(_, s)| s.contains(&replaced_id))
            .map(|(a, _)| *a)
            .ok_or_else(|| {
                format!(
                    "manifest says `{id}` is replaced by `{replaced_id}`, which no Rust attribute \
                     is authored from — the warning would name something the user cannot write."
                )
            })?;

        // Reason, then the field mapping in Rust spelling. Identity entries
        // (`selector` -> `selector`) carry nothing and are dropped.
        let mut note = dep["reason"].as_str().unwrap_or_default().to_string();
        let mapping: Vec<String> = dep["mapping"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| {
                        (
                            camel_to_snake(k),
                            camel_to_snake(v.as_str().unwrap_or_default()),
                        )
                    })
                    .filter(|(k, v)| k != v)
                    .map(|(k, v)| format!("{k} -> {v}"))
                    .collect()
            })
            .unwrap_or_default();
        if !mapping.is_empty() {
            note.push_str(&format!(" Mapping: {}.", mapping.join("; ")));
        }
        if let Some(extra) = item["note"].as_str() {
            note.push(' ');
            note.push_str(extra);
        }

        let when_lits: Vec<String> = when.iter().map(|k| format!("{k:?}")).collect();
        dep_entries.push(format!(
            "    DeprecationSpec {{ attr: {:?}, when_any_key: &[{}], replaced_by: {:?}, note: {:?} }},",
            attr,
            when_lits.join(", "),
            replaced_by,
            note,
        ));
    }

    // A stale exemption is as bad as a missing one: it means the policy table
    // still excuses something the manifest no longer says.
    for (id, _) in DEPRECATIONS_NOT_APPLICABLE {
        if !deprecations.iter().any(|d| d["id"].as_str() == Some(*id)) {
            return Err(format!(
                "DEPRECATIONS_NOT_APPLICABLE in spec-codegen/src/lib.rs excuses `{id}`, which the \
                 manifest no longer deprecates. Drop the entry."
            ));
        }
    }

    s.push_str(
        "/// Forms spytial-core has deprecated, in the order the manifest lists them.\n\
         pub static DEPRECATIONS: &[DeprecationSpec] = &[\n",
    );
    s.push_str(&dep_entries.join("\n"));
    s.push_str("\n];\n\n");

    // ---- source block support (spytial-core 5.4) ----
    //
    // Which forms accept a `source: { text, location }` block, by YAML key,
    // and which of those the viewer actually displays in conflict reports. The
    // derive stamps every form in the first list; a macro test holds it to
    // that, so a release that withdraws support fails loudly.
    let source_list = |key: &str| -> Vec<String> {
        man["source"][key]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(|v| format!("{v:?}"))
                    .collect()
            })
            .unwrap_or_default()
    };
    s.push_str(&format!(
        r#"/// Forms that accept a `source: {{ text, location }}` block (spytial-core 5.4),
/// by YAML key. The derive stamps every one of them; `flag` is a bare scalar
/// and has nowhere to carry one.
pub static SOURCE_SUPPORTED_BY: &[&str] = &[{}];

/// The forms whose `source` the viewer shows in conflict reports and
/// warnings. On the rest it is parsed and ignored.
pub static SOURCE_DISPLAYED_BY: &[&str] = &[{}];

"#,
        source_list("supportedBy").join(", "),
        source_list("displayedBy").join(", "),
    ));

    // ---- orientation list rules ----
    let orientation = find_item("orientation").ok_or("manifest has no `orientation` item")?;
    let list_rules = &orientation["fields"]
        .as_array()
        .and_then(|fs| fs.iter().find(|f| f["name"] == "directions"))
        .ok_or("orientation has no `directions` field")?["listRules"];

    let at_most: Vec<String> = list_rules["atMostOneOf"]
        .as_array()
        .map(|groups| {
            groups
                .iter()
                .map(|g| {
                    let items: Vec<String> = g
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(|v| format!("{v:?}"))
                                .collect()
                        })
                        .unwrap_or_default();
                    format!("&[{}]", items.join(", "))
                })
                .collect()
        })
        .unwrap_or_default();

    let narrows: Vec<String> = list_rules["narrowsListTo"]
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    let items: Vec<String> = v
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(|x| format!("{x:?}"))
                                .collect()
                        })
                        .unwrap_or_default();
                    format!("({k:?}, &[{}] as &[&str])", items.join(", "))
                })
                .collect()
        })
        .unwrap_or_default();

    s.push_str(&format!(
        r#"/// Direction pairs `orientation` rejects together (`above` with `below`,
/// `left` with `right`). spytial-core enforces these at parse time.
pub static ORIENTATION_AT_MOST_ONE_OF: &[&[&str]] = &[{}];

/// A `directly*` direction narrows the rest of the list to these values.
pub static ORIENTATION_NARROWS_TO: &[(&str, &[&str])] = &[{}];
"#,
        at_most.join(", "),
        narrows.join(", "),
    ));

    rustfmt(&s)
}

/// Run the emitted source through rustfmt, so the checked-in file is stable
/// under `cargo fmt --check` and the drift test compares like with like.
fn rustfmt(src: &str) -> Result<String, String> {
    let mut child = Command::new("rustfmt")
        .args(["--edition", "2021", "--emit", "stdout", "--quiet"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run rustfmt (is the component installed?): {e}"))?;
    child
        .stdin
        .take()
        .ok_or("rustfmt stdin unavailable")?
        .write_all(src.as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "rustfmt rejected the generated source: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    String::from_utf8(out.stdout).map_err(|e| e.to_string())
}

/// The vendored manifest, as text. Both the generator and the drift test read
/// it through here so they can never disagree about which file is the source.
pub fn vendored_manifest() -> std::io::Result<String> {
    std::fs::read_to_string(manifest_path())
}

/// Where the checked-in tables first differ from freshly generated ones, or
/// `None` if they agree.
///
/// Line-ending style is not a difference. Git hands the file to a Windows
/// checkout with CRLF while rustfmt always emits LF, so a byte comparison
/// reports every line of a perfectly current file as drifted. The committed
/// blob is LF either way, so normalizing here compares what is actually
/// tracked.
///
/// Reports the first differing line rather than the whole file: these tables
/// run to hundreds of lines, and a dump of both copies buries the one line
/// that moved.
pub fn first_difference(on_disk: &str, generated: &str) -> Option<String> {
    // `lines()` drops the distinction, and a file missing its final newline is
    // a file the generator would rewrite.
    if on_disk.ends_with('\n') != generated.ends_with('\n') {
        return Some(if generated.ends_with('\n') {
            "the checked-in file is missing its final newline".to_string()
        } else {
            "the checked-in file has a final newline the generated one does not".to_string()
        });
    }

    let mut disk_lines = on_disk.lines();
    let mut gen_lines = generated.lines();
    let mut n = 0usize;
    loop {
        n += 1;
        match (disk_lines.next(), gen_lines.next()) {
            (None, None) => return None,
            (a, b) if a == b => continue,
            (Some(a), Some(b)) => {
                return Some(format!(
                    "first difference at line {n}:\n  on disk:    {a}\n  generated:  {b}"
                ))
            }
            (Some(a), None) => {
                return Some(format!(
                    "the checked-in file has {n} or more lines; \
                     the generated one ends at line {}.\n  extra on disk: {a}",
                    n - 1
                ))
            }
            (None, Some(b)) => {
                return Some(format!(
                    "the checked-in file ends at line {}; \
                     the generated one continues.\n  missing: {b}",
                    n - 1
                ))
            }
        }
    }
}

/// Path to the vendored manifest, relative to this crate.
pub fn manifest_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("templates")
        .join("vendor")
        .join("spytial-language.json")
}

/// Path to the generated attribute reference the derive's rustdoc includes.
pub fn reference_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("macros")
        .join("src")
        .join("attributes.md")
}

/// Path to the generated tables.
pub fn tables_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("macros")
        .join("src")
        .join("spec_tables.rs")
}

// ---------------------------------------------------------------------------
// The attribute reference: `macros/src/attributes.md`
// ---------------------------------------------------------------------------

/// Turn the manifest into the attribute reference the derive's rustdoc
/// includes and the guide embeds.
///
/// The bullet list this replaces was hand-written and checked against the
/// tables by eye. Generating it from the same manifest means a new key, a
/// widened vocabulary, a changed default, or a new selector arity reaches the
/// docs in the same commit that reaches the macro — and the drift test holds
/// the checked-in file to it. Everything here is spytial-core's own wording;
/// the only local additions are the Rust spellings and the block syntax.
pub fn generate_reference(manifest_json: &str) -> Result<String, String> {
    let man: Value = serde_json::from_str(manifest_json).map_err(|e| e.to_string())?;
    let core_version = man["spytialCoreVersion"].as_str().unwrap_or("?");
    let lang_version = man["languageVersion"].as_str().unwrap_or("?");
    let items = man["items"].as_array().ok_or("manifest has no `items`")?;
    let blocks = man["blocks"].as_array().ok_or("manifest has no `blocks`")?;
    let find_item = |id: &str| item_by_id(items, id);
    let docs = &man["documentation"];
    let doc_link = |key: &str, label: &str| -> String {
        match docs[key].as_str() {
            Some(url) => format!("[{label}]({url})"),
            None => label.to_string(),
        }
    };

    let mut s = String::new();
    s.push_str(&format!(
        "<!-- @generated by spec-codegen from spytial-core {core_version} (layout-spec \
         language {lang_version}). Do not edit; run `cargo run --manifest-path \
         spec-codegen/Cargo.toml`. -->\n\n"
    ));
    s.push_str(&format!(
        "Generated from spytial-core {core_version}'s language manifest, so every key, \
         vocabulary, bound, default, and selector arity below is the engine's own \
         description of the language (dated {lang_version}). Keys are the Rust spellings; \
         `negated = true` is spytial-core's `hold: never`. Where a selector lists more than \
         one accepted shape, the first is the one the field is designed for. spytial-core's \
         own guides: {}, {}, {}, {}.\n\n",
        doc_link("reference", "YAML reference"),
        doc_link("constraints", "constraints"),
        doc_link("directives", "directives"),
        doc_link("selectors", "selectors"),
    ));

    for (heading, section) in [("Constraints", "constraints"), ("Directives", "directives")] {
        s.push_str(&format!("## {heading}\n\n"));
        for (attr, source_ids) in ATTR_SOURCES {
            let primary = find_item(source_ids[0])
                .ok_or_else(|| format!("manifest has no item `{}`", source_ids[0]))?;
            let in_section = primary["sections"]
                .as_array()
                .map(|a| a.iter().any(|v| v.as_str() == Some(section)))
                .unwrap_or(false);
            if !in_section {
                continue;
            }
            write_item_reference(&mut s, attr, source_ids, primary, items)?;
        }
    }

    s.push_str("## Style blocks\n\n");
    s.push_str(
        "Blocks are written as nested groups that mirror the YAML: \
         `line_style(color = \"gray\", pattern = \"dotted\")`. Every leaf is optional.\n\n",
    );
    for (yaml_name, rust_name) in BLOCK_SOURCES {
        let block = blocks
            .iter()
            .find(|b| b["name"].as_str() == Some(yaml_name))
            .ok_or_else(|| format!("manifest has no block `{yaml_name}`"))?;
        s.push_str(&format!("### `{rust_name}(…)`\n\n"));
        if let Some(d) = block["description"].as_str() {
            s.push_str(&format!("{d}\n\n"));
        }
        if let Some(fields) = block["fields"].as_array() {
            for f in fields {
                s.push_str(&field_line(yaml_name, f, None));
            }
        }
        s.push('\n');
    }
    for (item_id, field_name, rust_name) in ALT_FORM_BLOCKS {
        let item = find_item(item_id).ok_or_else(|| format!("manifest has no item `{item_id}`"))?;
        let field = item["fields"]
            .as_array()
            .and_then(|fs| fs.iter().find(|f| f["name"].as_str() == Some(*field_name)))
            .ok_or_else(|| format!("`{item_id}` has no field `{field_name}`"))?;
        let alt = &field["alternativeForm"];
        s.push_str(&format!(
            "### `{rust_name}(…)` (on `#[{}]`)\n\n",
            ATTR_SOURCES
                .iter()
                .find(|(_, ids)| ids.contains(item_id))
                .map(|(a, _)| *a)
                .unwrap_or(item_id)
        ));
        if let Some(d) = alt["description"].as_str() {
            s.push_str(&format!("{d}\n\n"));
        }
        if let Some(fields) = alt["fields"].as_array() {
            for f in fields {
                s.push_str(&field_line(field_name, f, None));
            }
        }
        s.push('\n');
    }

    // Deprecated forms, in manifest order, with the mapping in Rust spelling.
    let no_deps = Vec::new();
    let deprecations = man["deprecations"].as_array().unwrap_or(&no_deps);
    let mut wrote_heading = false;
    for dep in deprecations {
        if dep["kind"].as_str() != Some("item") {
            continue;
        }
        let id = dep["id"].as_str().unwrap_or_default();
        let Some((attr, sources)) = ATTR_SOURCES.iter().find(|(_, s)| s.contains(&id)) else {
            continue;
        };
        if !wrote_heading {
            s.push_str("## Deprecated forms\n\n");
            s.push_str(
                "Each still compiles and still works, with a compile-time deprecation warning \
                 naming the replacement; `#[allow(deprecated)]` on the type keeps one \
                 deliberately.\n\n",
            );
            wrote_heading = true;
        }
        let replaced = dep["replacedBy"].as_str().unwrap_or_default();
        let replaced_attr = ATTR_SOURCES
            .iter()
            .find(|(_, s)| s.contains(&replaced))
            .map(|(a, _)| *a)
            .unwrap_or(replaced);
        let form = if sources.len() == 1 {
            format!("`#[{attr}(…)]`")
        } else {
            // A deprecated shape of a current attribute: name it by its keys.
            let keys: Vec<String> = SHAPE_DISCRIMINATORS
                .iter()
                .find(|(d, _)| *d == id)
                .map(|(_, keys)| keys.iter().map(|k| format!("`{k}`")).collect())
                .unwrap_or_default();
            format!("`#[{attr}]` written with {}", keys.join("/"))
        };
        s.push_str(&format!(
            "- {form} → `#[{replaced_attr}]`. {}",
            dep["reason"].as_str().unwrap_or_default()
        ));
        let mapping: Vec<String> = dep["mapping"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| {
                        (
                            camel_to_snake(k),
                            camel_to_snake(v.as_str().unwrap_or_default()),
                        )
                    })
                    .filter(|(k, v)| k != v)
                    .map(|(k, v)| format!("`{k}` → `{v}`"))
                    .collect()
            })
            .unwrap_or_default();
        if !mapping.is_empty() {
            s.push_str(&format!(" Mapping: {}.", mapping.join("; ")));
        }
        s.push('\n');
    }
    if wrote_heading {
        s.push('\n');
    }

    Ok(s)
}

fn item_by_id<'a>(items: &'a [Value], id: &str) -> Option<&'a Value> {
    items.iter().find(|i| i["id"].as_str() == Some(id))
}

/// One attribute's entry: what it does, an example in Rust syntax, and a line
/// per key.
fn write_item_reference(
    s: &mut String,
    attr: &str,
    source_ids: &[&str],
    primary: &Value,
    items: &[Value],
) -> Result<(), String> {
    s.push_str(&format!("### `#[{attr}(…)]`\n\n"));

    if let Some(d) = primary["description"].as_str() {
        s.push_str(d);
    }
    let mut traits = Vec::new();
    if let Some(sections) = primary["sections"].as_array() {
        if sections.iter().any(|v| v.as_str() == Some("constraints")) {
            traits.push("a layout constraint".to_string());
        }
    }
    if primary["supportsHold"].as_bool() == Some(true) {
        traits.push("takes `negated = true`".to_string());
    }
    if let Some(d) = primary["deprecated"].as_object() {
        let replaced = d
            .get("replacedBy")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let replaced_attr = ATTR_SOURCES
            .iter()
            .find(|(_, s)| s.contains(&replaced))
            .map(|(a, _)| *a)
            .unwrap_or(replaced);
        traits.push(format!("**deprecated**, rewrites to `#[{replaced_attr}]`"));
    }
    if !traits.is_empty() {
        s.push_str(&format!(" _({})_", traits.join("; ")));
    }
    s.push_str("\n\n");

    if let Some(example) = primary["example"].as_object() {
        let args: Vec<String> = example
            .iter()
            .map(|(k, v)| rust_example_arg(&rust_key(source_ids[0], k), v))
            .collect();
        s.push_str(&format!("```text\n#[{attr}({})]\n```\n\n", args.join(", ")));
    }

    // Keys, primary source first, then the legacy keys a merged source adds.
    let mut seen: Vec<String> = Vec::new();
    for (index, id) in source_ids.iter().enumerate() {
        let item = item_by_id(items, id).ok_or_else(|| format!("manifest has no item `{id}`"))?;
        let legacy = if index == 0 {
            None
        } else {
            Some(format!(
                "the deprecated `{id}` form, which rewrites onto the blocks above; mixing the \
                 two shapes is a compile error"
            ))
        };
        if let Some(fields) = item["fields"].as_array() {
            for f in fields {
                let yaml = f["name"].as_str().unwrap_or_default();
                if is_skipped(id, yaml) {
                    continue;
                }
                let key = rust_key(id, yaml);
                if seen.contains(&key) {
                    continue;
                }
                seen.push(key);
                s.push_str(&field_line(id, f, legacy.as_deref()));
            }
        }
        if index == 0 && item["supportsHold"].as_bool() == Some(true) {
            s.push_str(
                "- `negated`: `true` asserts the relationship must *not* hold \
                 (spytial-core's `hold: never`).\n",
            );
        }
    }
    if let Some(note) = primary["note"].as_str() {
        s.push_str(&format!("\n> {note}\n"));
    }
    s.push('\n');
    Ok(())
}

/// One `key = value` (or, for a block, `key(leaf = value, …)`) of a manifest
/// example, in Rust attribute syntax.
fn rust_example_arg(key: &str, v: &Value) -> String {
    match v {
        Value::Object(map) => {
            let inner: Vec<String> = map
                .iter()
                .map(|(k, v)| rust_example_arg(&camel_to_snake(k), v))
                .collect();
            format!("{key}({})", inner.join(", "))
        }
        other => format!("{key} = {}", rust_example_value(other)),
    }
}

/// A manifest example scalar or list in Rust attribute syntax.
fn rust_example_value(v: &Value) -> String {
    match v {
        Value::String(text) => format!("{text:?}"),
        Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(rust_example_value).collect();
            format!("[{}]", inner.join(", "))
        }
        other => other.to_string(),
    }
}

/// One `- key: …` line for a field, from the manifest's own description of it.
fn field_line(item_id: &str, f: &Value, legacy: Option<&str>) -> String {
    let yaml = f["name"].as_str().unwrap_or_default();
    let key = rust_key(item_id, yaml);
    let kind = f["type"].as_str().unwrap_or("string");

    let mut parts: Vec<String> = Vec::new();
    let vocabulary = |f: &Value| -> Option<String> {
        f["values"].as_array().map(|vs| {
            vs.iter()
                .filter_map(Value::as_str)
                .map(|v| format!("`{v}`"))
                .collect::<Vec<_>>()
                .join(", ")
        })
    };
    match kind {
        "block" => {
            let block = f["block"].as_str().unwrap_or_default();
            let rust = BLOCK_SOURCES
                .iter()
                .find(|(yaml, _)| *yaml == block)
                .map(|(_, rust)| *rust)
                .unwrap_or(block);
            parts.push(format!("`{rust}(…)` block"));
        }
        "enum" => {
            if let Some(v) = vocabulary(f) {
                parts.push(format!("one of {v}"));
            }
            if !f["alternativeForm"].is_null() {
                let rust = ALT_FORM_BLOCKS
                    .iter()
                    .find(|(item, field, _)| *item == item_id && *field == yaml)
                    .map(|(_, _, rust)| *rust)
                    .unwrap_or(&key);
                parts.push(format!(
                    "or the `{rust}(…)` block, which also styles it (see Style blocks)"
                ));
            }
        }
        "enum-list" => {
            if let Some(v) = vocabulary(f) {
                parts.push(format!("one or more of {v}"));
            }
        }
        "boolean" => parts.push("`true` or `false`".to_string()),
        "number" => {
            let mut bound = "number".to_string();
            if f["exclusiveMinimum"].as_f64() == Some(0.0) {
                bound.push_str(" greater than 0");
            } else if let (Some(min), Some(max)) = (f["minimum"].as_f64(), f["maximum"].as_f64()) {
                bound.push_str(&format!(" between {min} and {max}"));
            } else if let Some(min) = f["minimum"].as_f64() {
                bound.push_str(&format!(" of at least {min}"));
            }
            parts.push(bound);
        }
        "selector" => parts.push("selector".to_string()),
        "relation" => parts.push("relation (field) name".to_string()),
        "color" => parts.push("color".to_string()),
        "icon-path" => parts.push("icon name, icon-pack reference, URL, or path".to_string()),
        other => parts.push(other.to_string()),
    }
    if let Some(d) = f["default"].as_str() {
        parts.push(format!("default `{d}`"));
    } else if !f["default"].is_null() {
        parts.push(format!("default `{}`", f["default"]));
    }
    // A merged legacy source's `required` describes its own shape, not the
    // attribute: `value` is required in the `edgeColor` form, not in
    // `#[edge_style]`.
    if f["required"].as_bool() == Some(true) && legacy.is_none() {
        parts.push("required".to_string());
    }
    if let Some(legacy) = legacy {
        parts.push(legacy.to_string());
    }

    let mut line = format!("- `{key}`: {}.", parts.join("; "));
    if let Some(d) = f["description"].as_str() {
        line.push(' ');
        line.push_str(d);
    }
    if let Some(note) = f["note"].as_str() {
        line.push(' ');
        line.push_str(note);
    }
    match f["enforcement"].as_str() {
        Some("parse-error") => {
            line.push_str(" _(a bad value makes spytial-core reject the whole spec)_")
        }
        Some("value-ignored") => line.push_str(" _(spytial-core drops a bad value silently)_"),
        _ => {}
    }
    line.push('\n');

    if let Some(accepts) = f["accepts"].as_array() {
        for a in accepts {
            let arity = a["arity"].as_str().unwrap_or_default();
            let columns = match (a["minColumns"].as_u64(), a["maxColumns"].as_u64()) {
                (Some(min), Some(max)) if min == max => {
                    format!("{min} column{}", if min == 1 { "" } else { "s" })
                }
                (Some(min), Some(max)) => format!("{min} to {max} columns"),
                (Some(min), None) => format!("{min} or more columns"),
                _ => String::new(),
            };
            let requires = a["requires"]
                .as_str()
                .map(|r| format!("; needs `{}`", camel_to_snake(r)))
                .unwrap_or_default();
            line.push_str(&format!(
                "  - {arity}{}{requires}: {}\n",
                if columns.is_empty() {
                    String::new()
                } else {
                    format!(" ({columns})")
                },
                a["meaning"].as_str().unwrap_or_default(),
            ));
        }
    }
    line
}
