use beryl_state::{
    ThemeColor, ThemePropertyId as Property, ThemeValue, builtin_fallback_appearance,
    canonical_theme_schema,
};

#[test]
fn activity_roles_resolve_distinct_statuses_and_compact_typography() {
    let schema = canonical_theme_schema();
    let appearance = builtin_fallback_appearance();
    let value = |role: &str, property| {
        appearance
            .style(schema.role(role).expect("registered Activity role").id())
            .unwrap()
            .property(property)
            .unwrap()
            .clone()
    };
    for (role, rgb) in [
        ("activity-panel.status-marker.running", [0x38, 0xbd, 0xf8]),
        (
            "activity-panel.status-marker.finished-ok",
            [0x22, 0xc5, 0x5e],
        ),
        (
            "activity-panel.status-marker.finished-error",
            [0xef, 0x44, 0x44],
        ),
        ("activity-panel.resize-handle", [0x33, 0x41, 0x55]),
    ] {
        assert_eq!(
            value(role, Property::Background),
            ThemeValue::Color(ThemeColor::from_rgb(rgb[0], rgb[1], rgb[2]))
        );
    }
    assert_eq!(
        value("activity-panel.key", Property::FontSize),
        ThemeValue::LogicalPixels(beryl_state::ThemeLogicalPixels::new(11.).unwrap())
    );
    assert_eq!(
        value("activity-panel.key", Property::FontWeight),
        ThemeValue::FontWeight(beryl_state::ThemeFontWeight::new(500).unwrap())
    );
    for role in [
        "activity-panel.feedback",
        "activity-panel.agent-value",
        "activity-panel.activity-value",
    ] {
        assert_eq!(
            value(role, Property::FontSize),
            ThemeValue::LogicalPixels(beryl_state::ThemeLogicalPixels::new(12.).unwrap())
        );
        assert!(matches!(
            value(role, Property::FontFamily),
            ThemeValue::FontFamily(_)
        ));
    }
}
