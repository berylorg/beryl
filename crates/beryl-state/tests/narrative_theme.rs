use beryl_state::{
    ThemeColor, ThemeDocument, ThemeFontStyle, ThemeParseMode, ThemePropertyId as Property,
    ThemeResolver, ThemeTextDecoration, ThemeValue, builtin_fallback_appearance,
    canonical_theme_schema,
};

#[test]
fn narrative_states_resolve_canonical_layout_and_composable_inline_defaults() {
    let schema = canonical_theme_schema();
    let appearance = builtin_fallback_appearance();
    let value = |role: &str, property| {
        appearance
            .style(schema.role(role).unwrap().id())
            .unwrap()
            .property(property)
            .unwrap()
            .clone()
    };
    for (level, scale, weight) in [
        (1, 1.75, 700),
        (2, 1.5, 700),
        (3, 1.25, 700),
        (4, 1.125, 700),
        (5, 1.0, 700),
        (6, 1.0, 600),
    ] {
        let role = format!("transcript-view.narrative-block.heading-{level}");
        assert!(
            matches!(value(&role, Property::FontScale), ThemeValue::FontScale(actual) if actual.get() == scale)
        );
        assert!(
            matches!(value(&role, Property::FontWeight), ThemeValue::FontWeight(actual) if actual.get() == weight)
        );
    }
    for (state, indent, border) in [
        ("quote", 12.0, 2.0),
        ("list", 20.0, 0.0),
        ("thematic-break", 0.0, 1.0),
    ] {
        let role = format!("transcript-view.narrative-block.{state}");
        assert!(
            matches!(value(&role, Property::Indent), ThemeValue::LogicalPixels(actual) if actual.get() == indent)
        );
        assert!(
            matches!(value(&role, Property::BorderWidth), ThemeValue::LogicalPixels(actual) if actual.get() == border)
        );
        assert_eq!(
            value(&role, Property::BorderColor),
            ThemeValue::Color(ThemeColor::from_rgb(0x47, 0x55, 0x69))
        );
    }
    assert_eq!(
        value("transcript-view.narrative-text", Property::Foreground),
        ThemeValue::Color(ThemeColor::from_rgb(0xe5, 0xe7, 0xeb))
    );
    assert_eq!(
        value(
            "transcript-view.narrative-text.emphasis",
            Property::FontStyle
        ),
        ThemeValue::FontStyle(ThemeFontStyle::Italic)
    );
    assert_eq!(
        value(
            "transcript-view.narrative-text.strike",
            Property::TextDecoration
        ),
        ThemeValue::TextDecoration(ThemeTextDecoration::LineThrough)
    );
    assert!(
        matches!(value("transcript-view.narrative-text.inline-code", Property::FontFamily), ThemeValue::FontFamily(family) if family.as_str() == "monospace")
    );
    assert_eq!(
        value(
            "transcript-view.narrative-text.link-label",
            Property::Foreground
        ),
        ThemeValue::Color(ThemeColor::from_rgb(0x7d, 0xd3, 0xfc))
    );
    assert!(
        schema
            .roles()
            .values()
            .all(|role| role.properties().len() <= 8)
    );
    assert!(
        !schema
            .role("root")
            .unwrap()
            .properties()
            .contains_key(&Property::FontScale)
    );
}

#[test]
fn narrative_custom_properties_round_trip_and_explicit_parent_inheritance_resolves() {
    let document = ThemeDocument::parse_bytes(
        br##"schema = 1
id = "narrative-custom"
name = "Narrative Custom"
[[role]]
id = "transcript-view.narrative-block"
indent = 5
border_color = "#112233"
[[role]]
id = "transcript-view.narrative-block.heading-2"
font_scale = 2.25
font_weight = 800
indent = "static_parent"
border_color = "static_parent"
[[role]]
id = "transcript-view.narrative-text"
foreground = "#abcdef"
[[role]]
id = "transcript-view.narrative-text.link-label"
foreground = "static_parent"
[[role]]
id = "transcript-view.narrative-text.emphasis"
font_style = "normal"
[[role]]
id = "transcript-view.narrative-text.strike"
text_decoration = "none"
"##,
        ThemeParseMode::StrictCandidate,
    )
    .unwrap();
    let encoded = document.to_canonical_toml().unwrap();
    assert_eq!(
        document,
        ThemeDocument::parse_bytes(encoded.as_bytes(), ThemeParseMode::StrictCandidate).unwrap()
    );
    let resolved = ThemeResolver::new(document.definition()).unwrap().resolve();
    let schema = canonical_theme_schema();
    let value = |role, property| {
        resolved
            .style(schema.role(role).unwrap().id())
            .unwrap()
            .property(property)
            .unwrap()
            .clone()
    };
    assert!(
        matches!(value("transcript-view.narrative-block.heading-2", Property::FontScale), ThemeValue::FontScale(scale) if scale.get() == 2.25)
    );
    assert!(
        matches!(value("transcript-view.narrative-block.heading-2", Property::Indent), ThemeValue::LogicalPixels(indent) if indent.get() == 5.0)
    );
    assert_eq!(
        value(
            "transcript-view.narrative-block.heading-2",
            Property::BorderColor
        ),
        ThemeValue::Color(ThemeColor::from_rgb(0x11, 0x22, 0x33))
    );
    assert_eq!(
        value(
            "transcript-view.narrative-text.link-label",
            Property::Foreground
        ),
        ThemeValue::Color(ThemeColor::from_rgb(0xab, 0xcd, 0xef))
    );
    assert_eq!(
        value(
            "transcript-view.narrative-text.emphasis",
            Property::FontStyle
        ),
        ThemeValue::FontStyle(ThemeFontStyle::Normal)
    );
    assert_eq!(
        value(
            "transcript-view.narrative-text.strike",
            Property::TextDecoration
        ),
        ThemeValue::TextDecoration(ThemeTextDecoration::None)
    );
}

#[test]
fn narrative_properties_reject_invalid_scale_style_and_dimension_values() {
    for (role, property) in [
        ("narrative-block.heading-1", "font_scale = 0"),
        ("narrative-block.heading-1", "font_scale = -1"),
        ("narrative-block.heading-1", "font_scale = nan"),
        ("narrative-block.heading-1", "font_scale = 4097"),
        ("narrative-block.quote", "indent = -1"),
        ("narrative-text.emphasis", "font_style = \"oblique\""),
        ("narrative-text.strike", "text_decoration = \"underline\""),
    ] {
        let source = format!(
            "schema = 1\nid = \"narrative-invalid\"\nname = \"Invalid\"\n[[role]]\nid = \"transcript-view.{role}\"\n{property}\n"
        );
        assert!(
            ThemeDocument::parse_bytes(source.as_bytes(), ThemeParseMode::StrictCandidate).is_err(),
            "{property}"
        );
    }
}
