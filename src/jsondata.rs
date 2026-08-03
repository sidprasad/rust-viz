//! Relational JSON data model produced by [`crate::export_json_instance`].
//!
//! Every Rust value spytial visualizes is flattened into this shape:
//! a flat list of [`IAtom`](crate::jsondata::IAtom) nodes plus a flat list of
//! [`IRelation`](crate::jsondata::IRelation) edges grouped by name. The same
//! shape is what spytial-core consumes on the JavaScript side — these structs
//! are part of the public, stable API.

use serde::{Deserialize, Serialize};

/// A relational instance: the full set of atoms (nodes) and relations (edges)
/// extracted from a single Rust value.
///
/// Serialized as JSON in the HTML template and consumed by spytial-core's
/// `JSONDataInstance` constructor in the browser.
///
/// # Root atom
///
/// Atoms are stored in serialization order, so **`atoms[0]` is the root** — the
/// atom for the top-level value — because [`export_json_instance`] emits a
/// container/struct atom before recursing into its children. Reconstruction
/// relies on this: [`from_datum`] starts at `atoms[0]`, and [`from_datum_root`]
/// takes an explicit id for callers that build or reorder an instance themselves.
///
/// There is intentionally **no `rootId` field**. For `export` output it would be
/// redundant (the root is always `atoms[0]`, and the data is acyclic so the root
/// is also recoverable as the atom that no relation targets), and it would not
/// survive a spytial-core round-trip anyway, since unknown JSON keys are dropped.
/// A future producer that needs an explicit root should pass it to
/// [`from_datum_root`] rather than rely on a field that silently disappears.
///
/// [`export_json_instance`]: crate::export_json_instance
/// [`from_datum`]: crate::from_datum
/// [`from_datum_root`]: crate::from_datum_root
#[derive(Serialize, Deserialize, Debug)]
pub struct JsonDataInstance {
    /// All atoms (graph nodes), in serialization order — `atoms[0]` is the root
    /// (see the "Root atom" note on [`JsonDataInstance`]).
    pub atoms: Vec<IAtom>,
    /// All relations (edges), grouped by relation name.
    pub relations: Vec<IRelation>,
}

/// A single atom — one node in the relational graph.
///
/// Atoms are created from struct instances, collection containers (sequence,
/// tuple, map), and primitive leaves. `id` is unique within the instance,
/// `type` is the Rust type name (e.g. `"Person"`, `"i32"`, `"sequence"`),
/// and `label` is the human-readable text shown in the diagram.
#[derive(Serialize, Deserialize, Debug)]
pub struct IAtom {
    /// Unique identifier within the enclosing [`JsonDataInstance`].
    pub id: String,
    /// Type name (e.g. struct name or `"sequence"`/`"tuple"`/`"map"`/`"i32"`).
    pub r#type: String,
    /// Human-readable label shown on the diagram.
    pub label: String,
}

/// A single tuple within a relation: the participating atoms and the type
/// of each position.
#[derive(Serialize, Deserialize, Debug)]
pub struct ITuple {
    /// Atom IDs in this tuple, in position order.
    pub atoms: Vec<String>,
    /// Type names of the atoms in this tuple, parallel to [`Self::atoms`].
    pub types: Vec<String>,
}

/// A relation — a named, typed edge set. Relations are keyed by name in a
/// single flat namespace, so same-named edges from different source types
/// (two structs that each have a `name` field, say) share one relation.
///
/// Examples: a field relation `name(Person, atom)`, a sequence relation
/// `idx(sequence, index, atom)`, or a map relation `map_entry(map, atom, atom)`.
/// The target position is always the literal `"atom"`, the universal type.
///
/// Tuples normally share one arity. The exception is a struct field that
/// shares its name with a built-in relation of different arity (a field
/// literally named `idx` or `map_entry`): its tuples land in the built-in's
/// relation, mixing arities. Per-tuple [`ITuple::types`] stays exact either
/// way, and tuples are ordered longest-arity-first so that [`Self::types`]
/// always has the first tuple's arity — spytial-core's normalizer discards
/// any header whose length differs from it.
#[derive(Serialize, Deserialize, Debug)]
pub struct IRelation {
    /// Stable identifier for the relation (currently the same as [`Self::name`]).
    pub id: String,
    /// Relation name — `"idx"`, `"map_entry"`, or a struct field name.
    pub name: String,
    /// Position types for the relation as a whole: the position-wise join of
    /// every tuple's [`ITuple::types`]. A position on which all tuples agree
    /// keeps its concrete type; one that varies is widened to `"atom"`. If
    /// tuple arities differ, the header has the longest arity that occurs.
    pub types: Vec<String>,
    /// All tuples belonging to this relation.
    pub tuples: Vec<ITuple>,
}
