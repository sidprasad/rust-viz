//! Guards against the macro's spec tables drifting from the vendored manifest.
//!
//! The failure this catches is quiet: bump the vendored spytial-core, forget to
//! regenerate, and `#[derive(SpytialDecorators)]` goes on accepting the keys of
//! a language version that is no longer shipped. Nothing fails to compile and
//! nothing warns — the diagrams just stop matching the spec.

use spytial_spec_codegen::{
    first_difference, generate, manifest_path, tables_path, vendored_manifest,
};

#[test]
fn checked_in_tables_match_the_vendored_manifest() {
    let generated = generate(&vendored_manifest().expect("vendored manifest is readable"))
        .expect("manifest generates cleanly");
    let checked_in = std::fs::read_to_string(tables_path()).expect("spec_tables.rs is readable");

    if let Some(diff) = first_difference(&checked_in, &generated) {
        panic!(
            "\n{} is out of date with {}.\n{}\n\n\
             Regenerate: cargo run --manifest-path spec-codegen/Cargo.toml\n",
            tables_path().display(),
            manifest_path().display(),
            diff,
        );
    }
}

/// The version the tables were generated from has to be the version that is
/// vendored — otherwise the stamp in the generated header is a lie, even if the
/// tables happen to be byte-identical because nothing relevant changed.
#[test]
fn generated_header_matches_the_vendored_version() {
    let manifest = vendored_manifest().expect("vendored manifest is readable");
    let value: serde_json::Value = serde_json::from_str(&manifest).expect("manifest is valid JSON");
    let core_version = value["spytialCoreVersion"].as_str().expect("has a version");

    let version_txt =
        std::fs::read_to_string(manifest_path().parent().unwrap().join("VERSION.txt"))
            .expect("VERSION.txt is readable");
    assert!(
        version_txt.contains(core_version),
        "templates/vendor/VERSION.txt says {:?} but the vendored manifest is from spytial-core {}.\n\
         The browser assets and the manifest must come from one release — \
         run scripts/update-spytial-core.sh rather than copying files by hand.",
        version_txt.trim(),
        core_version,
    );

    let tables = std::fs::read_to_string(tables_path()).expect("spec_tables.rs is readable");
    assert!(
        tables.contains(&format!("pub const SPYTIAL_CORE_VERSION: &str = {core_version:?}")),
        "spec_tables.rs was generated from a different spytial-core than the one vendored ({core_version}).\n\
         Regenerate: cargo run --manifest-path spec-codegen/Cargo.toml",
    );
}

/// Every attribute the derive macro declares must have a generated spec, and
/// vice versa. This is what would have caught `projection`: an attribute the
/// macro accepted for which spytial-core has never had a parser.
#[test]
fn macro_attributes_and_generated_specs_agree() {
    let lib = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("macros")
            .join("src")
            .join("lib.rs"),
    )
    .expect("macros/src/lib.rs is readable");

    // The `attributes(...)` list of the proc_macro_derive.
    let start = lib
        .find("attributes(")
        .expect("derive declares attributes(...)");
    let body = &lib[start + "attributes(".len()..];
    let end = body.find(')').expect("attributes(...) is closed");
    let declared: Vec<String> = body[..end]
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let generated = generate(&vendored_manifest().expect("manifest readable")).expect("generates");

    for attr in &declared {
        assert!(
            generated.contains(&format!("attr: {attr:?}")),
            "#[{attr}] is declared by the derive macro but has no generated spec — \
             spytial-core has no such form, or the codegen policy table needs an entry",
        );
    }

    // And nothing generated is missing from the derive's attribute list.
    for line in generated.lines() {
        let Some(rest) = line.trim().strip_prefix("attr: \"") else {
            continue;
        };
        let Some(attr) = rest.split('"').next() else {
            continue;
        };
        assert!(
            declared.iter().any(|d| d == attr),
            "the manifest describes `{attr}` but the derive macro does not declare it",
        );
    }
}

/// Which wire section each form is emitted in must match the manifest.
///
/// Rust users never see this split — they write `#[size(...)]` and the crate
/// decides — which is exactly why it drifted unnoticed: `size` and `hideAtom`
/// were emitted among the directives, a placement spytial-core still parses but
/// marks deprecated. Nothing in the authoring surface or the generated tables
/// could catch it, because the section is a property of the runtime enums.
#[test]
fn wire_sections_match_the_manifest() {
    let runtime = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("src")
            .join("spytial_annotations")
            .join("runtime.rs"),
    )
    .expect("runtime.rs is readable");

    // The variants of `enum Constraint` / `enum Directive`, by wrapper type.
    let variants = |enum_name: &str| -> Vec<String> {
        let start = runtime
            .find(&format!("pub enum {enum_name} {{"))
            .unwrap_or_else(|| panic!("runtime.rs declares `enum {enum_name}`"));
        let body = &runtime[start..];
        let end = body.find("\n}").expect("enum is closed");
        body[..end]
            .lines()
            .filter_map(|l| {
                let l = l.trim();
                let open = l.find('(')?;
                let name = l[..open].trim();
                name.chars()
                    .next()
                    .filter(|c| c.is_ascii_uppercase())
                    .map(|_| name.to_string())
            })
            .collect()
    };

    let constraints = variants("Constraint");
    let directives = variants("Directive");
    assert!(
        !constraints.is_empty() && !directives.is_empty(),
        "failed to parse the runtime enums"
    );

    let manifest = vendored_manifest().expect("manifest readable");
    let value: serde_json::Value = serde_json::from_str(&manifest).expect("valid JSON");

    // Manifest item id -> the Rust variant that emits it. Only forms the
    // runtime has a wire type for; legacy authoring forms (atomColor, icon,
    // edgeColor) desugar onto another variant and have none of their own.
    let emitted_by = [
        ("orientation", "Orientation"),
        ("cyclic", "Cyclic"),
        ("align", "Align"),
        ("group", "Group"),
        ("size", "Size"),
        ("hideAtom", "HideAtom"),
        ("flag", "Flag"),
        ("atomStyle", "AtomStyle"),
        ("edgeStyle", "EdgeStyle"),
        ("attribute", "Attribute"),
        ("tag", "Tag"),
        ("hideField", "HideField"),
        ("inferredEdge", "InferredEdge"),
    ];

    for (item_id, variant) in emitted_by {
        let item = value["items"]
            .as_array()
            .expect("items")
            .iter()
            .find(|i| i["id"].as_str() == Some(item_id))
            .unwrap_or_else(|| panic!("manifest has no item `{item_id}`"));
        let sections: Vec<&str> = item["sections"]
            .as_array()
            .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
            .unwrap_or_default();

        let in_constraints = constraints.iter().any(|v| v == variant);
        let in_directives = directives.iter().any(|v| v == variant);
        assert!(
            in_constraints || in_directives,
            "`{variant}` is in neither Constraint nor Directive in runtime.rs"
        );

        let actual = if in_constraints {
            "constraints"
        } else {
            "directives"
        };
        assert!(
            sections.contains(&actual),
            "`{item_id}` is emitted under `{actual}` (Rust variant `{variant}`), but the \
             manifest says it belongs under {sections:?}.\nspytial-core may still parse the \
             other placement, but it warns and will drop it in a major release.",
        );
    }
}
