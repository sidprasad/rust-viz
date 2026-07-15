use serde::Serialize;
use spytial::spytial_annotations::{
    to_yaml, Constraint, Directive, GroupParams, HasSpytialDecorators,
    SpytialDecorators as SpytialDecoratorsType, SpytialDecoratorsBuilder,
};
use spytial::SpytialDecorators;

#[derive(Serialize, SpytialDecorators)]
#[align(selector = "peer", direction = "horizontal")]
#[orientation(selector = "peer", directions = ["right"])]
#[flag(name = "important")]
struct DerivedNode {
    id: u32,
}

#[test]
fn derive_macro_emits_align_and_existing_decorators() {
    let decorators = DerivedNode::decorators();

    assert!(decorators.constraints.iter().any(|constraint| {
        matches!(constraint, Constraint::Align(align)
            if align.align.selector == "peer" && align.align.direction == "horizontal")
    }));
    assert!(decorators.constraints.iter().any(|constraint| {
        matches!(constraint, Constraint::Orientation(orientation)
            if orientation.orientation.selector == "peer"
            && orientation.orientation.directions == vec!["right".to_string()])
    }));
    assert!(decorators.directives.iter().any(|directive| {
        matches!(directive, Directive::Flag(flag) if flag.flag == "important")
    }));

    let yaml = to_yaml(&decorators).unwrap();
    assert!(yaml.contains("align:"));
    assert!(yaml.contains("direction: horizontal"));
    assert!(yaml.contains("orientation:"));
    assert!(yaml.contains("flag: important"));
}

#[derive(Serialize, SpytialDecorators)]
#[tag(to_tag = "Person", name = "status", value = "Person.status")]
struct TaggedPerson {
    name: String,
    status: String,
}

#[test]
fn tag_directive_single() {
    let decorators = TaggedPerson::decorators();

    let tag = decorators
        .directives
        .iter()
        .find_map(|d| match d {
            Directive::Tag(t) => Some(t),
            _ => None,
        })
        .expect("expected a Tag directive");

    assert_eq!(tag.tag.to_tag, "Person");
    assert_eq!(tag.tag.name, "status");
    assert_eq!(tag.tag.value, "Person.status");

    let yaml = to_yaml(&decorators).unwrap();
    assert!(yaml.contains("tag:"));
    assert!(yaml.contains("toTag: Person"));
    assert!(yaml.contains("name: status"));
    assert!(yaml.contains("value: Person.status"));
}

#[derive(Serialize, SpytialDecorators)]
#[tag(to_tag = "Person", name = "age", value = "Person.age")]
#[tag(to_tag = "Car", name = "owner", value = "Car.ownedBy")]
struct MultiTagged {
    id: u32,
}

#[derive(Serialize, SpytialDecorators)]
#[edge_style(field = "left", line_style(color = "#000000"))]
struct EdgeStyledMinimal {
    id: u32,
}

#[test]
fn edge_style_directive_minimal() {
    let decorators = EdgeStyledMinimal::decorators();

    let edge = decorators
        .directives
        .iter()
        .find_map(|d| match d {
            Directive::EdgeStyle(e) => Some(&e.edge_style),
            _ => None,
        })
        .expect("expected an EdgeStyle directive");

    assert_eq!(edge.field, "left");
    let line = edge
        .line_style
        .as_ref()
        .expect("expected a lineStyle block");
    assert_eq!(line.color.as_deref(), Some("#000000"));
    assert!(line.pattern.is_none());
    assert!(line.weight.is_none());
    assert!(edge.text_style.is_none());
    assert!(edge.show_label.is_none());
    assert!(edge.hidden.is_none());
    assert!(edge.filter.is_none());
    assert!(edge.selector.is_none());

    let yaml = to_yaml(&decorators).unwrap();
    assert!(yaml.contains("edgeStyle:"));
    assert!(yaml.contains("field: left"));
    assert!(yaml.contains("lineStyle:"));
    assert!(yaml.contains("color: '#000000'"));
    // Optional leaves are skipped when None.
    assert!(!yaml.contains("pattern:"));
    assert!(!yaml.contains("weight:"));
    assert!(!yaml.contains("showLabel:"));
    assert!(!yaml.contains("hidden:"));
    assert!(!yaml.contains("textStyle:"));
}

