use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, Attribute, Data, DeriveInput, Fields, GenericArgument, PathArguments, Type,
};

/// Emit a decorator-probe call for each distinct user type reachable through
/// this type's fields (looking through containers like `Vec`/`Option`/`Box`).
fn collect_field_type_decorators(
    data: &Data,
    self_type_name: &str,
) -> Vec<proc_macro2::TokenStream> {
    let mut field_decorators = Vec::new();
    let mut seen_types = std::collections::HashSet::new();

    // Add the self type to seen_types to prevent self-referential includes
    seen_types.insert(self_type_name.to_string());

    if let Data::Struct(data_struct) = data {
        match &data_struct.fields {
            Fields::Named(fields) => {
                for field in &fields.named {
                    field_decorators.extend(analyze_field_type(&field.ty, &mut seen_types));
                }
            }
            Fields::Unnamed(fields) => {
                for field in &fields.unnamed {
                    field_decorators.extend(analyze_field_type(&field.ty, &mut seen_types));
                }
            }
            Fields::Unit => {}
        }
    }

    field_decorators
}

/// Analyze a field type and generate decorator-collection calls for nested types.
///
/// Containers (`Vec`, `Option`, `Box`, `Rc`, `Arc`, `RefCell`, `Cell`,
/// `VecDeque`, `LinkedList`) are unwrapped recursively to reach the inner type
/// (`Option<Box<T>>` → `T`). Primitives and standard collections are skipped
/// (they can never carry decorators). Everything else gets a probe call via
/// [`DecoProbe`] — if the type implements `HasSpytialDecorators` the real
/// decorators are returned; otherwise the probe safely returns an empty set.
fn analyze_field_type(
    ty: &Type,
    seen_types: &mut std::collections::HashSet<String>,
) -> Vec<proc_macro2::TokenStream> {
    let Type::Path(type_path) = ty else {
        return Vec::new();
    };
    let Some(segment) = type_path.path.segments.last() else {
        return Vec::new();
    };
    let name = segment.ident.to_string();
    match name.as_str() {
        // Containers: unwrap to reach the inner type
        "Vec" | "Option" | "Box" | "Rc" | "Arc" | "RefCell" | "Cell" | "VecDeque"
        | "LinkedList" => {
            if let PathArguments::AngleBracketed(args) = &segment.arguments {
                if let Some(GenericArgument::Type(inner)) = args.args.first() {
                    return analyze_field_type(inner, seen_types);
                }
            }
            Vec::new()
        }
        // Primitives and std collections: can never have decorators
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
        | "usize" | "f32" | "f64" | "bool" | "char" | "String" | "str" | "Result" | "HashMap"
        | "HashSet" | "BTreeMap" | "BTreeSet" => Vec::new(),
        // Everything else: safe to probe
        _ => {
            if seen_types.insert(name.clone()) {
                vec![generate_probe_call(&name)]
            } else {
                Vec::new()
            }
        }
    }
}

/// Generate a probe call that safely collects decorators from `type_name`.
///
/// Uses the inherent-method-priority trick: if the type implements
/// `HasSpytialDecorators`, the inherent `DecoProbe::get` is chosen and
/// returns real decorators.  Otherwise the blanket `DefaultDecorators::get`
/// is chosen and returns an empty set.  No heuristic needed.
fn generate_probe_call(type_name: &str) -> proc_macro2::TokenStream {
    let type_ident = syn::Ident::new(type_name, proc_macro2::Span::call_site());
    quote! {
        .extend_with({
            use spytial::spytial_annotations::DefaultDecorators as _;
            spytial::spytial_annotations::DecoProbe::<#type_ident>(::std::marker::PhantomData).get()
        })
    }
}

