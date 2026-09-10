//! Randomized driver for the shared serde-data-model corpus.
//!
//! `serde_data_model.rs` runs the curated list: one value per category, plus
//! the extremes worth naming. This suite runs the search — a proptest
//! generator over [`Nest`], whose variants cover the same 29 categories but
//! nest them inside each other to arbitrary depth. Between them they answer
//! two different questions. The list answers "is every category covered?".
//! The generator answers "does coverage survive composition?".
//!
//! Four tests: [`oracles_hold`] is the property over a wide scalar domain,
//! [`oracles_hold_under_collisions`] is the same property over a domain narrow
//! enough that atom interning is forced to collide, [`inspect_holds_with_nan`]
//! covers values R-eq cannot judge, and [`generated_coverage_is_total`] proves
//! the generator can still reach all 29 categories — the guarantee
//! randomization takes away from a fixed list.

use std::collections::BTreeSet;

use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{FileFailurePersistence, TestCaseError, TestRunner};

use spytial_eval_corpus::pbt::{nest, Floats, Pool};
use spytial_eval_corpus::{inspect_only, parity, SERDE_DATA_MODEL};

proptest! {
    // 1024 rather than proptest's default 256: the narrow-domain property
    // finds an interning collision within a few dozen cases, but the wide one
    // has to compose a repeat out of a large domain and needs the runway.
    // `Direct` because the default persistence policy looks for a sibling
    // `src/`, which a file under `tests/` does not have, and warns on every run.
    #![proptest_config(ProptestConfig {
        cases: 1024,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "tests/pbt.proptest-regressions",
        ))),
        ..ProptestConfig::default()
    })]

    /// R-eq and R-inspect, over arbitrary nestings of every serde category.
    #[test]
    fn oracles_hold(v in nest(Floats::Comparable, Pool::Wide)) {
        if let Err(why) = parity(v) {
            return Err(TestCaseError::fail(why));
        }
    }

    /// The same property over a two-value-per-type domain, where the same
    /// scalar is near-certain to appear twice in one datum. Export interns
    /// primitives by value, so this is the setting in which a coarse
    /// interning key turns into a wrong reconstruction.
    #[test]
    fn oracles_hold_under_collisions(v in nest(Floats::Comparable, Pool::Narrow)) {
        if let Err(why) = parity(v) {
            return Err(TestCaseError::fail(why));
        }
    }

    /// R-inspect alone, for values that may contain NaN. R-eq is meaningless
    /// there — IEEE 754 makes `PartialEq` non-reflexive — but reproducing the
    /// printed output is still required.
    #[test]
    fn inspect_holds_with_nan(v in nest(Floats::WithNan, Pool::Wide)) {
        if let Err(why) = inspect_only(v) {
            return Err(TestCaseError::fail(why));
        }
    }
}

/// The generator reaches every serde category, and invents none.
///
/// A fixed list gets total coverage by construction; a generator only gets it
/// with positive probability, so it has to be measured. Sampling is seeded
/// (`TestRunner::deterministic`), so this test does not flake.
#[test]
fn generated_coverage_is_total() {
    const DRAWS: usize = 2_000;

    let strategy = nest(Floats::Comparable, Pool::Wide);
    let mut runner = TestRunner::deterministic();
    let mut seen: BTreeSet<&'static str> = BTreeSet::new();

    for _ in 0..DRAWS {
        let value = strategy
            .new_tree(&mut runner)
            .expect("generator produced no value")
            .current();
        seen.extend(value.categories());
    }

    let missing: Vec<&str> = SERDE_DATA_MODEL
        .iter()
        .copied()
        .filter(|c| !seen.contains(c))
        .collect();
    assert!(
        missing.is_empty(),
        "{DRAWS} draws never produced these serde categories: {missing:?}"
    );

    let unknown: Vec<&str> = seen
        .iter()
        .copied()
        .filter(|c| !SERDE_DATA_MODEL.contains(c))
        .collect();
    assert!(
        unknown.is_empty(),
        "generator claims categories outside the data model: {unknown:?}"
    );
}
