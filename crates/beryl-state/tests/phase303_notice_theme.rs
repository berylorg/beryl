use beryl_state::{
    ThemeColor, ThemeDocument, ThemeDocumentError, ThemeFontWeight, ThemeLogicalPixels,
    ThemeParseMode, ThemePropertyId, ThemeResolver, ThemeValue, builtin_fallback_appearance,
    canonical_theme_schema,
};

fn color(red: u8, green: u8, blue: u8) -> ThemeValue {
    ThemeValue::Color(ThemeColor::from_rgb(red, green, blue))
}

fn style_value<'a>(
    appearance: &'a beryl_state::ResolvedAppearance,
    role: &str,
    property: ThemePropertyId,
) -> &'a ThemeValue {
    let role = canonical_theme_schema()
        .role(role)
        .expect("notice role must be declared");
    appearance
        .style(role.id())
        .expect("complete appearance must resolve the notice role")
        .property(property)
        .expect("declared notice property must resolve")
}

fn assert_color(
    appearance: &beryl_state::ResolvedAppearance,
    role: &str,
    property: ThemePropertyId,
    expected: [u8; 3],
) {
    assert_eq!(
        style_value(appearance, role, property),
        &color(expected[0], expected[1], expected[2])
    );
}

#[test]
fn notice_roles_match_the_widget_color_and_typography_contract() {
    let schema = canonical_theme_schema();
    let expected = [
        (
            "main-window-notice",
            &[
                ThemePropertyId::Background,
                ThemePropertyId::Border,
                ThemePropertyId::Foreground,
            ][..],
        ),
        (
            "main-window-notice.title",
            &[
                ThemePropertyId::Foreground,
                ThemePropertyId::TextBackground,
                ThemePropertyId::FontFamily,
                ThemePropertyId::FontSize,
                ThemePropertyId::FontWeight,
            ][..],
        ),
        (
            "main-window-notice.variant-marker",
            &[ThemePropertyId::Background][..],
        ),
        (
            "main-window-notice.detail-viewport",
            &[
                ThemePropertyId::Foreground,
                ThemePropertyId::TextBackground,
                ThemePropertyId::FontFamily,
                ThemePropertyId::FontSize,
                ThemePropertyId::FontWeight,
            ][..],
        ),
        (
            "main-window-notice.close",
            &[ThemePropertyId::Foreground][..],
        ),
    ];

    for (role, properties) in expected {
        let declared = schema
            .role(role)
            .expect("base notice role must be declared");
        assert_eq!(
            declared.properties().keys().copied().collect::<Vec<_>>(),
            properties
        );
    }

    for severity in ["warning", "error", "info"] {
        for (role, properties) in expected {
            let role = format!("{role}.{severity}");
            let declared = schema.role(&role).expect("severity role must be declared");
            assert_eq!(
                declared.properties().keys().copied().collect::<Vec<_>>(),
                properties
            );
        }
    }

    let fallback = builtin_fallback_appearance();
    assert_eq!(
        style_value(
            &fallback,
            "main-window-notice.title",
            ThemePropertyId::FontSize
        ),
        &ThemeValue::LogicalPixels(ThemeLogicalPixels::new(13.0).unwrap())
    );
    assert_eq!(
        style_value(
            &fallback,
            "main-window-notice.title",
            ThemePropertyId::FontWeight
        ),
        &ThemeValue::FontWeight(ThemeFontWeight::new(650).unwrap())
    );
    assert_color(
        &fallback,
        "main-window-notice",
        ThemePropertyId::Background,
        [0x11, 0x18, 0x27],
    );
    assert_color(
        &fallback,
        "main-window-notice",
        ThemePropertyId::Border,
        [0x33, 0x41, 0x55],
    );
    assert_color(
        &fallback,
        "main-window-notice",
        ThemePropertyId::Foreground,
        [0xe5, 0xe7, 0xeb],
    );
    assert_color(
        &fallback,
        "main-window-notice.title",
        ThemePropertyId::Foreground,
        [0xf8, 0xfa, 0xfc],
    );
    assert_color(
        &fallback,
        "main-window-notice.variant-marker",
        ThemePropertyId::Background,
        [0x38, 0xbd, 0xf8],
    );
    assert_color(
        &fallback,
        "main-window-notice.detail-viewport",
        ThemePropertyId::Foreground,
        [0xcb, 0xd5, 0xe1],
    );
    assert_color(
        &fallback,
        "main-window-notice.close",
        ThemePropertyId::Foreground,
        [0xcb, 0xd5, 0xe1],
    );

    for (severity, background, border, foreground, title, marker) in [
        (
            "warning",
            [0x2b, 0x21, 0x10],
            [0xa1, 0x62, 0x07],
            [0xfd, 0xe6, 0x8a],
            [0xfe, 0xf3, 0xc7],
            [0xf5, 0x9e, 0x0b],
        ),
        (
            "error",
            [0x2b, 0x15, 0x18],
            [0xb9, 0x1c, 0x1c],
            [0xfe, 0xca, 0xca],
            [0xfe, 0xe2, 0xe2],
            [0xef, 0x44, 0x44],
        ),
        (
            "info",
            [0x10, 0x24, 0x3a],
            [0x03, 0x69, 0xa1],
            [0xba, 0xe6, 0xfd],
            [0xe0, 0xf2, 0xfe],
            [0x38, 0xbd, 0xf8],
        ),
    ] {
        assert_color(
            &fallback,
            &format!("main-window-notice.{severity}"),
            ThemePropertyId::Background,
            background,
        );
        assert_color(
            &fallback,
            &format!("main-window-notice.{severity}"),
            ThemePropertyId::Border,
            border,
        );
        assert_color(
            &fallback,
            &format!("main-window-notice.{severity}"),
            ThemePropertyId::Foreground,
            foreground,
        );
        assert_color(
            &fallback,
            &format!("main-window-notice.title.{severity}"),
            ThemePropertyId::Foreground,
            title,
        );
        assert_color(
            &fallback,
            &format!("main-window-notice.variant-marker.{severity}"),
            ThemePropertyId::Background,
            marker,
        );
        assert_color(
            &fallback,
            &format!("main-window-notice.detail-viewport.{severity}"),
            ThemePropertyId::Foreground,
            foreground,
        );
        assert_color(
            &fallback,
            &format!("main-window-notice.close.{severity}"),
            ThemePropertyId::Foreground,
            foreground,
        );
        assert_eq!(
            style_value(
                &fallback,
                &format!("main-window-notice.title.{severity}"),
                ThemePropertyId::FontSize
            ),
            &ThemeValue::LogicalPixels(ThemeLogicalPixels::new(13.0).unwrap())
        );
        assert_eq!(
            style_value(
                &fallback,
                &format!("main-window-notice.title.{severity}"),
                ThemePropertyId::FontWeight
            ),
            &ThemeValue::FontWeight(ThemeFontWeight::new(650).unwrap())
        );
    }
}