#[derive(Serialize, SpytialDecorators)]
#[edge_style(
    field = "right",
    line_style(color = "blue", pattern = "dashed", weight = 2.5),
    text_style(size = "small", color = "gray"),
    show_label = false,
    hidden = true,
    filter = "Node3 -> Node1",
    selector = "Tree"
)]
struct EdgeStyledAllOptions {
    id: u32,
}

#[test]
fn edge_style_directive_all_options() {
    let decorators = EdgeStyledAllOptions::decorators();

    let edge = decorators
        .directives
        .iter()
        .find_map(|d| match d {
            Directive::EdgeStyle(e) => Some(&e.edge_style),
            _ => None,
        })
        .expect("expected an EdgeStyle directive");

    assert_eq!(edge.field, "right");
    let line = edge
        .line_style
        .as_ref()
        .expect("expected a lineStyle block");
    assert_eq!(line.color.as_deref(), Some("blue"));
    assert_eq!(
        line.pattern,
        Some(spytial::spytial_annotations::LinePattern::Dashed)
    );
    assert_eq!(line.weight, Some(2.5));
    let text = edge
        .text_style
        .as_ref()
        .expect("expected a textStyle block");
    assert_eq!(
        text.size,
        Some(spytial::spytial_annotations::TextSize::Small)
    );
    assert_eq!(text.color.as_deref(), Some("gray"));
    assert_eq!(edge.show_label, Some(false));
    assert_eq!(edge.hidden, Some(true));
    assert_eq!(edge.filter.as_deref(), Some("Node3 -> Node1"));
    assert_eq!(edge.selector.as_deref(), Some("Tree"));

    let yaml = to_yaml(&decorators).unwrap();
    assert!(yaml.contains("edgeStyle:"));
    assert!(yaml.contains("lineStyle:"));
    assert!(yaml.contains("pattern: dashed"));
    assert!(yaml.contains("weight: 2.5"));
    assert!(yaml.contains("textStyle:"));
    assert!(yaml.contains("size: small"));
    assert!(yaml.contains("showLabel: false"));
    assert!(yaml.contains("hidden: true"));
    assert!(yaml.contains("filter: Node3"));
}

#[derive(Serialize, SpytialDecorators)]
#[edge_style(
    field = "legacy_edge",
    value = "seagreen",
    style = "Dotted",
    weight = 3.0
)]
struct EdgeStyledLegacyFlat {
    id: u32,
}

#[test]
fn edge_style_legacy_flat_keys_desugar_to_blocks() {
    // The 2.x flat keys keep working and rewrite onto the 3.x blocks:
    // value -> lineStyle.color, style -> lineStyle.pattern (normalized like
    // spytial-core: trimmed + lowercased), weight -> lineStyle.weight.
    let decorators = EdgeStyledLegacyFlat::decorators();
    let yaml = to_yaml(&decorators).unwrap();
    assert!(yaml.contains("edgeStyle:"));
    assert!(!yaml.contains("edgeColor:"));
    assert!(yaml.contains("lineStyle:"));
    assert!(yaml.contains("color: seagreen"));
    assert!(yaml.contains("pattern: dotted"));
    assert!(yaml.contains("weight: 3.0"));
    assert!(!yaml.contains("value:"));
}

#[derive(Serialize, SpytialDecorators)]
#[atom_style(
    selector = "Node",
    border_style(color = "steelblue", width = 2.0),
    fill_style(color = "#eef6ff"),
    text_style(size = "large")
)]
struct AtomStyled {
    id: u32,
}

#[test]
fn atom_style_directive_blocks() {
    let decorators = AtomStyled::decorators();
    let yaml = to_yaml(&decorators).unwrap();
    assert!(yaml.contains("atomStyle:"));
    assert!(yaml.contains("selector: Node"));
    assert!(yaml.contains("borderStyle:"));
    assert!(yaml.contains("color: steelblue"));
    assert!(yaml.contains("width: 2.0"));
    assert!(yaml.contains("fillStyle:"));
    assert!(yaml.contains("textStyle:"));
    assert!(yaml.contains("size: large"));
}

