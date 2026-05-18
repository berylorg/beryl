use beryl_app::{
    AppearanceSettings, BerylThemeProperty, BerylThemeRole, MAX_THEME_FONT_FAMILY_BYTES,
    StylePropertySource, StylePropertyValue, ThemeDiagnosticKind, ThemeDocument,
    ThemeDocumentError,
};

#[test]
fn compact_theme_document_roundtrips_role_records_and_sources() {
    let mut settings = AppearanceSettings::default();
    settings.code.foreground = "#010203".to_string();
    settings.code.font_family = "Cascadia Code".to_string();
    let document = ThemeDocument::new(
        None,
        Some("Compact Theme".to_string()),
        settings.to_theme_definition().unwrap(),
    )
    .unwrap();

    let text = document.to_toml_string().unwrap();

    assert!(text.contains("[[role]]"));
    assert!(text.contains("foreground = { value = \"#010203\" }"));
    assert!(text.contains("background = { value = "));
    assert!(text.contains("ambient_parent"));

    let parsed = ThemeDocument::from_toml_str(&text).unwrap();
    assert_eq!(parsed.name(), Some("Compact Theme"));
    let code = parsed
        .definition()
        .roles()
        .iter()
        .find(|role| role.role_id().as_str() == BerylThemeRole::CodePanelBodyText.id())
        .unwrap();
    assert_eq!(
        code.properties()
            .get(&BerylThemeProperty::Foreground.id().into()),
        Some(&StylePropertySource::Concrete(StylePropertyValue::color(
            "#010203"
        )))
    );
}

#[test]
fn compact_theme_document_accepts_source_keywords_and_inline_values() {
    let text = r##"
schema = 1
name = "Keyword Theme"

[[role]]
id = "app.window"
foreground = { value = "#112233" }
background = "fallback"

[[role]]
id = "app.window.title"
font_weight = { value = 500 }

[[role]]
id = "code_panel.body"
background = "fallback"

[[role]]
id = "code_panel.body.text"
foreground = { value = "#ddeeff" }
font_family = { value = "Inter" }
font_size = { value = 15.0 }
font_weight = { value = 400 }

[[role]]
id = "markdown.inline_code"
text_background = "ambient_parent"
foreground = "static_parent"
"##;

    let document = ThemeDocument::from_toml_str(text).unwrap();
    let inline = document
        .definition()
        .roles()
        .iter()
        .find(|role| role.role_id().as_str() == BerylThemeRole::MarkdownInlineCode.id())
        .unwrap();

    assert_eq!(
        inline
            .properties()
            .get(&BerylThemeProperty::TextBackground.id().into()),
        Some(&StylePropertySource::AmbientParent)
    );
    assert_eq!(
        inline
            .properties()
            .get(&BerylThemeProperty::Foreground.id().into()),
        Some(&StylePropertySource::StaticParent)
    );
}

#[test]
fn compact_theme_document_serialization_preserves_omitted_properties() {
    let text = r##"
schema = 1
name = "Sparse Theme"

[[role]]
id = "app.window"
foreground = { value = "#112233" }

[[role]]
id = "code_panel.body.text"
font_family = "fallback"
"##;

    let document = ThemeDocument::from_toml_str(text).unwrap();
    let serialized = document.to_toml_string().unwrap();
    let parsed = ThemeDocument::from_toml_str(&serialized).unwrap();
    let app = parsed
        .definition()
        .roles()
        .iter()
        .find(|role| role.role_id().as_str() == BerylThemeRole::AppWindow.id())
        .unwrap();

    assert_eq!(
        app.properties()
            .get(&BerylThemeProperty::Foreground.id().into()),
        Some(&StylePropertySource::Concrete(StylePropertyValue::color(
            "#112233"
        )))
    );
    assert!(
        !app.properties()
            .contains_key(&BerylThemeProperty::Background.id().into())
    );
    let code = parsed
        .definition()
        .roles()
        .iter()
        .find(|role| role.role_id().as_str() == BerylThemeRole::CodePanelBodyText.id())
        .unwrap();
    assert_eq!(
        code.properties()
            .get(&BerylThemeProperty::FontFamily.id().into()),
        Some(&StylePropertySource::Fallback)
    );
    assert!(
        !role_record_text(&serialized, BerylThemeRole::AppWindow.id()).contains("background =")
    );
}

#[test]
fn compact_theme_document_rejects_duplicate_role_records() {
    let text = r##"
schema = 1

[[role]]
id = "app.window"
foreground = { value = "#112233" }

[[role]]
id = "app.window"
background = { value = "#223344" }
"##;

    let error = ThemeDocument::from_toml_str(text).unwrap_err();

    assert_validation_kind(error, ThemeDiagnosticKind::DuplicateRole);
}

#[test]
fn compact_theme_document_rejects_unknown_properties() {
    let text = r##"
schema = 1

[[role]]
id = "app.window"
not_a_property = { value = "#112233" }
"##;

    let error = ThemeDocument::from_toml_str(text).unwrap_err();

    assert_validation_kind(error, ThemeDiagnosticKind::UnknownProperty);
}

#[test]
fn compact_theme_document_accepts_separator_color() {
    let text = r##"
schema = 1

[[role]]
id = "main.separator"
color = { value = "#112233" }
"##;

    let document = ThemeDocument::from_toml_str(text).unwrap();
    let separator = document
        .definition()
        .roles()
        .iter()
        .find(|role| role.role_id().as_str() == BerylThemeRole::MainSeparator.id())
        .unwrap();

    assert_eq!(
        separator
            .properties()
            .get(&BerylThemeProperty::Color.id().into()),
        Some(&StylePropertySource::Concrete(StylePropertyValue::color(
            "#112233"
        )))
    );
}

