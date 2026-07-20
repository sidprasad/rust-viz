//! Shared value corpus for the cross-language Spytial reify evaluation.
//!
//! One question, asked once per host language: does relationalizing a value
//! preserve enough structure to reproduce the language's own inspection
//! output? Rust exposes structure through `serde::Serialize`, so the shapes
//! Spytial can possibly see are exactly the 29 types of the
//! [serde data model](https://serde.rs/data-model.html). [`SERDE_DATA_MODEL`]
//! names all 29; [`cases`] exercises each with concrete values; two oracles
//! judge every case:
//!
//! * **R-eq** — `v == from_datum(export(v))`. The datum reconstructs the value.
//! * **R-inspect** — `format!("{:?}", v) == replit(export(v))`. The datum
//!   reproduces Rust's textual inspection output.
//!
//! This crate exists so its two consumers cannot drift apart:
//! `../tests/serde_data_model.rs` enforces the corpus in CI, and
//! `../../reify-eval/rust.ipynb` imports the very same crate to narrate the
//! results. It is unpublished (`publish = false`) and deliberately outside the
//! `spytial` crate, so evaluation machinery never reaches the published API.

#![deny(missing_docs)]

use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use spytial::export::try_export_json_instance;
use spytial::{from_datum, replit};

/// The serde data model: 29 types, the complete set of structural categories a
/// `Serialize` impl can express. Every entry must appear in [`cases`] —
/// `../tests/serde_data_model.rs` asserts exactly that.
pub const SERDE_DATA_MODEL: [&str; 29] = [
    "bool",
    "i8",
    "i16",
    "i32",
    "i64",
    "i128",
    "u8",
    "u16",
    "u32",
    "u64",
    "u128",
    "f32",
    "f64",
    "char",
    "string",
    "byte array",
    "option",
    "unit",
    "unit_struct",
    "unit_variant",
    "newtype_struct",
    "newtype_variant",
    "seq",
    "tuple",
    "tuple_struct",
    "tuple_variant",
    "map",
    "struct",
    "struct_variant",
];

// ──────────────────────────────────────────────
// Oracles
// ──────────────────────────────────────────────

/// Which oracles a case is expected to satisfy.
///
/// The two oracles have complementary blind spots, and the values that expose
/// them are unrelated to the relational form: `HashMap` defeats R-inspect
/// because its `{:?}` is iteration-order-dependent, `NaN` defeats R-eq because
/// IEEE 754 makes `PartialEq` non-reflexive. Each is still checked by the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    /// R-eq and R-inspect both hold.
    Parity,
    /// R-eq only — `{:?}` has no single correct rendering (`HashMap`).
    EqOnly,
    /// R-inspect only — the value is not `==` to itself (`NaN`).
    InspectOnly,
}

impl Support {
    /// One-character mark for compact tables: `=` both oracles, `e` R-eq only,
    /// `i` R-inspect only.
    pub fn mark(self) -> char {
        match self {
            Support::Parity => '=',
            Support::EqOnly => 'e',
            Support::InspectOnly => 'i',
        }
    }

    /// Which oracle(s) apply, in words.
    pub fn describe(self) -> &'static str {
        match self {
            Support::Parity => "both oracles",
            Support::EqOnly => "R-eq only",
            Support::InspectOnly => "R-inspect only",
        }
    }
}

/// One corpus entry: a value, the serde category it exercises, and the oracles
/// it must satisfy.
pub struct Case {
    /// The serde data model category this case exercises — always one of
    /// [`SERDE_DATA_MODEL`].
    pub category: &'static str,
    /// The value under test, as written in source.
    pub expr: &'static str,
    /// Which oracles apply.
    pub support: Support,
    /// Runs the applicable oracles, returning the reconstructed rendering on
    /// success and a diagnostic on failure.
    pub run: fn() -> Result<String, String>,
}

/// R-eq + R-inspect.
pub fn parity<T>(v: T) -> Result<String, String>
where
    T: Serialize + DeserializeOwned + Debug + PartialEq,
{
    let datum = try_export_json_instance(&v).map_err(|e| format!("export failed: {e}"))?;
    let back: T = from_datum(&datum).map_err(|e| format!("R-eq: from_datum failed: {e}"))?;
    if back != v {
        return Err(format!("R-eq: {:?} != {:?}", v, back));
    }
    let printed = replit::<T>(&datum).map_err(|e| format!("R-inspect: replit failed: {e}"))?;
    let expected = format!("{:?}", v);
    if printed != expected {
        return Err(format!("R-inspect: expected {expected}, got {printed}"));
    }
    Ok(printed)
}

