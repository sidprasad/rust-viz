//! Guards the build configuration that decides what actually gets tested.
//!
//! This lives in the root crate on purpose. A test inside `macros/` could not
//! do the job: if that crate stops being a workspace member, `cargo test
//! --workspace` skips it, so a guard placed there would never run and never
//! fail — the same silence it is meant to catch.

use std::process::Command;

/// `macros` has to resolve as a workspace member, not merely as a path
/// dependency.
///
/// A package with no `[workspace]` section is a workspace of one: `cargo
/// metadata` reports only `spytial`, and both `cargo test` and `cargo test
/// --workspace` skip the derive macro entirely — its unit tests and its doc
/// example included. That is how a doc example which could never compile sat
/// green in CI: nothing ever ran it.
///
/// Asked of `cargo metadata` rather than of the manifest text, because the
/// resolved workspace is what decides which crates run. Reading the manifest
/// would also get the rule wrong in both directions: once the section exists
/// path dependencies join whether listed or not, and a comment mentioning
/// `[workspace]` is enough to fool a parser.
#[test]
fn the_derive_macro_crate_is_a_workspace_member() {
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&out.stderr),
    );

    let meta: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("cargo metadata emits JSON");
    let members: Vec<&str> = meta["packages"]
        .as_array()
        .expect("metadata has packages")
        .iter()
        .filter_map(|p| p["name"].as_str())
        .collect();

    assert!(
        members.contains(&"spytial_export_macros"),
        "`spytial_export_macros` is not a workspace member — the workspace is {members:?}.\n\
         A path dependency is not a member: without it listed, `cargo test --workspace` \
         silently stops running the derive macro's unit tests and its doc example, \
         with no failure to notice.",
    );
}
