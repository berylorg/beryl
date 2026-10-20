use beryl_state::{
    ThemeColor, ThemeDocument, ThemeParseMode, ThemePropertyId as Property, ThemeResolver,
    ThemeValue, builtin_fallback_appearance, canonical_theme_schema,
};

#[test]
fn segmented_status_roles_resolve_bundled_fallbacks_without_geometry_settings() {
    let schema = canonical_theme_schema();
    let appearance = builtin_fallback_appearance();
    for (role, property, rgb) in [
        (
            "segmented-status-bar",
            Property::Background,
            [0xf8, 0xfa, 0xfc],
        ),
        (
            "segmented-status-bar",
            Property::Foreground,
            [0x33, 0x41, 0x55],
        ),
        (
            "segmented-status-bar",
            Property::BorderColor,
            [0xcb, 0xd5, 0xe1],
        ),
        (
            "segmented-status-bar.segment.hover",
            Property::Background,
            [0xee, 0xf2, 0xf7],
        ),
        (
            "segmented-status-bar.segment.pressed",
            Property::Background,
            [0xe2, 0xe8, 0xf0],
        ),
        (
            "segmented-status-bar.segment.open",
            Property::Background,
            [0xe2, 0xe8, 0xf0],
        ),
        (
            "segmented-status-bar.segment.open",
            Property::Foreground,
            [0x0f, 0x17, 0x2a],
        ),
        (
            "segmented-status-bar.segment.disabled",
            Property::Foreground,
            [0x94, 0xa3, 0xb8],
        ),
        (
            "segmented-status-bar.divider",
            Property::Background,
            [0xcb, 0xd5, 0xe1],
        ),
        (
            "segmented-status-bar.key",
            Property::Foreground,
            [0x64, 0x74, 0x8b],
        ),
        (
            "segmented-status-bar.value",
            Property::Foreground,
            [0x0f, 0x17, 0x2a],
        ),
    ] {
        let role = schema.role(role).unwrap();
        assert_eq!(
            appearance.style(role.id()).unwrap().property(property),
            Some(&ThemeValue::Color(ThemeColor::from_rgb(
                rgb[0], rgb[1], rgb[2]
            )))
        );
        assert!(role.property(Property::BorderWidth).is_none());
        assert!(role.property(Property::FontFamily).is_none());
    }
    for (role, property, expected) in [
        ("segmented-status-bar", Property::FontSize, 12),
        ("segmented-status-bar.key", Property::FontWeight, 400),
        ("segmented-status-bar.value", Property::FontWeight, 500),
    ] {
        match appearance
            .style(schema.role(role).unwrap().id())
            .unwrap()
            .property(property)
            .unwrap()
        {
            ThemeValue::LogicalPixels(size) => assert_eq!(size.get(), expected as f32),
            ThemeValue::FontWeight(weight) => assert_eq!(weight.get(), expected),
            _ => panic!("unexpected typography property"),
        }
    }
}

#[test]
fn segmented_status_theme_accepts_only_declared_canonical_properties() {
    let document = ThemeDocument::parse_bytes(
        br##"schema = 1
id = "status-test"
name = "Status Test"
[[role]]
id = "segmented-status-bar"
background = "#102030"
border_color = "#405060"
font_size = 18
[[role]]
id = "segmented-status-bar.segment.disabled"
foreground = "#abcdef"
[[role]]
id = "segmented-status-bar.value"
font_weight = 700
"##,
        ThemeParseMode::StrictCandidate,
    )
    .unwrap();
    let resolved = ThemeResolver::new(document.definition()).unwrap().resolve();
    let schema = canonical_theme_schema();
    let property = |role, property| {
        resolved
            .style(schema.role(role).unwrap().id())
            .unwrap()
            .property(property)
            .unwrap()
    };
    assert_eq!(
        property("segmented-status-bar", Property::Background),
        &ThemeValue::Color(ThemeColor::from_rgb(0x10, 0x20, 0x30))
    );
    assert_eq!(
        property("segmented-status-bar", Property::BorderColor),
        &ThemeValue::Color(ThemeColor::from_rgb(0x40, 0x50, 0x60))
    );
    assert_eq!(
        property(
            "segmented-status-bar.segment.disabled",
            Property::Foreground
        ),
        &ThemeValue::Color(ThemeColor::from_rgb(0xab, 0xcd, 0xef))
    );
    assert!(
        matches!(property("segmented-status-bar", Property::FontSize), ThemeValue::LogicalPixels(size) if size.get() == 18.0)
    );
    assert!(
        matches!(property("segmented-status-bar.value", Property::FontWeight), ThemeValue::FontWeight(weight) if weight.get() == 700)
    );
    let unsupported = ThemeDocument::parse_bytes(
        br##"schema = 1
id = "invalid-status"
name = "Invalid Status"
[[role]]
id = "segmented-status-bar.segment.disabled"
font_size = 18
"##,
        ThemeParseMode::StrictCandidate,
    );
    assert!(unsupported.is_err());
}