#[derive(Serialize, SpytialDecorators)]
#[atom_color(selector = "Legacy", value = "crimson")]
struct AtomColorLegacy {
    id: u32,
}

#[test]
fn atom_color_desugars_to_border_style() {
    // Legacy atomColor colored the atom's border; the rewrite must preserve
    // that look (border, NOT fill).
    let decorators = AtomColorLegacy::decorators();
    let yaml = to_yaml(&decorators).unwrap();
    assert!(yaml.contains("atomStyle:"));
    assert!(!yaml.contains("atomColor:"));
    assert!(yaml.contains("borderStyle:"));
    assert!(yaml.contains("color: crimson"));
    assert!(!yaml.contains("fillStyle:"));
}

#[derive(Serialize, SpytialDecorators)]
#[inferred_edge(
    name = "ancestor",
    selector = "^parent",
    line_style(color = "gray", pattern = "dotted")
)]
struct InferredEdgeStyled {
    id: u32,
}

#[test]
fn inferred_edge_line_style_block() {
    let decorators = InferredEdgeStyled::decorators();
    let yaml = to_yaml(&decorators).unwrap();
    assert!(yaml.contains("inferredEdge:"));
    assert!(yaml.contains("name: ancestor"));
    assert!(yaml.contains("lineStyle:"));
    assert!(yaml.contains("pattern: dotted"));
}

#[derive(Serialize, SpytialDecorators)]
#[attribute(field = "weight", text_style(size = "small"))]
#[tag(
    to_tag = "Person",
    name = "age",
    value = "Person.age",
    text_style(color = "gray")
)]
struct TextStyledLines {
    weight: u32,
}

#[test]
fn attribute_and_tag_text_style() {
    // spytial-core 3.1: attribute/tag lines take the shared textStyle block.
    let decorators = TextStyledLines::decorators();
    let yaml = to_yaml(&decorators).unwrap();
    assert!(yaml.contains("attribute:"));
    assert!(yaml.contains("size: small"));
    assert!(yaml.contains("tag:"));
    assert!(yaml.contains("color: gray"));
    assert!(!yaml.contains("textSize:"));
}

#[derive(Serialize, SpytialDecorators)]
#[group(
    selector = "Team.members",
    name = "Team",
    add_edge(
        points = "togroup",
        line_style(pattern = "dashed"),
        text_style(size = "small")
    ),
    text_style(color = "navy")
)]
struct GroupStyled {
    id: u32,
}

#[test]
fn group_add_edge_block_and_label_style() {
    let decorators = GroupStyled::decorators();
    let yaml = to_yaml(&decorators).unwrap();
    assert!(yaml.contains("group:"));
    assert!(yaml.contains("addEdge:"));
    assert!(yaml.contains("points: togroup"));
    assert!(yaml.contains("pattern: dashed"));
    // The group's own label styling is a sibling of addEdge.
    assert!(yaml.contains("color: navy"));
}

#[derive(Serialize, SpytialDecorators)]
#[group(selector = "Herd.animals", name = "Herd", add_edge = "fromgroup")]
struct GroupBareAddEdge {
    id: u32,
}

#[test]
fn group_add_edge_bare_direction() {
    let decorators = GroupBareAddEdge::decorators();
    let yaml = to_yaml(&decorators).unwrap();
    assert!(yaml.contains("addEdge: fromgroup"));
    assert!(!yaml.contains("points:"));
}

#[test]
fn tag_directive_multiple() {
    let decorators = MultiTagged::decorators();

    let tags: Vec<_> = decorators
        .directives
        .iter()
        .filter_map(|d| match d {
            Directive::Tag(t) => Some(&t.tag),
            _ => None,
        })
        .collect();

    assert_eq!(tags.len(), 2);
    assert!(tags.iter().any(|t| t.to_tag == "Person" && t.name == "age"));
    assert!(tags.iter().any(|t| t.to_tag == "Car" && t.name == "owner"));
}

#[derive(Serialize, SpytialDecorators)]
#[orientation(selector = "Person", directions = ["above"], negated = true)]
#[align(selector = "Person", direction = "horizontal", negated = true)]
#[cyclic(selector = "next", direction = "clockwise", negated = true)]
#[group(selector = "Foo", name = "fooGroup", negated = true)]
#[allow(clippy::duplicated_attributes)]
#[group(field = "rel", group_on = 0, add_to_group = 1, negated = true)]
struct AllNegated {
    id: u32,
}

