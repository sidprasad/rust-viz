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

/// A package nested under `.claude/worktrees/` has to resolve standalone.
///
/// Claude Code parks git worktrees there, physically inside this checkout. A
/// worktree on a branch that predates the `[workspace]` section has none of
/// its own, so cargo walks up from the nested package, lands on this crate's
/// manifest, and — if the path is neither member nor excluded — refuses to
/// build the worktree at all: "current package believes it's in a workspace
/// when it's not". The `exclude` entry is what keeps such worktrees buildable.
///
/// Exercised with a throwaway package rather than by reading the manifest,
/// for the same reason as above: exclusion is resolution behavior (it covers
/// everything beneath the excluded directory), and only cargo can say whether
/// it applies.
#[test]
fn packages_under_claude_worktrees_resolve_standalone() {
    let probe = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".claude/worktrees")
        .join(format!("exclusion-probe-{}", std::process::id()));

    struct RemoveOnDrop(std::path::PathBuf);
    impl Drop for RemoveOnDrop {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    std::fs::create_dir_all(probe.join("src")).expect("create probe package dir");
    let _cleanup = RemoveOnDrop(probe.clone());
    std::fs::write(
        probe.join("Cargo.toml"),
        "[package]\nname = \"exclusion-probe\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write probe manifest");
    std::fs::write(probe.join("src/lib.rs"), "").expect("write probe lib.rs");

    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(&probe)
        .output()
        .expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "a package under .claude/worktrees/ no longer resolves standalone — \
         is `.claude/worktrees` missing from this workspace's `exclude`?\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
}