#[test]
fn compact_theme_document_rejects_unsupported_separator_properties() {
    for property in [
        "border",
        "foreground",
        "text_background",
        "font_family",
        "font_size",
        "font_weight",
    ] {
        let text = format!(
            r##"
schema = 1

[[role]]
id = "main.separator"
{property} = "fallback"
"##
        );

        let error = ThemeDocument::from_toml_str(&text).unwrap_err();

        assert_validation_kind(error, ThemeDiagnosticKind::UnknownProperty);
    }
}

#[test]
fn compact_theme_document_rejects_non_single_primitive_color_property() {
    let text = r##"
schema = 1

[[role]]
id = "app.window"
color = { value = "#112233" }
"##;

    let error = ThemeDocument::from_toml_str(text).unwrap_err();

    assert_validation_kind(error, ThemeDiagnosticKind::UnknownProperty);
}

#[test]
fn compact_theme_document_rejects_properties_outside_role_capabilities() {
    for (role_id, property) in [
        ("syntax.string", "background"),
        ("code_panel.body", "border"),
        ("code_panel.body", "foreground"),
        ("code_panel.body", "font_family"),
        ("code_panel.header", "foreground"),
        ("code_panel.header", "font_weight"),
        ("code_panel.border", "border"),
        ("markdown.block_quote", "border"),
        ("markdown.thematic_break", "border"),
        ("transcript.turn.user", "font_size"),
        ("transcript.selection", "background"),
        ("button", "font_weight"),
        ("interaction.pressed", "font_weight"),
        ("input.caret", "background"),
        ("scrollbar.thumb.normal", "background"),
        ("scrollbar.thumb.hover", "background"),
        ("popup.surface", "font_weight"),
        ("notice.info", "font_weight"),
        ("notice.error", "font_weight"),
        ("popup.row.normal", "foreground"),
        ("row.info", "font_weight"),
        ("button.primary.normal", "font_weight"),
        ("button.secondary.normal", "font_weight"),
        ("settings.button.primary", "font_weight"),
        ("settings.button.secondary", "font_weight"),
        ("settings.input.focused", "background"),
        ("settings.input.focused", "foreground"),
        ("settings.input.selection", "background"),
        ("settings.row.normal", "font_weight"),
        ("settings.row.disabled", "foreground"),
        ("settings.sidebar.row.text", "background"),
        ("graph.row.topic", "font_weight"),
        ("graph.row.topic.text", "background"),
        ("graph.row.disabled.text", "foreground"),
        ("checklist.status.todo", "foreground"),
        ("checklist.status.todo.text", "color"),
        ("app.window", "font_weight"),
        ("main.toolbar", "font_weight"),
        ("workspace_picker.row.active", "foreground"),
        ("workspace_picker.row.active", "font_weight"),
        ("media.placeholder", "foreground"),
        ("status.value.ok", "background"),
        ("status.value.working", "border"),
        ("activity.indicator.running", "background"),
    ] {
        let text = format!(
            r##"
schema = 1

[[role]]
id = "{role_id}"
{property} = "fallback"
"##
        );

        let error = ThemeDocument::from_toml_str(&text).unwrap_err();

        assert_validation_kind(error, ThemeDiagnosticKind::UnknownProperty);
    }
}

#[test]
fn compact_theme_document_rejects_invalid_value_types() {
    let text = r##"
schema = 1

[[role]]
id = "app.window"
foreground = { value = 12 }
"##;

    let error = ThemeDocument::from_toml_str(text).unwrap_err();

    assert!(matches!(
        error,
        ThemeDocumentError::InvalidPropertySource { .. }
    ));
}

#[test]
fn compact_theme_document_rejects_oversized_font_family_values() {
    let font_family = "F".repeat(MAX_THEME_FONT_FAMILY_BYTES + 1);
    let text = format!(
        r##"
schema = 1

[[role]]
id = "app.window.title"
font_weight = {{ value = 400 }}

[[role]]
id = "code_panel.body.text"
font_family = {{ value = "{font_family}" }}
"##
    );

    let error = ThemeDocument::from_toml_str(&text).unwrap_err();

    assert_validation_kind(error, ThemeDiagnosticKind::InvalidPropertyValue);
}

#[test]
fn compact_theme_document_rejects_static_parent_cycles() {
    let text = r##"
schema = 1

[[role]]
id = "app.window"
static_parent = "main.toolbar"

[[role]]
id = "main.toolbar"
static_parent = "app.window"
"##;

    let error = ThemeDocument::from_toml_str(text).unwrap_err();

    assert_validation_kind(error, ThemeDiagnosticKind::StaticParentCycle);
}

fn assert_validation_kind(error: ThemeDocumentError, kind: ThemeDiagnosticKind) {
    let ThemeDocumentError::Validation { source } = error else {
        panic!("expected validation error");
    };
    assert!(
        source
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind() == kind)
    );
}

fn role_record_text<'a>(document: &'a str, role_id: &str) -> &'a str {
    let role_id_line = format!("id = \"{role_id}\"");
    document
        .split("[[role]]")
        .skip(1)
        .find(|section| section.contains(&role_id_line))
        .expect("theme document role should be present")
}
