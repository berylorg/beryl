use beryl_state::{
    ThemeColor, ThemeDocument, ThemeParseMode, ThemePropertyId as Property, ThemeResolver,
    ThemeValue, builtin_fallback_appearance, canonical_theme_schema,
};
#[test]
fn table_roles_resolve_consumed_cell_header_state_and_active_typography_properties() {
    let schema = canonical_theme_schema();
    let fallback = builtin_fallback_appearance();
    for (role, property, color) in [
        ("table-panel", Property::Background, [0x0f, 0x17, 0x2a]),
        ("table-panel.cell", Property::Border, [0x33, 0x41, 0x55]),
        (
            "table-panel.header-cell",
            Property::Background,
            [0x11, 0x18, 0x27],
        ),
        (
            "table-panel.cell.selected",
            Property::Background,
            [0x1e, 0x3a, 0x5f],
        ),
        (
            "table-panel.cell.focused",
            Property::Color,
            [0x38, 0xbd, 0xf8],
        ),
    ] {
        assert_eq!(
            fallback
                .style(schema.role(role).unwrap().id())
                .unwrap()
                .property(property),
            Some(&ThemeValue::Color(ThemeColor::from_rgb(
                color[0], color[1], color[2]
            )))
        );
    }
    assert!(
        schema
            .role("table-panel.cell")
            .unwrap()
            .property(Property::TextBackground)
            .is_none()
    );
    let document = ThemeDocument::parse_bytes(
        br##"schema=1
id="table-test"
name="Table Test"
[[role]]
id="table-panel.cell"
font_size=27
background="#123456"
border="#654321"
[[role]]
id="table-panel.header-cell"
font_size=32
font_weight=650
"##,
        ThemeParseMode::StrictCandidate,
    )
    .unwrap();
    let appearance = ThemeResolver::new(document.definition()).unwrap().resolve();
    let cell = appearance
        .style(schema.role("table-panel.cell").unwrap().id())
        .unwrap();
    assert!(
        matches!(cell.property(Property::FontSize),Some(ThemeValue::LogicalPixels(size)) if size.get()==27.0)
    );
    let header = appearance
        .style(schema.role("table-panel.header-cell").unwrap().id())
        .unwrap();
    assert!(
        matches!(header.property(Property::FontSize),Some(ThemeValue::LogicalPixels(size)) if size.get()==32.0)
    );
}
