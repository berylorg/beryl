#[path = "support/tempdir.rs"]
mod tempdir_support;

use beryl_app::{
    BerylThemeProperty, BerylThemeRole, MAX_THEME_ACTIVE_DOCUMENT_RESPONSE_BYTES,
    MAX_THEME_FONT_FAMILY_BYTES, StylePropertySource, StylePropertyValue, ThemeDefinition,
    ThemeRepositoryStore, ThemeRoleDefinition, built_in_theme_supported_properties,
    theme_repository_value,
};

#[test]
fn repository_read_truncates_oversized_active_document_with_metadata() {
    let root = tempdir_support::temp_dir("beryl-theme-dynamic-tool-repository-test-");
    let snapshot = ThemeRepositoryStore::new(&root)
        .save_as_theme("Verbose", verbose_theme_definition())
        .unwrap();

    let value = theme_repository_value(&snapshot, true).unwrap();
    let active_document = &value["activeDocument"];
    let text = active_document["text"].as_str().unwrap();
    let byte_length = active_document["byteLength"].as_u64().unwrap();
    let retained_byte_length = active_document["retainedByteLength"].as_u64().unwrap();

    assert_eq!(active_document["themeId"], "verbose");
    assert_eq!(active_document["name"], "Verbose");
    assert_eq!(
        active_document["byteLimit"],
        MAX_THEME_ACTIVE_DOCUMENT_RESPONSE_BYTES
    );
    assert_eq!(active_document["truncated"], true);
    assert!(text.len() <= MAX_THEME_ACTIVE_DOCUMENT_RESPONSE_BYTES);
    assert_eq!(retained_byte_length, text.len() as u64);
    assert!(byte_length > retained_byte_length);
    assert_eq!(
        active_document["omittedByteLength"],
        byte_length - retained_byte_length
    );

    root.close().unwrap();
}

fn verbose_theme_definition() -> ThemeDefinition {
    let font_family = "F".repeat(MAX_THEME_FONT_FAMILY_BYTES);
    ThemeDefinition::new(
        BerylThemeRole::ALL
            .iter()
            .copied()
            .map(|role| {
                let mut definition = ThemeRoleDefinition::new(role.id());
                if let Some(parent) = role.static_parent() {
                    definition = definition.with_static_parent(parent.id());
                }
                if built_in_theme_supported_properties(role)
                    .contains(&BerylThemeProperty::FontFamily)
                {
                    definition = definition.with_property(
                        BerylThemeProperty::FontFamily.id(),
                        StylePropertySource::Concrete(StylePropertyValue::font_family(
                            font_family.clone(),
                        )),
                    );
                }
                definition
            })
            .collect(),
    )
}