/// Derive `HasSpytialDecorators`, turning a type's spatial-annotation attributes
/// into a single `decorators()` impl that also pulls in the decorators of nested
/// field types.
///
/// # Supported Attributes
///
/// Styling uses the spytial-core 3.x nested blocks, written as groups that
/// mirror the YAML 1:1: `line_style(color = ..., pattern = ..., weight = ...,
/// highlight = ...)`, `text_style(size = ..., color = ...)`,
/// `border_style(color = ..., width = ...)`, `fill_style(color = ...)`.
///
/// - `#[attribute(field = "field_name", text_style(size = "small"))]` - Adds attribute directive (`text_style` optional)
/// - `#[flag(name = "flag_name")]` - Adds flag directive
/// - `#[orientation(selector = "sel", directions = ["up", "down"], negated = true)]` - Adds orientation constraint (`negated` optional)
/// - `#[align(selector = "sel", direction = "horizontal", negated = true)]` - Adds align constraint (`negated` optional)
/// - `#[cyclic(selector = "sel", direction = "up", negated = true)]` - Adds cyclic constraint (`negated` optional)
/// - `#[group(selector = "sel", name = "group_name", add_edge(points = "togroup", line_style(pattern = "dashed")), text_style(color = "navy"), negated = true)]` - Adds selector-based group constraint (`add_edge` — bare `add_edge = "togroup"` or the styled block, `text_style` for the group's own label, and `negated` all optional)
/// - `#[group(field = "field", group_on = 1, add_to_group = 2, negated = true)]` - Adds field-based group constraint (`negated` optional)
/// - `#[atom_style(selector = "sel", border_style(color = "steelblue", width = 2.0), fill_style(color = "#eef6ff"), text_style(size = "large"))]` - Adds atom style directive (all parts optional)
/// - `#[atom_color(selector = "sel", value = "red")]` - Legacy form; rewrites to `atom_style` with `value` as the *border* colour
/// - `#[size(selector = "sel", height = 20, width = 30)]` - Adds size directive
/// - `#[icon(selector = "sel", path = "icon.png", show_labels = true)]` - Adds icon directive
/// - `#[edge_style(field = "field", line_style(color = "blue", pattern = "dashed", weight = 2.0), text_style(size = "small"), show_label = true, hidden = false, filter = "...", selector = "...")]` - Adds edge style directive. The legacy flat keys (`value = "blue", style = "dashed", weight = 2.0`) still parse and rewrite onto the blocks (`value` -> line colour); mixing the two shapes is a compile error
/// - `#[projection(sig = "signature")]` - Adds projection directive
/// - `#[hide_field(field = "field")]` - Adds hide field directive
/// - `#[hide_atom(selector = "sel")]` - Adds hide atom directive
/// - `#[inferred_edge(name = "edge", selector = "sel", line_style(color = "gray", pattern = "dotted"))]` - Adds inferred edge directive (`line_style`/`text_style` optional)
/// - `#[tag(to_tag = "sel", name = "attr", value = "n-ary selector", text_style(size = "small"))]` - Adds tag directive (`text_style` optional)
///
/// # Example
/// ```rust
/// use serde::Serialize;
/// use spytial::SpytialDecorators;
///
/// #[derive(Serialize, SpytialDecorators)]
/// #[attribute(field = "name")]
/// #[flag(name = "important")]
/// struct Person {
///     name: String,
///     age: u32,
/// }
/// ```
#[proc_macro_derive(
    SpytialDecorators,
    attributes(
        attribute,
        flag,
        orientation,
        align,
        cyclic,
        group,
        atom_color,
        atom_style,
        size,
        icon,
        edge_style,
        projection,
        hide_field,
        hide_atom,
        inferred_edge,
        tag
    )
)]
pub fn derive_spytial_decorators(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Parse spatial annotation attributes for this type
    let mut decorator_calls = Vec::new();

    for attr in &input.attrs {
        let parsed = match parse_spatial_attribute(attr) {
            Ok(parsed) => parsed,
            Err(err) => return err.to_compile_error().into(),
        };
        match parsed {
            Some(SpatialAttribute::Attribute { field, text_style }) => {
                let ts = quote_text_style(&text_style);
                decorator_calls.push(quote! {
                    .attribute_styled(#field, None, #ts)
                });
            }
            Some(SpatialAttribute::Flag { name }) => {
                decorator_calls.push(quote! {
                    .flag(#name)
                });
            }
            Some(SpatialAttribute::Orientation {
                selector,
                directions,
                negated,
            }) => {
                decorator_calls.push(quote! {
                    .orientation(#selector, vec![#(#directions),*], #negated)
                });
            }
            Some(SpatialAttribute::Align {
                selector,
                direction,
                negated,
            }) => {
                decorator_calls.push(quote! {
                    .align(#selector, #direction, #negated)
                });
            }
            Some(SpatialAttribute::Cyclic {
                selector,
                direction,
                negated,
            }) => {
                decorator_calls.push(quote! {
                    .cyclic(#selector, #direction, #negated)
                });
            }
            Some(SpatialAttribute::GroupSelector {
                selector,
                name,
                add_edge,
                text_style,
                negated,
            }) => {
                let ae = quote_add_edge(&add_edge);
                let ts = quote_text_style(&text_style);
                decorator_calls.push(quote! {
                    .group_selector_based_styled(#selector, #name, #ae, #ts, #negated)
                });
            }
            Some(SpatialAttribute::GroupField {
                field,
                group_on,
                add_to_group,
                negated,
            }) => {
                decorator_calls.push(quote! {
                    .group_field_based(#field, #group_on, #add_to_group, None, #negated)
                });
            }
            Some(SpatialAttribute::AtomColor { selector, value }) => {
                decorator_calls.push(quote! {
                    .atom_color(#selector, #value)
                });
            }
            Some(SpatialAttribute::AtomStyle {
                selector,
                fill_style,
                border_style,
                text_style,
            }) => {
                let selector_arg = match selector {
                    Some(s) => quote! { Some(#s) },
                    None => quote! { None },
                };
                let fs = quote_fill_style(&fill_style);
                let bs = quote_border_style(&border_style);
                let ts = quote_text_style(&text_style);
                decorator_calls.push(quote! {
                    .atom_style(#selector_arg, #fs, #bs, #ts)
                });
            }
            Some(SpatialAttribute::Size {
                selector,
                height,
                width,
            }) => {
                decorator_calls.push(quote! {
                    .size(#selector, #height, #width)
                });
            }
            Some(SpatialAttribute::Icon {
                selector,
                path,
                show_labels,
            }) => {
                decorator_calls.push(quote! {
                    .icon(#selector, #path, #show_labels)
                });
            }
            Some(SpatialAttribute::EdgeStyleLegacy {
                field,
                value,
                selector,
                filter,
                style,
                weight,
                show_label,
                hidden,
            }) => {
                let opt_str = |v: Option<String>| match v {
                    Some(s) => quote! { Some(#s) },
                    None => quote! { None },
                };
                let opt_f64 = |v: Option<f64>| match v {
                    Some(n) => quote! { Some(#n) },
                    None => quote! { None },
                };
                let opt_bool = |v: Option<bool>| match v {
                    Some(b) => quote! { Some(#b) },
                    None => quote! { None },
                };
                let selector_arg = opt_str(selector);
                let filter_arg = opt_str(filter);
                let style_arg = opt_str(style);
                let weight_arg = opt_f64(weight);
                let show_label_arg = opt_bool(show_label);
                let hidden_arg = opt_bool(hidden);
                decorator_calls.push(quote! {
                    .edge_color(
                        #field,
                        #value,
                        #selector_arg,
                        #filter_arg,
                        #style_arg,
                        #weight_arg,
                        #show_label_arg,
                        #hidden_arg,
                    )
                });
            }
            Some(SpatialAttribute::EdgeStyle {
                field,
                selector,
                filter,
                line_style,
                text_style,
                show_label,
                hidden,
            }) => {
                let opt_str = |v: Option<String>| match v {
                    Some(s) => quote! { Some(#s) },
                    None => quote! { None },
                };
                let opt_bool = |v: Option<bool>| match v {
                    Some(b) => quote! { Some(#b) },
                    None => quote! { None },
                };
                let selector_arg = opt_str(selector);
                let filter_arg = opt_str(filter);
                let ls = quote_line_style(&line_style);
                let ts = quote_text_style(&text_style);
                let show_label_arg = opt_bool(show_label);
                let hidden_arg = opt_bool(hidden);
                decorator_calls.push(quote! {
                    .edge_style(
                        #field,
                        #selector_arg,
                        #filter_arg,
                        #ls,
                        #ts,
                        #show_label_arg,
                        #hidden_arg,
                    )
                });
            }
            Some(SpatialAttribute::Projection { sig }) => {
                decorator_calls.push(quote! {
                    .projection(#sig)
                });
            }
            Some(SpatialAttribute::HideField { field, selector }) => {
                let selector_arg = match selector {
                    Some(s) => quote! { Some(#s) },
                    None => quote! { None },
                };
                decorator_calls.push(quote! {
                    .hide_field(#field, #selector_arg)
                });
            }
            Some(SpatialAttribute::HideAtom { selector }) => {
                decorator_calls.push(quote! {
                    .hide_atom(#selector)
                });
            }
            Some(SpatialAttribute::InferredEdge {
                name,
                selector,
                line_style,
                text_style,
            }) => {
                let ls = quote_line_style(&line_style);
                let ts = quote_text_style(&text_style);
                decorator_calls.push(quote! {
                    .inferred_edge_styled(#name, #selector, #ls, #ts)
                });
            }
            Some(SpatialAttribute::Tag {
                to_tag,
                name,
                value,
                text_style,
            }) => {
                let ts = quote_text_style(&text_style);
                decorator_calls.push(quote! {
                    .tag_styled(#to_tag, #name, #value, #ts)
                });
            }
            None => {}
        }
    }

    // Analyze field types and collect their decorators at compile time.
    // Only structs have fields to walk; enums and unions contribute nothing.
    let field_type_decorators = match &input.data {
        Data::Struct(_) => collect_field_type_decorators(&input.data, &name.to_string()),
        Data::Enum(_) | Data::Union(_) => Vec::new(),
    };

    // Combine own decorators with field type decorators
    decorator_calls.extend(field_type_decorators);

    // Generate the HasSpytialDecorators implementation
    let expanded = quote! {
        impl #impl_generics spytial::spytial_annotations::HasSpytialDecorators for #name #ty_generics #where_clause {
            fn decorators() -> spytial::spytial_annotations::SpytialDecorators {
                // Register this type automatically when decorators() is called
                static REGISTRATION: ::std::sync::Once = ::std::sync::Once::new();
                REGISTRATION.call_once(|| {
                    let decorators = spytial::spytial_annotations::SpytialDecoratorsBuilder::new()
                        #(#decorator_calls)*
                        .build();
                    spytial::spytial_annotations::register_type_decorators(
                        stringify!(#name),
                        decorators.clone()
                    );
                });

                spytial::spytial_annotations::SpytialDecoratorsBuilder::new()
                    #(#decorator_calls)*
                    .build()
            }
        }
    };

    TokenStream::from(expanded)
}

#[derive(Debug)]
enum SpatialAttribute {
    Attribute {
        field: String,
        text_style: Option<TextStyleTok>,
    },
    Flag {
        name: String,
    },
    Orientation {
        selector: String,
        directions: Vec<String>,
        negated: bool,
    },
    Align {
        selector: String,
        direction: String,
        negated: bool,
    },
    Cyclic {
        selector: String,
        direction: String,
        negated: bool,
    },
    GroupSelector {
        selector: String,
        name: String,
        add_edge: Option<AddEdgeTok>,
        text_style: Option<TextStyleTok>,
        negated: bool,
    },
    GroupField {
        field: String,
        group_on: u32,
        add_to_group: u32,
        negated: bool,
    },
    AtomColor {
        selector: String,
        value: String,
    },
    AtomStyle {
        selector: Option<String>,
        fill_style: Option<FillStyleTok>,
        border_style: Option<BorderStyleTok>,
        text_style: Option<TextStyleTok>,
    },
    Size {
        selector: String,
        height: u32,
        width: u32,
    },
    Icon {
        selector: String,
        path: String,
        show_labels: bool,
    },
    /// Legacy flat edge form (`value`/`style`/`weight`); desugars at runtime
    /// via the builder's `edge_color`.
    EdgeStyleLegacy {
        field: String,
        value: String,
        selector: Option<String>,
        filter: Option<String>,
        style: Option<String>,
        weight: Option<f64>,
        show_label: Option<bool>,
        hidden: Option<bool>,
    },
    /// spytial-core 3.x block form.
    EdgeStyle {
        field: String,
        selector: Option<String>,
        filter: Option<String>,
        line_style: Option<LineStyleTok>,
        text_style: Option<TextStyleTok>,
        show_label: Option<bool>,
        hidden: Option<bool>,
    },
    Projection {
        sig: String,
    },
    HideField {
        field: String,
        selector: Option<String>,
    },
    HideAtom {
        selector: String,
    },
    InferredEdge {
        name: String,
        selector: String,
        line_style: Option<LineStyleTok>,
        text_style: Option<TextStyleTok>,
    },
    Tag {
        to_tag: String,
        name: String,
        value: String,
        text_style: Option<TextStyleTok>,
    },
}

fn parse_spatial_attribute(attr: &Attribute) -> Result<Option<SpatialAttribute>, syn::Error> {
    let path = &attr.path();

    if path.is_ident("attribute") {
        parse_attribute_args(attr)
    } else if path.is_ident("flag") {
        parse_flag_args(attr)
    } else if path.is_ident("orientation") {
        parse_orientation_args(attr)
    } else if path.is_ident("align") {
        parse_align_args(attr)
    } else if path.is_ident("cyclic") {
        parse_cyclic_args(attr)
    } else if path.is_ident("group") {
        parse_group_args(attr)
    } else if path.is_ident("atom_color") {
        parse_atom_color_args(attr)
    } else if path.is_ident("atom_style") {
        parse_atom_style_args(attr)
    } else if path.is_ident("size") {
        parse_size_args(attr)
    } else if path.is_ident("icon") {
        parse_icon_args(attr)
    } else if path.is_ident("edge_style") {
        parse_edge_style_args(attr)
    } else if path.is_ident("projection") {
        parse_projection_args(attr)
    } else if path.is_ident("hide_field") {
        parse_hide_field_args(attr)
    } else if path.is_ident("hide_atom") {
        parse_hide_atom_args(attr)
    } else if path.is_ident("inferred_edge") {
        parse_inferred_edge_args(attr)
    } else if path.is_ident("tag") {
        parse_tag_args(attr)
    } else {
        Ok(None)
    }
}

/// Walk the meta items of `attr` and emit a `syn::Error` (pointing at the
/// offending key's span) for any key that isn't in `known`.  This catches typos
/// like `#[orientation(typo = "...")]` at compile time instead of silently
/// falling back to defaults.
///
/// The value side of each pair is consumed but not interpreted; the existing
/// string-based extractors handle the actual value parsing.
fn validate_known_keys(
    attr: &Attribute,
    attr_name: &str,
    known: &[&str],
) -> Result<(), syn::Error> {
    // Attributes like `#[flag]` with no list body have no keys to check.
    if attr.meta.require_list().is_err() {
        return Ok(());
    }

    attr.parse_nested_meta(|meta| {
        let ident = match meta.path.get_ident() {
            Some(ident) => ident,
            None => return Ok(()),
        };
        let key = ident.to_string();
        if !known.iter().any(|k| *k == key) {
            return Err(syn::Error::new(
                ident.span(),
                format!(
                    "unknown parameter `{}` for #[{}(...)]; expected one of: {}",
                    key,
                    attr_name,
                    known.join(", "),
                ),
            ));
        }
        // Consume the value (if any) so parse_nested_meta advances correctly.
        // We use `syn::Expr` rather than `TokenStream` because the latter
        // greedily eats the rest of the attribute (including subsequent
        // key/value pairs).  Tolerate parse failures here — we only care about
        // validating keys; any malformed value will surface elsewhere.
        if let Ok(value) = meta.value() {
            let _ = value.parse::<syn::Expr>();
        } else if meta.input.peek(syn::token::Paren) {
            // Group-valued key like `line_style(color = "red")`: consume the
            // parenthesized block so parse_nested_meta can advance. The block's
            // contents are parsed by the style-block extractors.
            let content;
            syn::parenthesized!(content in meta.input);
            let _ = content.parse::<proc_macro2::TokenStream>();
        }
        Ok(())
    })
}

fn parse_attribute_args(attr: &Attribute) -> Result<Option<SpatialAttribute>, syn::Error> {
    validate_known_keys(attr, "attribute", &["field", "text_style"])?;
    // Look for `field = "..."`; fall back to `name` when it's omitted.
    if let Ok(meta) = attr.meta.require_list() {
        let tokens = &meta.tokens;
        let token_str = normalize_whitespace(&tokens.to_string());
        let text_style = parse_text_style_group(attr, &token_str)?;

        if let Some(field) = extract_string_from_tokens(&strip_groups(&token_str), "field") {
            return Ok(Some(SpatialAttribute::Attribute { field, text_style }));
        }
        return Ok(Some(SpatialAttribute::Attribute {
            field: "name".to_string(),
            text_style,
        }));
    }

    Ok(Some(SpatialAttribute::Attribute {
        field: "name".to_string(),
        text_style: None,
    }))
}

fn parse_flag_args(attr: &Attribute) -> Result<Option<SpatialAttribute>, syn::Error> {
    validate_known_keys(attr, "flag", &["name"])?;
    if let Ok(meta) = attr.meta.require_list() {
        let tokens = &meta.tokens;
        let token_str = tokens.to_string();

        if let Some(name) = extract_string_from_tokens(&token_str, "name") {
            return Ok(Some(SpatialAttribute::Flag { name }));
        }
    }

    Ok(Some(SpatialAttribute::Flag {
        name: "important".to_string(),
    }))
}

fn parse_orientation_args(attr: &Attribute) -> Result<Option<SpatialAttribute>, syn::Error> {
    validate_known_keys(attr, "orientation", &["selector", "directions", "negated"])?;
    if let Ok(meta) = attr.meta.require_list() {
        let tokens = &meta.tokens;
        let token_str = normalize_whitespace(&tokens.to_string());

        let selector =
            extract_string_from_tokens(&token_str, "selector").unwrap_or_else(|| "".to_string());
        let directions = extract_array_from_tokens(&token_str, "directions")
            .unwrap_or_else(|| vec!["up".to_string(), "down".to_string()]);
        let negated = extract_bool_from_tokens(&token_str, "negated").unwrap_or(false);

        return Ok(Some(SpatialAttribute::Orientation {
            selector,
            directions,
            negated,
        }));
    }

    Ok(None)
}

fn parse_group_args(attr: &Attribute) -> Result<Option<SpatialAttribute>, syn::Error> {
    // `group` accepts two shapes (selector-based or field-based); accept the
    // union of valid keys here and let the body pick the right variant.
    validate_known_keys(
        attr,
        "group",
        &[
            "selector",
            "name",
            "field",
            "group_on",
            "add_to_group",
            "add_edge",
            "text_style",
            "negated",
        ],
    )?;
    if let Ok(meta) = attr.meta.require_list() {
        let tokens = &meta.tokens;
        let token_str = normalize_whitespace(&tokens.to_string());
        // Flat keys are read from the group-stripped string so nothing inside
        // add_edge(...)/text_style(...) is mistaken for a top-level key.
        let stripped = strip_groups(&token_str);
        let negated = extract_bool_from_tokens(&stripped, "negated").unwrap_or(false);

        if stripped.contains("field =") {
            // Field-based grouping
            let field =
                extract_string_from_tokens(&stripped, "field").unwrap_or_else(|| "id".to_string());
            let group_on = extract_number_from_tokens(&stripped, "group_on").unwrap_or(1);
            let add_to_group = extract_number_from_tokens(&stripped, "add_to_group").unwrap_or(2);

            Ok(Some(SpatialAttribute::GroupField {
                field,
                group_on,
                add_to_group,
                negated,
            }))
        } else {
            // Selector-based grouping
            let selector =
                extract_string_from_tokens(&stripped, "selector").unwrap_or_else(|| "".to_string());
            let name = extract_string_from_tokens(&stripped, "name")
                .unwrap_or_else(|| "default".to_string());
            let add_edge = parse_add_edge(attr, &token_str, &stripped)?;
            let text_style = parse_text_style_group(attr, &token_str)?;

            Ok(Some(SpatialAttribute::GroupSelector {
                selector,
                name,
                add_edge,
                text_style,
                negated,
            }))
        }
    } else {
        Ok(None)
    }
}

fn parse_align_args(attr: &Attribute) -> Result<Option<SpatialAttribute>, syn::Error> {
    validate_known_keys(attr, "align", &["selector", "direction", "negated"])?;
    if let Ok(meta) = attr.meta.require_list() {
        let tokens = &meta.tokens;
        let token_str = normalize_whitespace(&tokens.to_string());

        let selector =
            extract_string_from_tokens(&token_str, "selector").unwrap_or_else(|| "".to_string());
        let direction = extract_string_from_tokens(&token_str, "direction")
            .unwrap_or_else(|| "horizontal".to_string());
        let negated = extract_bool_from_tokens(&token_str, "negated").unwrap_or(false);

        Ok(Some(SpatialAttribute::Align {
            selector,
            direction,
            negated,
        }))
    } else {
        Ok(None)
    }
}

fn parse_cyclic_args(attr: &Attribute) -> Result<Option<SpatialAttribute>, syn::Error> {
    validate_known_keys(attr, "cyclic", &["selector", "direction", "negated"])?;
    if let Ok(meta) = attr.meta.require_list() {
        let tokens = &meta.tokens;
        let token_str = normalize_whitespace(&tokens.to_string());

        let selector =
            extract_string_from_tokens(&token_str, "selector").unwrap_or_else(|| "".to_string());
        let direction =
            extract_string_from_tokens(&token_str, "direction").unwrap_or_else(|| "up".to_string());
        let negated = extract_bool_from_tokens(&token_str, "negated").unwrap_or(false);

        Ok(Some(SpatialAttribute::Cyclic {
            selector,
            direction,
            negated,
        }))
    } else {
        Ok(None)
    }
}

fn parse_atom_color_args(attr: &Attribute) -> Result<Option<SpatialAttribute>, syn::Error> {
    validate_known_keys(attr, "atom_color", &["selector", "value"])?;
    if let Ok(meta) = attr.meta.require_list() {
        let tokens = &meta.tokens;
        let token_str = tokens.to_string();

        let selector =
            extract_string_from_tokens(&token_str, "selector").unwrap_or_else(|| "".to_string());
        let value =
            extract_string_from_tokens(&token_str, "value").unwrap_or_else(|| "blue".to_string());

        Ok(Some(SpatialAttribute::AtomColor { selector, value }))
    } else {
        Ok(None)
    }
}

fn parse_size_args(attr: &Attribute) -> Result<Option<SpatialAttribute>, syn::Error> {
    validate_known_keys(attr, "size", &["selector", "height", "width"])?;
    if let Ok(meta) = attr.meta.require_list() {
        let tokens = &meta.tokens;
        let token_str = tokens.to_string();

        let selector =
            extract_string_from_tokens(&token_str, "selector").unwrap_or_else(|| "".to_string());
        let height = extract_number_from_tokens(&token_str, "height").unwrap_or(20);
        let width = extract_number_from_tokens(&token_str, "width").unwrap_or(30);

        Ok(Some(SpatialAttribute::Size {
            selector,
            height,
            width,
        }))
    } else {
        Ok(None)
    }
}

fn parse_icon_args(attr: &Attribute) -> Result<Option<SpatialAttribute>, syn::Error> {
    validate_known_keys(attr, "icon", &["selector", "path", "show_labels"])?;
    if let Ok(meta) = attr.meta.require_list() {
        let tokens = &meta.tokens;
        let token_str = tokens.to_string();

        let selector =
            extract_string_from_tokens(&token_str, "selector").unwrap_or_else(|| "".to_string());
        let path = extract_string_from_tokens(&token_str, "path")
            .unwrap_or_else(|| "icon.png".to_string());
        let show_labels = extract_bool_from_tokens(&token_str, "show_labels").unwrap_or(true);

        Ok(Some(SpatialAttribute::Icon {
            selector,
            path,
            show_labels,
        }))
    } else {
        Ok(None)
    }
}

fn parse_edge_style_args(attr: &Attribute) -> Result<Option<SpatialAttribute>, syn::Error> {
    validate_known_keys(
        attr,
        "edge_style",
        &[
            "field",
            "value",
            "selector",
            "filter",
            "style",
            "weight",
            "show_label",
            "hidden",
            "line_style",
            "text_style",
        ],
    )?;
    if let Ok(meta) = attr.meta.require_list() {
        let tokens = &meta.tokens;
        let token_str = normalize_whitespace(&tokens.to_string());
        // Flat keys come from the group-stripped string, so `weight`/`color`
        // inside `line_style(...)` are never read as legacy flat keys.
        let stripped = strip_groups(&token_str);

        let field = extract_string_from_tokens(&stripped, "field")
            .unwrap_or_else(|| "relation".to_string());
        let selector = extract_string_from_tokens(&stripped, "selector");
        let filter = extract_string_from_tokens(&stripped, "filter");
        let show_label = extract_bool_from_tokens(&stripped, "show_label");
        let hidden = extract_bool_from_tokens(&stripped, "hidden");

        let line_style = parse_line_style_group(attr, &token_str)?;
        let text_style = parse_text_style_group(attr, &token_str)?;

        // Legacy flat keys (2.x edgeColor shape).
        let value = extract_string_from_tokens(&stripped, "value");
        let style = extract_string_from_tokens(&stripped, "style");
        let weight = extract_float_from_tokens(&stripped, "weight");

        let has_legacy = value.is_some() || style.is_some() || weight.is_some();
        let has_blocks = line_style.is_some() || text_style.is_some();
        if has_legacy && has_blocks {
            return Err(err(
                attr,
                "edge_style got both the legacy flat keys (value/style/weight) and \
                 line_style(...)/text_style(...) blocks; use the blocks only"
                    .to_string(),
            ));
        }

        if has_legacy {
            return Ok(Some(SpatialAttribute::EdgeStyleLegacy {
                field,
                value: value.unwrap_or_else(|| "blue".to_string()),
                selector,
                filter,
                style,
                weight,
                show_label,
                hidden,
            }));
        }

        Ok(Some(SpatialAttribute::EdgeStyle {
            field,
            selector,
            filter,
            line_style,
            text_style,
            show_label,
            hidden,
        }))
    } else {
        Ok(None)
    }
}

fn parse_atom_style_args(attr: &Attribute) -> Result<Option<SpatialAttribute>, syn::Error> {
    validate_known_keys(
        attr,
        "atom_style",
        &["selector", "fill_style", "border_style", "text_style"],
    )?;
    if let Ok(meta) = attr.meta.require_list() {
        let tokens = &meta.tokens;
        let token_str = normalize_whitespace(&tokens.to_string());
        let stripped = strip_groups(&token_str);

        Ok(Some(SpatialAttribute::AtomStyle {
            selector: extract_string_from_tokens(&stripped, "selector"),
            fill_style: parse_fill_style_group(&token_str),
            border_style: parse_border_style_group(attr, &token_str)?,
            text_style: parse_text_style_group(attr, &token_str)?,
        }))
    } else {
        Ok(None)
    }
}

fn parse_projection_args(attr: &Attribute) -> Result<Option<SpatialAttribute>, syn::Error> {
    validate_known_keys(attr, "projection", &["sig"])?;
    if let Ok(meta) = attr.meta.require_list() {
        let tokens = &meta.tokens;
        let token_str = tokens.to_string();

        let sig =
            extract_string_from_tokens(&token_str, "sig").unwrap_or_else(|| "default".to_string());

        Ok(Some(SpatialAttribute::Projection { sig }))
    } else {
        Ok(None)
    }
}

fn parse_hide_field_args(attr: &Attribute) -> Result<Option<SpatialAttribute>, syn::Error> {
    validate_known_keys(attr, "hide_field", &["field", "selector"])?;
    if let Ok(meta) = attr.meta.require_list() {
        let tokens = &meta.tokens;
        let token_str = tokens.to_string();

        let field =
            extract_string_from_tokens(&token_str, "field").unwrap_or_else(|| "field".to_string());
        let selector = extract_string_from_tokens(&token_str, "selector");

        Ok(Some(SpatialAttribute::HideField { field, selector }))
    } else {
        Ok(None)
    }
}

fn parse_hide_atom_args(attr: &Attribute) -> Result<Option<SpatialAttribute>, syn::Error> {
    validate_known_keys(attr, "hide_atom", &["selector"])?;
    if let Ok(meta) = attr.meta.require_list() {
        let tokens = &meta.tokens;
        let token_str = tokens.to_string();

        let selector =
            extract_string_from_tokens(&token_str, "selector").unwrap_or_else(|| "".to_string());

        Ok(Some(SpatialAttribute::HideAtom { selector }))
    } else {
        Ok(None)
    }
}

fn parse_inferred_edge_args(attr: &Attribute) -> Result<Option<SpatialAttribute>, syn::Error> {
    validate_known_keys(
        attr,
        "inferred_edge",
        &["name", "selector", "line_style", "text_style"],
    )?;
    if let Ok(meta) = attr.meta.require_list() {
        let tokens = &meta.tokens;
        let token_str = normalize_whitespace(&tokens.to_string());
        let stripped = strip_groups(&token_str);

        let name =
            extract_string_from_tokens(&stripped, "name").unwrap_or_else(|| "edge".to_string());
        let selector =
            extract_string_from_tokens(&stripped, "selector").unwrap_or_else(|| "".to_string());

        Ok(Some(SpatialAttribute::InferredEdge {
            name,
            selector,
            line_style: parse_line_style_group(attr, &token_str)?,
            text_style: parse_text_style_group(attr, &token_str)?,
        }))
    } else {
        Ok(None)
    }
}

fn parse_tag_args(attr: &Attribute) -> Result<Option<SpatialAttribute>, syn::Error> {
    validate_known_keys(attr, "tag", &["to_tag", "name", "value", "text_style"])?;
    if let Ok(meta) = attr.meta.require_list() {
        let tokens = &meta.tokens;
        let token_str = normalize_whitespace(&tokens.to_string());
        let stripped = strip_groups(&token_str);

        let to_tag = extract_string_from_tokens(&stripped, "to_tag").unwrap_or_default();
        let name = extract_string_from_tokens(&stripped, "name").unwrap_or_default();
        let value = extract_string_from_tokens(&stripped, "value").unwrap_or_default();

        Ok(Some(SpatialAttribute::Tag {
            to_tag,
            name,
            value,
            text_style: parse_text_style_group(attr, &token_str)?,
        }))
    } else {
        Ok(None)
    }
}

/// Replace newlines/tabs with spaces so the `extract_*_from_tokens` helpers
/// (which match `key = ` / `key = "`) work even when proc-macro2 wraps long
/// attribute lists across multiple lines.
fn normalize_whitespace(tokens: &str) -> String {
    tokens.replace(['\n', '\r', '\t'], " ")
}

// ---------------------------------------------------------------------------
// Style blocks (spytial-core 3.x): parsed attribute forms + codegen
//
// The nested-group attribute syntax mirrors the YAML blocks 1:1:
//   line_style(color = "red", pattern = "dashed", weight = 2.0, highlight = "...")
//   text_style(size = "small", color = "gray")
//   border_style(color = "steelblue", width = 2.0) / fill_style(color = "#eef6ff")
//   add_edge(points = "togroup", line_style(...), text_style(...))
// Closed vocabularies (pattern/size/points) and weight positivity are checked
// here, at compile time — spytial-core silently drops invalid leaves, so the
// macro is where a typo must fail.
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct LineStyleTok {
    color: Option<String>,
    pattern: Option<String>,
    weight: Option<f64>,
    highlight: Option<String>,
}

#[derive(Debug, Default)]
struct TextStyleTok {
    size: Option<String>,
    color: Option<String>,
}

#[derive(Debug, Default)]
struct BorderStyleTok {
    color: Option<String>,
    width: Option<f64>,
}

#[derive(Debug, Default)]
struct FillStyleTok {
    color: Option<String>,
}

#[derive(Debug)]
enum AddEdgeTok {
    /// Bare direction form: `add_edge = "togroup"`.
    Direction(String),
    /// Block form: `add_edge(points = "...", line_style(...), text_style(...))`.
    Block {
        points: String,
        line_style: Option<LineStyleTok>,
        text_style: Option<TextStyleTok>,
    },
}

fn err(attr: &Attribute, msg: String) -> syn::Error {
    syn::Error::new_spanned(attr, msg)
}

fn check_pattern(attr: &Attribute, pattern: &str) -> Result<(), syn::Error> {
    match pattern {
        "solid" | "dashed" | "dotted" => Ok(()),
        other => Err(err(
            attr,
            format!(
                "invalid line pattern {other:?}; expected \"solid\", \"dashed\", or \"dotted\""
            ),
        )),
    }
}

fn check_size(attr: &Attribute, size: &str) -> Result<(), syn::Error> {
    match size {
        "small" | "normal" | "large" => Ok(()),
        other => Err(err(
            attr,
            format!("invalid text size {other:?}; expected \"small\", \"normal\", or \"large\""),
        )),
    }
}

fn check_points(attr: &Attribute, points: &str) -> Result<(), syn::Error> {
    match points {
        "none" | "togroup" | "fromgroup" => Ok(()),
        other => Err(err(
            attr,
            format!("invalid addEdge direction {other:?}; expected \"none\", \"togroup\", or \"fromgroup\""),
        )),
    }
}

fn check_positive(attr: &Attribute, value: f64, what: &str) -> Result<(), syn::Error> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(err(
            attr,
            format!("{what} must be a number greater than 0; got {value}"),
        ))
    }
}

/// Parse a `line_style(...)` group out of `tokens`, validating its leaves.
fn parse_line_style_group(
    attr: &Attribute,
    tokens: &str,
) -> Result<Option<LineStyleTok>, syn::Error> {
    let Some(body) = extract_group_from_tokens(tokens, "line_style") else {
        return Ok(None);
    };
    let ls = LineStyleTok {
        color: extract_string_from_tokens(&body, "color"),
        pattern: extract_string_from_tokens(&body, "pattern"),
        weight: extract_float_from_tokens(&body, "weight"),
        highlight: extract_string_from_tokens(&body, "highlight"),
    };
    if let Some(p) = &ls.pattern {
        check_pattern(attr, p)?;
    }
    if let Some(w) = ls.weight {
        check_positive(attr, w, "line_style weight")?;
    }
    Ok(Some(ls))
}

/// Parse a `text_style(...)` group out of `tokens`, validating its leaves.
fn parse_text_style_group(
    attr: &Attribute,
    tokens: &str,
) -> Result<Option<TextStyleTok>, syn::Error> {
    let Some(body) = extract_group_from_tokens(tokens, "text_style") else {
        return Ok(None);
    };
    let ts = TextStyleTok {
        size: extract_string_from_tokens(&body, "size"),
        color: extract_string_from_tokens(&body, "color"),
    };
    if let Some(s) = &ts.size {
        check_size(attr, s)?;
    }
    Ok(Some(ts))
}

/// Parse a `border_style(...)` group out of `tokens`, validating its leaves.
fn parse_border_style_group(
    attr: &Attribute,
    tokens: &str,
) -> Result<Option<BorderStyleTok>, syn::Error> {
    let Some(body) = extract_group_from_tokens(tokens, "border_style") else {
        return Ok(None);
    };
    let bs = BorderStyleTok {
        color: extract_string_from_tokens(&body, "color"),
        width: extract_float_from_tokens(&body, "width"),
    };
    if let Some(w) = bs.width {
        check_positive(attr, w, "border_style width")?;
    }
    Ok(Some(bs))
}

/// Parse a `fill_style(...)` group out of `tokens`.
fn parse_fill_style_group(tokens: &str) -> Option<FillStyleTok> {
    extract_group_from_tokens(tokens, "fill_style").map(|body| FillStyleTok {
        color: extract_string_from_tokens(&body, "color"),
    })
}

/// Parse an `add_edge` value: either the bare string form (from the stripped
/// token string) or the styled block form.
fn parse_add_edge(
    attr: &Attribute,
    tokens: &str,
    stripped: &str,
) -> Result<Option<AddEdgeTok>, syn::Error> {
    if let Some(body) = extract_group_from_tokens(tokens, "add_edge") {
        let points =
            extract_string_from_tokens(&body, "points").unwrap_or_else(|| "none".to_string());
        check_points(attr, &points)?;
        return Ok(Some(AddEdgeTok::Block {
            points,
            line_style: parse_line_style_group(attr, &body)?,
            text_style: parse_text_style_group(attr, &body)?,
        }));
    }
    if let Some(direction) = extract_string_from_tokens(stripped, "add_edge") {
        check_points(attr, &direction)?;
        return Ok(Some(AddEdgeTok::Direction(direction)));
    }
    Ok(None)
}

// Codegen: turn the parsed blocks into runtime constructor tokens.

fn quote_opt_string(v: &Option<String>) -> proc_macro2::TokenStream {
    match v {
        Some(s) => quote! { Some(#s.to_string()) },
        None => quote! { None },
    }
}

fn quote_opt_f64(v: Option<f64>) -> proc_macro2::TokenStream {
    match v {
        Some(n) => quote! { Some(#n) },
        None => quote! { None },
    }
}

fn quote_pattern(p: &str) -> proc_macro2::TokenStream {
    match p {
        "solid" => quote! { spytial::spytial_annotations::LinePattern::Solid },
        "dashed" => quote! { spytial::spytial_annotations::LinePattern::Dashed },
        _ => quote! { spytial::spytial_annotations::LinePattern::Dotted },
    }
}

fn quote_size(s: &str) -> proc_macro2::TokenStream {
    match s {
        "small" => quote! { spytial::spytial_annotations::TextSize::Small },
        "large" => quote! { spytial::spytial_annotations::TextSize::Large },
        _ => quote! { spytial::spytial_annotations::TextSize::Normal },
    }
}

fn quote_points(p: &str) -> proc_macro2::TokenStream {
    match p {
        "togroup" => quote! { spytial::spytial_annotations::GroupEdgePoints::Togroup },
        "fromgroup" => quote! { spytial::spytial_annotations::GroupEdgePoints::Fromgroup },
        _ => quote! { spytial::spytial_annotations::GroupEdgePoints::None },
    }
}

fn quote_line_style(ls: &Option<LineStyleTok>) -> proc_macro2::TokenStream {
    match ls {
        None => quote! { None },
        Some(ls) => {
            let color = quote_opt_string(&ls.color);
            let pattern = match &ls.pattern {
                Some(p) => {
                    let tok = quote_pattern(p);
                    quote! { Some(#tok) }
                }
                None => quote! { None },
            };
            let weight = quote_opt_f64(ls.weight);
            let highlight = quote_opt_string(&ls.highlight);
            quote! {
                Some(spytial::spytial_annotations::LineStyle {
                    color: #color,
                    pattern: #pattern,
                    weight: #weight,
                    highlight: #highlight,
                })
            }
        }
    }
}

fn quote_text_style(ts: &Option<TextStyleTok>) -> proc_macro2::TokenStream {
    match ts {
        None => quote! { None },
        Some(ts) => {
            let size = match &ts.size {
                Some(s) => {
                    let tok = quote_size(s);
                    quote! { Some(#tok) }
                }
                None => quote! { None },
            };
            let color = quote_opt_string(&ts.color);
            quote! {
                Some(spytial::spytial_annotations::TextStyle {
                    size: #size,
                    color: #color,
                })
            }
        }
    }
}

fn quote_border_style(bs: &Option<BorderStyleTok>) -> proc_macro2::TokenStream {
    match bs {
        None => quote! { None },
        Some(bs) => {
            let color = quote_opt_string(&bs.color);
            let width = quote_opt_f64(bs.width);
            quote! {
                Some(spytial::spytial_annotations::BorderStyle {
                    color: #color,
                    width: #width,
                })
            }
        }
    }
}

fn quote_fill_style(fs: &Option<FillStyleTok>) -> proc_macro2::TokenStream {
    match fs {
        None => quote! { None },
        Some(fs) => {
            let color = quote_opt_string(&fs.color);
            quote! {
                Some(spytial::spytial_annotations::FillStyle { color: #color })
            }
        }
    }
}

fn quote_add_edge(ae: &Option<AddEdgeTok>) -> proc_macro2::TokenStream {
    match ae {
        None => quote! { None },
        Some(AddEdgeTok::Direction(d)) => {
            let tok = quote_points(d);
            quote! { Some(spytial::spytial_annotations::GroupEdgeValue::Direction(#tok)) }
        }
        Some(AddEdgeTok::Block {
            points,
            line_style,
            text_style,
        }) => {
            let points_tok = quote_points(points);
            let ls = quote_line_style(line_style);
            let ts = quote_text_style(text_style);
            quote! {
                Some(spytial::spytial_annotations::GroupEdgeValue::Block(
                    spytial::spytial_annotations::GroupEdge {
                        points: #points_tok,
                        line_style: #ls,
                        text_style: #ts,
                    },
                ))
            }
        }
    }
}

fn extract_string_from_tokens(tokens: &str, key: &str) -> Option<String> {
    // Try both with and without spaces around =
    let patterns = [
        format!("{} = \"", key),
        format!("{}=\"", key),
        format!("{} =\"", key),
        format!("{}= \"", key),
    ];

    for pattern in &patterns {
        if let Some(start) = tokens.find(pattern) {
            let start = start + pattern.len();
            if let Some(end) = tokens[start..].find('"') {
                return Some(tokens[start..start + end].to_string());
            }
        }
    }
    None
}

fn extract_number_from_tokens(tokens: &str, key: &str) -> Option<u32> {
    let pattern = format!("{} = ", key);
    if let Some(start) = tokens.find(&pattern) {
        let start = start + pattern.len();
        let rest = &tokens[start..];
        let end = rest.find([',', ' ', ')']).unwrap_or(rest.len());
        if let Ok(value) = rest[..end].trim().parse::<u32>() {
            return Some(value);
        }
    }
    None
}

fn extract_bool_from_tokens(tokens: &str, key: &str) -> Option<bool> {
    let pattern = format!("{} = ", key);
    if let Some(start) = tokens.find(&pattern) {
        let start = start + pattern.len();
        let rest = &tokens[start..];
        let end = rest.find([',', ' ', ')']).unwrap_or(rest.len());
        if let Ok(value) = rest[..end].trim().parse::<bool>() {
            return Some(value);
        }
    }
    None
}

fn extract_float_from_tokens(tokens: &str, key: &str) -> Option<f64> {
    let pattern = format!("{} = ", key);
    if let Some(start) = tokens.find(&pattern) {
        let start = start + pattern.len();
        let rest = &tokens[start..];
        let end = rest.find([',', ' ', ')']).unwrap_or(rest.len());
        if let Ok(value) = rest[..end].trim().parse::<f64>() {
            return Some(value);
        }
    }
    None
}

/// Remove the contents of every parenthesized group from a token string,
/// leaving only the top-level `key = value` pairs (the group keys survive as
/// `key ()`). Used so flat-key extraction never matches a key *inside* a
/// style block — e.g. `weight` inside `line_style(weight = 2.0)` must not be
/// read as a top-level legacy `weight`. Tracks string literals so parens
/// inside selector strings don't unbalance the scan.
fn strip_groups(tokens: &str) -> String {
    let mut out = String::with_capacity(tokens.len());
    let mut depth = 0usize;
    let mut in_string = false;
    let mut prev_escape = false;
    for c in tokens.chars() {
        if in_string {
            if depth == 0 {
                out.push(c);
            }
            if c == '"' && !prev_escape {
                in_string = false;
            }
            prev_escape = c == '\\' && !prev_escape;
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                if depth == 0 {
                    out.push(c);
                }
            }
            '(' => {
                if depth == 0 {
                    out.push(c);
                }
                depth += 1;
            }
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    out.push(c);
                }
            }
            _ => {
                if depth == 0 {
                    out.push(c);
                }
            }
        }
    }
    out
}

/// Extract the inner token text of a top-level `key ( ... )` group from a
/// token string, or `None` if the key has no group at depth 0. Quote-aware
/// and balanced, so nested groups (`add_edge(points = ..., line_style(...))`)
/// and parens inside selector strings are handled.
fn extract_group_from_tokens(tokens: &str, key: &str) -> Option<String> {
    let chars: Vec<char> = tokens.chars().collect();
    let key_chars: Vec<char> = key.chars().collect();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut prev_escape = false;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            if c == '"' && !prev_escape {
                in_string = false;
            }
            prev_escape = c == '\\' && !prev_escape;
            i += 1;
            continue;
        }
        match c {
            '"' => in_string = true,
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {
                // Match `key` at depth 0, on an identifier boundary.
                if depth == 0
                    && chars[i..].starts_with(&key_chars[..])
                    && (i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_'))
                {
                    let mut j = i + key_chars.len();
                    // Identifier must end exactly at the key.
                    if j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                        i += 1;
                        continue;
                    }
                    while j < chars.len() && chars[j].is_whitespace() {
                        j += 1;
                    }
                    if j < chars.len() && chars[j] == '(' {
                        // Collect the balanced group body.
                        let mut body = String::new();
                        let mut inner_depth = 1usize;
                        let mut inner_in_string = false;
                        let mut inner_prev_escape = false;
                        let mut k = j + 1;
                        while k < chars.len() {
                            let ic = chars[k];
                            if inner_in_string {
                                if ic == '"' && !inner_prev_escape {
                                    inner_in_string = false;
                                }
                                inner_prev_escape = ic == '\\' && !inner_prev_escape;
                                body.push(ic);
                                k += 1;
                                continue;
                            }
                            match ic {
                                '"' => {
                                    inner_in_string = true;
                                    body.push(ic);
                                }
                                '(' => {
                                    inner_depth += 1;
                                    body.push(ic);
                                }
                                ')' => {
                                    inner_depth -= 1;
                                    if inner_depth == 0 {
                                        return Some(body);
                                    }
                                    body.push(ic);
                                }
                                _ => body.push(ic),
                            }
                            k += 1;
                        }
                        return None; // unbalanced — treat as absent
                    }
                }
            }
        }
        i += 1;
    }
    None
}

fn extract_array_from_tokens(tokens: &str, key: &str) -> Option<Vec<String>> {
    // Try different patterns since the tokenizer might have different spacing
    let patterns = [
        format!("{}=[", key),
        format!("{} = [", key),
        format!("{}= [", key),
        format!("{} =[", key),
    ];

    for pattern in &patterns {
        if let Some(start) = tokens.find(pattern) {
            let start = start + pattern.len();
            let rest = &tokens[start..];
            if let Some(end) = rest.find(']') {
                let array_content = &rest[..end];
                let items: Vec<String> = array_content
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                return Some(items);
            }
        }
    }
    None
}
