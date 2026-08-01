//! Regenerate `macros/src/spec_tables.rs` from the vendored manifest.
//!
//! ```text
//! cargo run --manifest-path spec-codegen/Cargo.toml
//! ```
//!
//! Pass `--check` to verify the checked-in file is current without writing it —
//! that is what the drift test and CI do.

use spytial_spec_codegen::{generate, tables_path, vendored_manifest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let check_only = std::env::args().any(|a| a == "--check");
    let generated = generate(&vendored_manifest()?)?;
    let path = tables_path();

    if check_only {
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current != generated {
            eprintln!(
                "{} is out of date with templates/vendor/spytial-language.json.\n\
                 Regenerate: cargo run --manifest-path spec-codegen/Cargo.toml",
                path.display()
            );
            std::process::exit(1);
        }
        println!("{} is up to date.", path.display());
        return Ok(());
    }

    std::fs::write(&path, &generated)?;
    println!("wrote {} ({} bytes)", path.display(), generated.len());
    Ok(())
}