/// R-eq only — for containers whose `{:?}` is iteration-order-dependent.
pub fn eq_only<T>(v: T) -> Result<String, String>
where
    T: Serialize + DeserializeOwned + Debug + PartialEq,
{
    let datum = try_export_json_instance(&v).map_err(|e| format!("export failed: {e}"))?;
    let back: T = from_datum(&datum).map_err(|e| format!("R-eq: from_datum failed: {e}"))?;
    if back != v {
        return Err(format!("R-eq: {:?} != {:?}", v, back));
    }
    Ok(format!("{:?}", back))
}

/// R-inspect only — for values that are not `==` to themselves.
pub fn inspect_only<T>(v: T) -> Result<String, String>
where
    T: Serialize + DeserializeOwned + Debug,
{
    let datum = try_export_json_instance(&v).map_err(|e| format!("export failed: {e}"))?;
    let printed = replit::<T>(&datum).map_err(|e| format!("R-inspect: replit failed: {e}"))?;
    let expected = format!("{:?}", v);
    if printed != expected {
        return Err(format!("R-inspect: expected {expected}, got {printed}"));
    }
    Ok(printed)
}

// ──────────────────────────────────────────────
// Types exercising the compound categories
// ──────────────────────────────────────────────

/// Exercises `unit_struct`.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct UnitStruct;

/// Exercises `newtype_struct`.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Meters(
    /// The wrapped length.
    pub f64,
);

/// Exercises `tuple_struct`.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Pair(
    /// First component.
    pub i32,
    /// Second component.
    pub i32,
);

/// Exercises `struct` with primitive fields.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: i32,
    /// Vertical coordinate.
    pub y: i32,
}

/// Exercises `struct` nesting.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Line {
    /// Start point.
    pub start: Point,
    /// End point.
    pub end: Point,
}

/// Exercises `struct` with no fields.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Empty {}

/// Exercises all four enum variant shapes.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum Shape {
    /// `unit_variant`.
    Empty,
    /// `newtype_variant`.
    Radius(f64),
    /// `tuple_variant`.
    Offset(i32, i32),
    /// `struct_variant`.
    Rect {
        /// Width.
        w: i32,
        /// Height.
        h: i32,
    },
}

/// A hand-written `Debug` — R-inspect has to reproduce this too, not just the
/// derived form.
#[derive(Serialize, Deserialize, PartialEq)]
pub struct Temperature {
    /// Degrees Celsius.
    pub celsius: f64,
}

impl Debug for Temperature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}°C", self.celsius)
    }
}

/// Exercises `byte array`: `Vec<u8>` serializes as a *seq* by default; only
/// `serialize_bytes` reaches the byte-array category, which is what the
/// `#[serde(with)]` shim forces.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Bytes(
    /// The raw bytes.
    #[serde(with = "byte_string")]
    pub Vec<u8>,
);

mod byte_string {
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(v)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        use serde::de::{Error, Visitor};

        struct ByteVisitor;

        impl<'de> Visitor<'de> for ByteVisitor {
            type Value = Vec<u8>;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a byte string")
            }

            fn visit_bytes<E: Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                Ok(v.to_vec())
            }

            fn visit_byte_buf<E: Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
                Ok(v)
            }
        }

        d.deserialize_byte_buf(ByteVisitor)
    }
}

// ──────────────────────────────────────────────
// The cases
// ──────────────────────────────────────────────

/// Shorthand: `case!("i32", 42_i32)` → a [`Case`] whose `expr` is the literal
/// source text, so coverage tables show what was actually tested.
macro_rules! case {
    ($category:literal, $value:expr) => {
        Case {
            category: $category,
            expr: stringify!($value),
            support: Support::Parity,
            run: || parity($value),
        }
    };
    ($category:literal, $value:expr, eq_only) => {
        Case {
            category: $category,
            expr: stringify!($value),
            support: Support::EqOnly,
            run: || eq_only($value),
        }
    };
    ($category:literal, $value:expr, inspect_only) => {
        Case {
            category: $category,
            expr: stringify!($value),
            support: Support::InspectOnly,
            run: || inspect_only($value),
        }
    };
}

