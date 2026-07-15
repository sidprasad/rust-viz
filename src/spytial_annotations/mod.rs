//! # SpyTial Annotations
//!
//! Compile-time decorator system for SpyTial spatial layout and visualization.
//! Provides type-level decorator collection via derive macros and YAML serialization.

/// Decorator data types, derive-macro runtime, and YAML serialization plumbing.
pub mod runtime;

// Re-export the main types and functions
pub use runtime::{
    get_type_decorators, register_type_decorators, to_yaml, AlignConstraint, AlignParams,
    BorderStyle, Constraint, CyclicConstraint, CyclicParams, DecoProbe, DefaultDecorators,
    Directive, FillStyle, GroupConstraint, GroupEdge, GroupEdgePoints, GroupEdgeValue, GroupParams,
    HasSpytialDecorators, LinePattern, LineStyle, OrientationConstraint, OrientationParams,
    SpytialDecorators, SpytialDecoratorsBuilder, TextSize, TextStyle,
};
