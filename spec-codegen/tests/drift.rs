//! Guards against the macro's spec tables drifting from the vendored manifest.
//!
//! The failure this catches is quiet: bump the vendored spytial-core, forget to
//! regenerate, and `#[derive(SpytialDecorators)]` goes on accepting the keys of
//! a language version that is no longer shipped. Nothing fails to compile and
//! nothing warns — the diagrams just stop matching the spec.

use spytial_spec_codegen::{generate, manifest_path, tables_path, vendored_manifest};

#[test]
fn checked_in_tables_match_the_vendored_manifest() {
    let generated = generate(&vendored_manifest().expect("vendored manifest is readable"))
        .expect("manifest generates cleanly");
    let checked_in = std::fs::read_to_string(tables_path()).expect("spec_tables.rs is readable");

    assert_eq!(
        checked_in,
        generated,
        "\n{} is out of date with {}.\n\
         Regenerate: cargo run --manifest-path spec-codegen/Cargo.toml\n",
        tables_path().display(),
        manifest_path().display(),
    );
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
