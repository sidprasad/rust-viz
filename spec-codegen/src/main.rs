//! Regenerate `macros/src/spec_tables.rs` from the vendored manifest.
//!
//! ```text
//! cargo run --manifest-path spec-codegen/Cargo.toml
//! ```
//!
//! Pass `--check` to verify the checked-in file is current without writing it —
//! that is what the drift test and CI do.

use spytial_spec_codegen::{
    first_difference, generate, generate_reference, reference_path, tables_path, vendored_manifest,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let check_only = std::env::args().any(|a| a == "--check");
    let manifest = vendored_manifest()?;
    let outputs = [
        (tables_path(), generate(&manifest)?),
        (reference_path(), generate_reference(&manifest)?),
    ];

    if check_only {
        let mut stale = false;
        for (path, generated) in &outputs {
            let current = std::fs::read_to_string(path).unwrap_or_default();
            if let Some(diff) = first_difference(&current, generated) {
                eprintln!(
                    "{} is out of date with templates/vendor/spytial-language.json.\n{}\n",
                    path.display(),
                    diff,
                );
                stale = true;
            }
        }
        if stale {
            eprintln!("Regenerate: cargo run --manifest-path spec-codegen/Cargo.toml");
            std::process::exit(1);
        }
        println!("generated files are up to date.");
        return Ok(());
    }

    for (path, generated) in &outputs {
        std::fs::write(path, generated)?;
        println!("wrote {} ({} bytes)", path.display(), generated.len());
    }
    Ok(())
}
