//! CI driver for the shared serde-data-model corpus.
//!
//! The corpus itself is this crate's `src/lib.rs`: all 29 types of the
//! [serde data model](https://serde.rs/data-model.html), concrete values for
//! each, and the two oracles — **R-eq** (`v == from_datum(export(v))`) and
//! **R-inspect** (`format!("{:?}", v) == replit(export(v))`). The literate
//! report at `../../reify-eval/rust.ipynb` depends on the very same crate, so
//! this suite and that report cannot drift apart.
//!
//! The suite lives here rather than in `spytial`'s own `tests/` so that
//! `spytial` needs nothing from this directory to build, test, or package.
//!
//! Two tests: [`round_trip`] checks every case, [`coverage_is_total`] checks
//! that every serde category has one. Run with `--nocapture` to print the
//! coverage table.
//!
//! `spytial`'s `tests/reify.rs` covers the same machinery from the other
//! direction — it is organized by Rust-level shape and pins specific
//! regressions. This corpus is organized by serde category and is the one that
//! must stay exhaustive.

use spytial_eval_corpus::{cases, SERDE_DATA_MODEL};

#[test]
fn round_trip() {
    let cases = cases();
    let mut failures: Vec<String> = Vec::new();
    let mut rows: Vec<(&str, String, char, String)> = Vec::new();

    for case in &cases {
        // `stringify!` keeps the source's line breaks; the table wants one line.
        let expr = case.expr.split_whitespace().collect::<Vec<_>>().join(" ");
        match (case.run)() {
            Ok(printed) => rows.push((case.category, expr, case.support.mark(), printed)),
            Err(why) => {
                failures.push(format!("[{}] {}: {}", case.category, expr, why));
                rows.push((case.category, expr, '!', why));
            }
        }
    }

    print_table(&rows);

    assert!(
        failures.is_empty(),
        "{} of {} cases failed:\n  {}",
        failures.len(),
        cases.len(),
        failures.join("\n  ")
    );
}

/// Every serde category is exercised, and no case is filed under a category
/// that is not in the data model. Guards the corpus against silently shrinking.
#[test]
fn coverage_is_total() {
    let cases = cases();

    let missing: Vec<&str> = SERDE_DATA_MODEL
        .iter()
        .copied()
        .filter(|c| !cases.iter().any(|case| case.category == *c))
        .collect();
    assert!(
        missing.is_empty(),
        "serde data model categories with no case: {missing:?}"
    );

    let unknown: Vec<&str> = cases
        .iter()
        .map(|case| case.category)
        .filter(|c| !SERDE_DATA_MODEL.contains(c))
        .collect();
    assert!(
        unknown.is_empty(),
        "cases filed under categories outside the serde data model: {unknown:?}"
    );
}

/// Print the coverage table, in `SERDE_DATA_MODEL` order. Visible under
/// `cargo test --test serde_data_model -- --nocapture`.
fn print_table(rows: &[(&str, String, char, String)]) {
    let width = rows.iter().map(|(_, e, _, _)| e.len()).max().unwrap_or(0);
    println!(
        "\nserde data model coverage  \
         (= both oracles, e R-eq only, i R-inspect only, ! failed)\n"
    );
    for category in SERDE_DATA_MODEL {
        let mut first = true;
        for (_, expr, mark, printed) in rows.iter().filter(|(c, _, _, _)| *c == category) {
            let label = if first { category } else { "" };
            println!("  {label:<16} {mark} {expr:<width$}  {printed}");
            first = false;
        }
    }
    println!();
}
