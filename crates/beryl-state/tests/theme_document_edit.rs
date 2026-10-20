use std::collections::BTreeMap;

use beryl_state::{
    CANONICAL_THEME_PROPERTY_ENTRY_COUNT_MAX, ThemeColor, ThemeDocument, ThemeDocumentError,
    ThemeFontFamily, ThemeParseMode, ThemePropertyId, ThemePropertySource, ThemeValue,
    canonical_theme_schema,
};

fn document() -> ThemeDocument {
    ThemeDocument::parse_bytes(
        br##"schema = 1
id = "original"
name = "Original"
[[role]]
id = "app.window"
background = "#102030"
"##,
        ThemeParseMode::StrictCandidate,
    )
    .unwrap()
}

#[test]
fn property_batch_preserves_document_identity_and_unrelated_properties() {
    let original = document();
    let role = canonical_theme_schema()
        .role("settings.window")
        .unwrap()
        .id()
        .clone();
    let next = original
        .with_property_sources(BTreeMap::from([(
            (role.clone(), ThemePropertyId::Background),
            ThemePropertySource::Concrete(ThemeValue::Color(ThemeColor::from_rgb(4, 5, 6))),
        )]))
        .unwrap();
    assert_eq!(next.id(), original.id());
    assert_eq!(next.name(), original.name());
    let app = canonical_theme_schema().role("app.window").unwrap().id();
    assert_eq!(
        next.definition().roles().get(app),
        original.definition().roles().get(app)
    );
    assert_eq!(
        next.definition()
            .roles()
            .get(&role)
            .unwrap()
            .properties()
            .get(&ThemePropertyId::Background),
        Some(&ThemePropertySource::Concrete(ThemeValue::Color(
            ThemeColor::from_rgb(4, 5, 6)
        )))
    );
    assert_eq!(
        ThemeDocument::parse_bytes(
            next.to_canonical_toml().unwrap().as_bytes(),
            ThemeParseMode::StrictCandidate
        )
        .unwrap(),
        next
    );
    assert!(!original.definition().roles().contains_key(&role));
}

#[test]
fn invalid_value_and_ineligible_source_refuse_the_entire_batch() {
    let original = document();
    let before = original.to_canonical_toml().unwrap();
    let role = canonical_theme_schema()
        .role("settings.window")
        .unwrap()
        .id()
        .clone();
    for source in [
        ThemePropertySource::Concrete(ThemeValue::FontFamily(
            ThemeFontFamily::new("Font").unwrap(),
        )),
        ThemePropertySource::AmbientParent,
    ] {
        assert!(
            original
                .with_property_source(role.clone(), ThemePropertyId::Background, source)
                .is_err()
        );
        assert_eq!(original.to_canonical_toml().unwrap(), before);
    }
    assert!(
        original
            .with_property_source(
                role,
                ThemePropertyId::FontWeight,
                ThemePropertySource::Fallback
            )
            .is_err()
    );
    assert_eq!(original.to_canonical_toml().unwrap(), before);
}

#[test]
fn oversized_change_map_is_refused_before_editing_or_validation() {
    let original = document();
    let mut changes = BTreeMap::new();
    'roles: for role in canonical_theme_schema().roles().keys() {
        for property in ThemePropertyId::ALL {
            changes.insert((role.clone(), property), ThemePropertySource::Fallback);
            if changes.len() > CANONICAL_THEME_PROPERTY_ENTRY_COUNT_MAX {
                break 'roles;
            }
        }
    }
    assert_eq!(changes.len(), CANONICAL_THEME_PROPERTY_ENTRY_COUNT_MAX + 1);
    assert!(matches!(
        original.with_property_sources(changes),
        Err(ThemeDocumentError::TooManyPropertyEntries { .. })
    ));
    assert_eq!(original, document());
}
