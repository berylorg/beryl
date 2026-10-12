use beryl_state::{
    ThemeColor, ThemeDocument, ThemeParseMode, ThemePropertyId as Property, ThemeResolver,
    ThemeValue, builtin_fallback_appearance, canonical_theme_schema,
};

#[test]
fn context_roles_resolve_widget_fallbacks_and_active_typography_colors() {
    let schema = canonical_theme_schema();
    let fallback = builtin_fallback_appearance();
    for (role, color, size) in [
        ("transcript-view.context-heading", [0x7d, 0xd3, 0xfc], 10.0),
        ("transcript-view.context-body", [0xe2, 0xe8, 0xf0], 14.0),
        (
            "transcript-view.context-provenance",
            [0x94, 0xa3, 0xb8],
            11.0,
        ),
    ] {
        let role = schema.role(role).unwrap();
        let style = fallback.style(role.id()).unwrap();
        assert_eq!(
            style.property(Property::Foreground),
            Some(&ThemeValue::Color(ThemeColor::from_rgb(
                color[0], color[1], color[2]
            )))
        );
        assert!(
            matches!(style.property(Property::FontSize),Some(ThemeValue::LogicalPixels(actual)) if actual.get()==size)
        );
        assert!(role.properties().len() <= 8);
        assert!(role.property(Property::TextBackground).is_none());
    }
    let document = ThemeDocument::parse_bytes(
        br##"schema = 1
id = "context-test"
name = "Context Test"
[[role]]
id = "transcript-view.context-group"
background = "#112233"
border = "#445566"
[[role]]
id = "transcript-view.context-body"
font_family = ".SystemUIFont"
font_size = 28
foreground = "#abcdef"
"##,
        ThemeParseMode::StrictCandidate,
    )
    .unwrap();
    let resolved = ThemeResolver::new(document.definition()).unwrap().resolve();
    let body = resolved
        .style(schema.role("transcript-view.context-body").unwrap().id())
        .unwrap();
    assert!(
        matches!(body.property(Property::FontSize),Some(ThemeValue::LogicalPixels(size)) if size.get()==28.0)
    );
    assert_eq!(
        body.property(Property::Foreground),
        Some(&ThemeValue::Color(ThemeColor::from_rgb(0xab, 0xcd, 0xef)))
    );
}
