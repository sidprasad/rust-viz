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
/// Most are 1:1. Two are not:
///   * `group` merges `group` (selector-based) and `group.byField`, because the
///     Rust attribute picks its shape from which keys are present.
///   * `edge_style` merges `edgeStyle` and `edgeColor`, because the legacy flat
///     keys desugar onto the same attribute.
///
/// `projection` is deliberately absent: no version of spytial-core parses it.
const ATTR_SOURCES: &[(&str, &[&str])] = &[
    ("attribute", &["attribute"]),
    ("flag", &["flag"]),
    ("orientation", &["orientation"]),
    ("align", &["align"]),
    ("cyclic", &["cyclic"]),
    ("group", &["group", "group.byField"]),
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
            default: f["default"].as_str().map(str::to_string),
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

fn emit_rule(r: &Rule) -> String {
    let values = match &r.values {
        Some(vs) => {
            let items: Vec<String> = vs.iter().map(|v| format!("{v:?}")).collect();
            format!("Some(&[{}])", items.join(", "))
        }
        None => "None".to_string(),
    };
    format!(
        "FieldRule {{ key: {:?}, yaml: {:?}, values: {}, exclusive_min: {}, min: {}, max: {}, required: {}, enforcement: {}, default: {} }}",
        r.key,
        r.yaml,
        values,
        opt_f64(r.exclusive_min),
        opt_f64(r.min),
        opt_f64(r.max),
        r.required,
        opt_str(&r.enforcement),
        opt_str(&r.default),
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
    s.push_str("/// Every nested style block the derive macro accepts.\npub static BLOCKS: &[BlockSpec] = &[\n");
    s.push_str(&block_entries.join("\n"));
    s.push_str("\n];\n\n");

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

/// Path to the vendored manifest, relative to this crate.
pub fn manifest_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("templates")
        .join("vendor")
        .join("spytial-language.json")
}

/// Path to the generated tables.
pub fn tables_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("macros")
        .join("src")
        .join("spec_tables.rs")
}