/// The corpus: concrete values covering every category in
/// [`SERDE_DATA_MODEL`], including extremes (integer bounds, `-0.0`,
/// infinities, `NaN`), quoting hazards, empty containers, nested options, and
/// cross-category composition.
pub fn cases() -> Vec<Case> {
    vec![
        // -- scalars ------------------------------------------------------
        case!("bool", true),
        case!("bool", false),
        case!("i8", 0_i8),
        case!("i8", i8::MIN),
        case!("i8", i8::MAX),
        case!("i16", i16::MIN),
        case!("i16", i16::MAX),
        case!("i32", 42_i32),
        case!("i32", -7_i32),
        case!("i32", i32::MIN),
        case!("i64", i64::MIN),
        case!("i64", i64::MAX),
        case!("i128", 0_i128),
        case!("i128", i128::MIN),
        case!("i128", i128::MAX),
        case!("u8", 0_u8),
        case!("u8", u8::MAX),
        case!("u16", u16::MAX),
        case!("u32", u32::MAX),
        case!("u64", u64::MAX),
        case!("u128", 0_u128),
        case!("u128", u128::MAX),
        // -- floats: parsing the label must not perturb the bit pattern ---
        case!("f32", -0.25_f32),
        case!("f32", f32::MIN),
        case!("f64", 3.0_f64),
        case!("f64", 3.5_f64),
        case!("f64", -0.0_f64),
        case!("f64", f64::MIN_POSITIVE),
        case!("f64", f64::INFINITY),
        case!("f64", f64::NEG_INFINITY),
        case!("f64", f64::NAN, inspect_only),
        // -- text: labels are raw, so escaping is Debug's job, not ours ---
        case!("char", 'z'),
        case!("char", '💡'),
        case!("char", '\n'),
        case!("char", '\''),
        case!("string", "hello".to_string()),
        case!("string", String::new()),
        case!("string", "a\"b\nc\\d".to_string()),
        case!("string", "λ 💡 中文".to_string()),
        case!("byte array", Bytes(vec![1, 2, 3])),
        case!("byte array", Bytes(vec![])),
        case!("byte array", Bytes(vec![0, 255])),
        // -- option: the Some-wrapper rule (see export::serialize_some) ---
        case!("option", Some(5_i32)),
        case!("option", Option::<i32>::None),
        case!("option", Some(Option::<i32>::None)),
        case!("option", Some(Some(5_i32))),
        case!("option", Some(Some(Option::<i32>::None))),
        // -- nullary shapes: all interned as singletons -------------------
        case!("unit", ()),
        case!("unit_struct", UnitStruct),
        case!("unit_variant", Shape::Empty),
        // -- wrappers -----------------------------------------------------
        case!("newtype_struct", Meters(2.5)),
        case!("newtype_variant", Shape::Radius(1.5)),
        case!("newtype_variant", Shape::Radius(f64::NAN), inspect_only),
        // -- sequences: idx relations -------------------------------------
        case!("seq", vec![1_i32, 2, 3]),
        case!("seq", Vec::<i32>::new()),
        case!("seq", vec![1_i32, 1, 1]),
        case!("seq", vec![vec![1_i32], vec![], vec![2, 3]]),
        case!("seq", vec![Some(1_i32), None, Some(3)]),
        // -- tuples: fixed-size arrays serialize as tuples too ------------
        case!("tuple", (1_i32, "x".to_string(), true)),
        case!("tuple", (1_i32,)),
        case!("tuple", [1_i32, 2, 3]),
        case!("tuple_struct", Pair(3, 4)),
        case!("tuple_variant", Shape::Offset(2, 3)),
        // -- maps: BTreeMap has a deterministic {:?}, HashMap does not ----
        case!("map", BTreeMap::from([("a".to_string(), 1_i32)])),
        case!("map", BTreeMap::<String, i32>::new()),
        case!("map", BTreeMap::from([(1_i32, vec!["x".to_string()])])),
        case!(
            "map",
            HashMap::from([("a".to_string(), 1_i32), ("b".to_string(), 2)]),
            eq_only
        ),
        // -- structs ------------------------------------------------------
        case!("struct", Point { x: 1, y: 2 }),
        case!("struct", Empty {}),
        case!(
            "struct",
            Line {
                start: Point { x: 1, y: 2 },
                end: Point { x: 3, y: 4 },
            }
        ),
        case!("struct", Temperature { celsius: 21.5 }),
        case!("struct_variant", Shape::Rect { w: 10, h: 5 }),
        // -- composition: categories nested inside each other -------------
        case!(
            "struct",
            Point {
                x: i32::MAX,
                y: i32::MIN
            }
        ),
        case!(
            "seq",
            vec![
                Shape::Empty,
                Shape::Radius(2.0),
                Shape::Offset(1, 2),
                Shape::Rect { w: 1, h: 1 },
            ]
        ),
    ]
}
