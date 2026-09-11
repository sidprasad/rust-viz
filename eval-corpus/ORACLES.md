# Oracles: where R-inspect fails

This document defines the two oracles used by the reify evaluation. It lists
where each oracle fails. It states which failures are defects in Spytial and
which failures are not.

## 1. Definitions

Let `v` be a Rust value. Let `E` be export. Let `R` be reify. Both oracles
compute `v' = R(E(v))`.

| Oracle | Test |
| --- | --- |
| R-eq | `v' == v` |
| R-inspect | `format!("{:?}", v')` equals `format!("{:?}", v)` |

R-eq compares values. R-inspect compares text.

## 2. The claim

The evaluation makes one claim:

> The datum keeps all information that the host language's printed output shows.

R-inspect tests this claim directly. R-eq supports it.

String equality is a sufficient test for the claim. String equality is not a
necessary test. If the two strings are equal, the datum kept the information.
If the two strings differ, there are two possible causes:

1. The datum lost information.
2. The value does not determine the string.

Section 4 separates these two causes.

## 3. String equality is too strict

`HashMap` shows the problem. The measurement below builds two maps with
identical contents. Both maps are built in one process and one thread.

```text
a == b               : true
Debug(a)             : {"k0": 0, "k2": 2, "k6": 6, "k7": 7, "k4": 4, ...}
Debug(b)             : {"k7": 7, "k2": 2, "k6": 6, "k5": 5, "k1": 1, ...}
Debug(a) == Debug(b) : false
```

`Debug` reads a hash seed. The hash seed is not part of the value. Therefore
`Debug` is not a function of the value for `HashMap`. One `HashMap` value has
many correct renderings.

R-inspect compares one rendering against another rendering. For `HashMap` this
comparison fails without any action by Spytial.

Entry order is not information about the value. Therefore no information is
lost. R-eq confirms this. The reconstructed map equals the original map.

Entry order is ignorable only when the type does not define an order:

- `BTreeMap` defines an order. `BTreeMap` has one rendering. `BTreeMap` passes
  R-inspect.
- `Vec` defines an order. A change of `Vec` order is a defect.

"Order does not matter" is a per-type statement. It is not a general rule.

## 4. Failure modes

Every failure observed so far is in one of four modes. Mode 1 is a defect in
Spytial. Modes 2, 3 and 4 are not.

| Mode | Cause | Example | Defect |
| --- | --- | --- | --- |
| 1 | The datum lost information | `[0.0, -0.0]` | Yes |
| 2 | The printer is not a function of the value | `HashMap`, `HashSet` | No |
| 3 | The information was lost before export | `#[serde(skip)]` | No |
| 4 | The comparator is not reflexive | `f64::NAN` | No |

### Mode 1: the datum lost information

The datum does not hold the information. No view over the datum can recover it.

```text
expected  [0.0, -0.0]
got       [0.0, 0.0]
```

Export interned float atoms by `==`. Rust reports `0.0 == -0.0` as true. Both
values therefore shared one atom. The second value took the label of the first
value.

R-eq passed this case, because `0.0 == -0.0`. Only R-inspect failed. The fix
keys float atoms by bit pattern.

### Mode 2: the printer is not a function of the value

The value has more than one correct rendering. Section 3 gives the evidence.
These cases are judged by R-eq only.

```text
expected  {"k0": 0, "k2": 2, "k6": 6, ...}
got       {"k0": 0, "k6": 6, "k7": 7, ...}
```

The entries are the same. The order differs. R-eq passes.

### Mode 3: the information was lost before export

`#[serde(skip)]` tells serde not to expose the field. Export never receives the
field. Both oracles fail.

```text
expected  Skipped { kept: 1, dropped: 7 }
got       Skipped { kept: 1, dropped: 0 }
```

The information is gone. The cause is the chosen inspection mechanism. The
cause is not the relational form.

Use this control to classify a suspected mode 3 failure: serialize the same
value with `serde_json`. If `serde_json` also loses the information, the loss
is in the mechanism. If `serde_json` keeps the information, the failure is
mode 1.

### Mode 4: the comparator is not reflexive

`f64::NAN` is not equal to itself. R-eq fails on a correct reconstruction.

```text
R-eq       FAIL    NaN != NaN
R-inspect  PASS    "NaN" == "NaN"
```

Mode 4 blinds R-eq. Mode 2 blinds R-inspect. Each oracle covers the blind spot
of the other oracle. The corpus runs both oracles for this reason.

## 5. Exemption rule

Modes 2 and 4 need a per-case marker that says which oracles apply. The same
marker can hide a mode 1 defect. The following rule prevents this.

> A case may waive an oracle only with a **witness**. A witness is a runnable
> demonstration of the source-language behaviour that makes the oracle
> ill-posed. A witness must not refer to Spytial. If the witness fails, the
> exemption is void.

The signed-zero defect had no available witness. Rust prints `[0.0, -0.0]`
one way only. The case was therefore a defect and not an exemption.

Each exemption waives one oracle. The other oracle still judges the case. No
case escapes both oracles.

## 6. Current exemptions

Three exemptions in 102 hand-written cases.

| Case | Source | Waives | Witness |
| --- | --- | --- | --- |
| `HashMap::from([..])` | `src/lib.rs:411` | R-inspect | `a == b && Debug(a) != Debug(b)` |
| `f64::NAN` | `src/lib.rs:365` | R-eq | `f64::NAN != f64::NAN` |
| `Shape::Radius(f64::NAN)` | `src/lib.rs:391` | R-eq | `f64::NAN != f64::NAN` |

The witnesses are not yet encoded as tests.

## 7. Measured results

Measured on branch `reify-pbt-and-signed-zero`.

| Value | R-eq | R-inspect | Mode |
| --- | --- | --- | --- |
| `BTreeMap` | pass | pass | none |
| `#[serde(rename)]` | pass | pass | none |
| `#[serde(rename_all)]` | pass | pass | none |
| `#[serde(skip_serializing_if)]`, present | pass | pass | none |
| `#[serde(skip_serializing_if)]`, absent | pass | pass | none |
| `PhantomData` | pass | pass | none |
| enum discriminant | pass | pass | none |
| `HashMap`, `HashSet` | pass | **fail** | 2 |
| `f64::NAN` | **fail** | pass | 4 |
| `#[serde(skip)]` | **fail** | **fail** | 3 |

Corpus size: 75 data-model cases, 27 serde-representation cases.

## 8. Statements for the paper

Use this form of the claim:

> For every value in the corpus, the Spytial datum determines the host's
> printed rendering, up to choices that the host leaves undetermined.

The final clause is checkable. Each instance of "undetermined" carries a
witness under the rule in section 5.

Two further statements:

1. R-inspect is the stronger oracle for this claim. `Debug` is written by
   rustc. R-eq is a round trip through code written for this project at both
   ends. R-eq is only as sharp as `PartialEq`. The signed-zero defect passed
   R-eq.
2. The datum may hold more than the printed output holds. Sharing, identity
   and internal structure may be present and hidden by a view. Only the
   reverse direction is a defect. Only R-inspect tests the reverse direction.

## 9. Related files

| File | Content |
| --- | --- |
| `src/lib.rs` | Oracles, `Support` markers, 75 data-model cases |
| `tests/serde_data_model.rs` | Runs the data-model cases |
| `tests/serde_attributes.rs` | 27 serde-representation cases |
| `src/pbt.rs` | Property-test generator |
| `tests/pbt.rs` | Property tests and category coverage |
