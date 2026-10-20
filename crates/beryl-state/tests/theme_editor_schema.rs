use std::collections::BTreeSet;

use beryl_state::{
    ThemeColor, ThemeDocument, ThemeParseMode, ThemePropertyId as Property, ThemeResolver,
    ThemeValue, builtin_fallback_appearance, canonical_theme_schema,
};

#[test]
fn editor_and_settings_typography_expose_only_their_declared_properties() {
    let schema = canonical_theme_schema();
    for (role, expected) in [
        ("settings.input.caret", vec![Property::Color]),
        (
            "settings.window.text",
            vec![
                Property::FontFamily,
                Property::FontSize,
                Property::FontWeight,
            ],
        ),
        ("theme-editor", vec![Property::Foreground]),
        (
            "theme-editor.navigator",
            vec![Property::Background, Property::BorderColor],
        ),
        (
            "theme-editor.column",
            vec![Property::Background, Property::BorderColor],
        ),
        ("theme-editor.column-header", vec![Property::Foreground]),
        ("theme-editor.role-row", vec![Property::Foreground]),
        ("theme-editor.role-sample", vec![Property::BorderColor]),
        (
            "theme-editor.role-row.hover",
            vec![Property::Background, Property::Foreground],
        ),
        (
            "theme-editor.role-row.pressed",
            vec![Property::Background, Property::Foreground],
        ),
        (
            "theme-editor.role-row.selected",
            vec![Property::Background, Property::Foreground],
        ),
        ("theme-editor.role-row.focused", vec![Property::Color]),
    ] {
        let role = schema.role(role).unwrap();
        assert_eq!(
            role.properties().keys().copied().collect::<BTreeSet<_>>(),
            expected.into_iter().collect()
        );
        assert!(
            role.properties()
                .values()
                .all(|property| !property.ambient_parent_eligible())
        );
    }
    assert_eq!(
        schema
            .role("settings.window.text")
            .unwrap()
            .static_parent()
            .unwrap()
            .as_str(),
        "text"
    );
}

#[test]
fn editor_fallbacks_match_the_owning_widget_palette_and_settings_typography() {
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
    for (role, property, [red, green, blue]) in [
        ("theme-editor", Property::Foreground, [0xe7, 0xe3, 0xd8]),
        (
            "theme-editor.navigator",
            Property::Background,
            [0x11, 0x12, 0x14],
        ),
        (
            "theme-editor.navigator",
            Property::BorderColor,
            [0x31, 0x36, 0x3b],
        ),
        (
            "theme-editor.column",
            Property::Background,
            [0x1d, 0x21, 0x25],
        ),
        (
            "theme-editor.column",
            Property::BorderColor,
            [0x31, 0x36, 0x3b],
        ),
        (
            "theme-editor.column-header",
            Property::Foreground,
            [0x8d, 0x95, 0x9c],
        ),
        (
            "theme-editor.role-row",
            Property::Foreground,
            [0xe7, 0xe3, 0xd8],
        ),
        (
            "theme-editor.role-sample",
            Property::BorderColor,
            [0x4b, 0x53, 0x5c],
        ),
        (
            "theme-editor.role-row.hover",
            Property::Background,
            [0x25, 0x2b, 0x30],
        ),
        (
            "theme-editor.role-row.hover",
            Property::Foreground,
            [0xf3, 0xf7, 0xf4],
        ),
        (
            "theme-editor.role-row.pressed",
            Property::Background,
            [0x18, 0x2f, 0x3c],
        ),
        (
            "theme-editor.role-row.pressed",
            Property::Foreground,
            [0xf3, 0xf7, 0xf4],
        ),
        (
            "theme-editor.role-row.selected",
            Property::Background,
            [0x21, 0x48, 0x5d],
        ),
        (
            "theme-editor.role-row.selected",
            Property::Foreground,
            [0xf3, 0xf7, 0xf4],
        ),
        (
            "theme-editor.role-row.focused",
            Property::Color,
            [0x49, 0x96, 0x6f],
        ),
    ] {
        assert_eq!(
            value(role, property),
            ThemeValue::Color(ThemeColor::from_rgb(red, green, blue))
        );
    }
    assert!(
        matches!(value("settings.window.text", Property::FontFamily), ThemeValue::FontFamily(family) if family.as_str() == "Inter")
    );
    assert!(
        matches!(value("settings.window.text", Property::FontSize), ThemeValue::LogicalPixels(size) if size.get() == 14.0)
    );
    assert!(
        matches!(value("settings.window.text", Property::FontWeight), ThemeValue::FontWeight(weight) if weight.get() == 400)
    );
}

#[test]
fn canonical_editor_properties_round_trip_and_resolve_eligible_static_inheritance() {
    let document = ThemeDocument::parse_bytes(
        br##"schema = 1
[[role]]
id = "text"
font_family = "Configured Font"
font_size = 19.0
font_weight = 500
[[role]]
id = "settings.window.text"
font_family = "static_parent"
font_size = "static_parent"
font_weight = "static_parent"
[[role]]
id = "theme-editor.navigator"
border_color = "#123456"
[[role]]
id = "theme-editor.column"
border_color = "static_parent"
"##,
        ThemeParseMode::StrictCandidate,
    )
    .unwrap();
    let schema = canonical_theme_schema();
    let appearance = ThemeResolver::new(document.definition()).unwrap().resolve();
    let typography = appearance
        .style(schema.role("settings.window.text").unwrap().id())
        .unwrap();
    assert!(
        matches!(typography.property(Property::FontSize), Some(ThemeValue::LogicalPixels(size)) if size.get() == 19.0)
    );
    assert!(
        matches!(typography.property(Property::FontFamily), Some(ThemeValue::FontFamily(family)) if family.as_str() == "Configured Font")
    );
    assert_eq!(
        appearance
            .style(schema.role("theme-editor.column").unwrap().id())
            .unwrap()
            .property(Property::BorderColor),
        Some(&ThemeValue::Color(ThemeColor::from_rgb(0x12, 0x34, 0x56)))
    );
    assert_eq!(
        ThemeDocument::parse_bytes(
            document.to_canonical_toml().unwrap().as_bytes(),
            ThemeParseMode::StrictCandidate
        )
        .unwrap(),
        document
    );
}

#[test]
fn transparent_fills_geometry_and_ambient_sources_are_not_invented_as_properties() {
    for entry in [
        "id = \"theme-editor.role-row\"\nbackground = \"#123456\"",
        "id = \"theme-editor.role-sample\"\nbackground = \"#123456\"",
        "id = \"theme-editor.navigator\"\nheight = 156",
        "id = \"settings.window.text\"\nforeground = \"#123456\"",
        "id = \"theme-editor.role-row\"\nforeground = \"ambient_parent\"",
    ] {
        let text = format!("schema = 1\n[[role]]\n{entry}\n");
        assert!(
            ThemeDocument::parse_bytes(text.as_bytes(), ThemeParseMode::StrictCandidate).is_err()
        );
    }
}
