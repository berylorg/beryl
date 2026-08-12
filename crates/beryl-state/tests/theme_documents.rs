use beryl_state::{
    THEME_DOCUMENT_MAX_BYTES, ThemeAmbientContext, ThemeDocument, ThemeDocumentError,
    ThemeParseMode, ThemeResolver, builtin_fallback_appearance, canonical_theme_schema,
};

const VALID: &str = r##"schema = 1
id = "operator-theme"
name = "Operator Theme"

[[role]]
id = "app.window"
background = "#102030"
"##;

#[test]
fn strict_document_round_trips_through_canonical_toml() {
    let document =
        ThemeDocument::parse_bytes(VALID.as_bytes(), ThemeParseMode::StrictCandidate).unwrap();
    let canonical = document.to_canonical_toml().unwrap();
    let reparsed =
        ThemeDocument::parse_bytes(canonical.as_bytes(), ThemeParseMode::StrictCandidate).unwrap();

    assert_eq!(document, reparsed);
    assert_eq!(document.id().unwrap().as_str(), "operator-theme");
    assert_eq!(document.name(), Some("Operator Theme"));
    assert!(ThemeResolver::new(document.definition()).is_ok());
}

#[test]
fn installed_load_omits_unsupported_entries_but_strict_candidates_reject_them() {
    let source = br##"schema = 1
future_top_level = { nested = [1, 2] }

[future_metadata]
label = "ignored"

[[role]]
id = "future.role"
background = "#abcdef"

[[role]]
id = "future.role"
future_table = { nested = [1, 2] }

[[role]]
id = "app.window"
future_property = "#abcdef"
future_array = [1, 2, { nested = true }]
background = "#102030"
"##;

    let installed = ThemeDocument::parse_bytes(source, ThemeParseMode::InstalledLoad).unwrap();
    let canonical = installed.to_canonical_toml().unwrap();
    assert!(!canonical.contains("future.role"));
    assert!(!canonical.contains("future_property"));
    assert!(!canonical.contains("future_array"));
    assert!(!canonical.contains("future_top_level"));
    assert!(!canonical.contains("future_metadata"));
    assert!(matches!(
        ThemeDocument::parse_bytes(source, ThemeParseMode::StrictCandidate),
        Err(ThemeDocumentError::InvalidSyntax { .. })
    ));
}

#[test]
fn parser_rejects_input_beyond_the_document_bound() {
    let source = vec![b' '; THEME_DOCUMENT_MAX_BYTES + 1];
    assert_eq!(
        ThemeDocument::parse_bytes(&source, ThemeParseMode::InstalledLoad),
        Err(ThemeDocumentError::DocumentTooLarge),
    );
}

#[test]
fn builtin_fallback_resolves_every_declared_role_and_ambient_variant() {
    let schema = canonical_theme_schema();
    let appearance = builtin_fallback_appearance();
    assert!(!schema.roles().is_empty());

    for (role_id, role) in schema.roles() {
        let base = appearance.style(role_id).expect("missing fallback role");
        assert_eq!(base.properties().len(), role.properties().len());
        for ambient in ThemeAmbientContext::ALL {
            let contextual = appearance
                .style_in(role_id, ambient)
                .expect("missing ambient fallback role");
            assert_eq!(contextual.properties().len(), role.properties().len());
        }
    }
}