#[test]
fn notice_document_overrides_resolve_through_the_declared_severity_hierarchy() {
    let document = ThemeDocument::parse_bytes(
        br##"schema = 1

[[role]]
id = "main-window-notice.warning"
background = "#010203"
foreground = "#010203"

[[role]]
id = "main-window-notice.title.warning"
static_parent = "main-window-notice.warning"
foreground = "static_parent"

[[role]]
id = "main-window-notice.variant-marker.warning"
background = "#040506"

[[role]]
id = "main-window-notice.detail-viewport.warning"
static_parent = "main-window-notice.warning"
foreground = "static_parent"

[[role]]
id = "main-window-notice.close.warning"
static_parent = "main-window-notice.warning"
foreground = "static_parent"
"##,
        ThemeParseMode::StrictCandidate,
    )
    .unwrap();
    let appearance = ThemeResolver::new(document.definition()).unwrap().resolve();

    assert_eq!(
        style_value(
            &appearance,
            "main-window-notice.warning",
            ThemePropertyId::Background
        ),
        &color(1, 2, 3)
    );
    assert_eq!(
        style_value(
            &appearance,
            "main-window-notice.title.warning",
            ThemePropertyId::Foreground
        ),
        &color(1, 2, 3)
    );
    assert_eq!(
        style_value(
            &appearance,
            "main-window-notice.variant-marker.warning",
            ThemePropertyId::Background
        ),
        &color(4, 5, 6)
    );
    assert_eq!(
        style_value(
            &appearance,
            "main-window-notice.detail-viewport.warning",
            ThemePropertyId::Foreground
        ),
        &color(1, 2, 3)
    );
    assert_eq!(
        style_value(
            &appearance,
            "main-window-notice.close.warning",
            ThemePropertyId::Foreground
        ),
        &color(1, 2, 3)
    );

    let opacity = ThemeDocument::parse_bytes(
        br##"schema = 1

[[role]]
id = "main-window-notice.entering"
opacity = 0
"##,
        ThemeParseMode::StrictCandidate,
    );
    assert!(matches!(
        opacity,
        Err(ThemeDocumentError::UnknownRole { .. })
    ));
}