#[test]
fn negated_constraints_emit_hold_never() {
    let decorators = AllNegated::decorators();

    let orientation = decorators
        .constraints
        .iter()
        .find_map(|c| match c {
            Constraint::Orientation(o) => Some(&o.orientation),
            _ => None,
        })
        .expect("orientation");
    assert!(orientation.negated);

    let align = decorators
        .constraints
        .iter()
        .find_map(|c| match c {
            Constraint::Align(a) => Some(&a.align),
            _ => None,
        })
        .expect("align");
    assert!(align.negated);

    let cyclic = decorators
        .constraints
        .iter()
        .find_map(|c| match c {
            Constraint::Cyclic(c) => Some(&c.cyclic),
            _ => None,
        })
        .expect("cyclic");
    assert!(cyclic.negated);

    let mut group_selector_negated = false;
    let mut group_field_negated = false;
    for c in &decorators.constraints {
        if let Constraint::Group(g) = c {
            match &g.group {
                GroupParams::SelectorBased { negated, .. } if *negated => {
                    group_selector_negated = true;
                }
                GroupParams::FieldBased { negated, .. } if *negated => {
                    group_field_negated = true;
                }
                _ => {}
            }
        }
    }
    assert!(
        group_selector_negated,
        "expected negated selector-based group"
    );
    assert!(group_field_negated, "expected negated field-based group");

    // Wire-format: negation surfaces as `hold: never` inside each inner
    // constraint object (matching spytial-core's parser).
    let yaml = to_yaml(&decorators).unwrap();
    let hold_never_count = yaml.matches("hold: never").count();
    assert_eq!(
        hold_never_count, 5,
        "expected 5 `hold: never` entries (one per negated constraint), got {hold_never_count}\n{yaml}"
    );
}

#[derive(Serialize, SpytialDecorators)]
#[orientation(selector = "Person", directions = ["above"])]
#[align(selector = "Person", direction = "horizontal")]
#[cyclic(selector = "next", direction = "clockwise")]
struct AllPositive {
    id: u32,
}

#[test]
fn positive_constraints_omit_hold_field() {
    let decorators = AllPositive::decorators();

    let yaml = to_yaml(&decorators).unwrap();
    assert!(
        !yaml.contains("hold:"),
        "positive constraints should not emit `hold` at all, got:\n{yaml}"
    );

    // Sanity-check: negated flags are all false.
    for c in &decorators.constraints {
        match c {
            Constraint::Orientation(o) => assert!(!o.orientation.negated),
            Constraint::Align(a) => assert!(!a.align.negated),
            Constraint::Cyclic(c) => assert!(!c.cyclic.negated),
            Constraint::Group(_) => {}
        }
    }
}

#[test]
fn negated_constraint_round_trips_through_yaml() {
    // Hand-build a single negated orientation, serialize, then deserialize
    // and verify negated survives. Matches spytial-core's `hold: never`
    // wire form.
    let original = SpytialDecoratorsBuilder::new()
        .orientation("r", vec!["above"], true)
        .build();

    let yaml = to_yaml(&original).unwrap();
    assert!(
        yaml.contains("hold: never"),
        "expected hold: never in:\n{yaml}"
    );

    let parsed: SpytialDecoratorsType = serde_yaml_ng::from_str(&yaml).unwrap();
    assert_eq!(parsed, original);

    // And a spytial-core-shaped YAML with the inner `hold: never` should
    // round-trip into a `negated == true` constraint.
    let core_yaml = r#"
constraints:
  - orientation:
      selector: r
      directions:
        - above
      hold: never
directives: []
"#;
    let from_core: SpytialDecoratorsType = serde_yaml_ng::from_str(core_yaml).unwrap();
    assert_eq!(from_core.constraints.len(), 1);
    if let Constraint::Orientation(o) = &from_core.constraints[0] {
        assert!(o.orientation.negated);
        assert_eq!(o.orientation.selector, "r");
        assert_eq!(o.orientation.directions, vec!["above".to_string()]);
    } else {
        panic!("expected orientation, got {:?}", from_core.constraints[0]);
    }
}
