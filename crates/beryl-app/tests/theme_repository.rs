#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::fs;

use beryl_app::{
    AppearanceSettings, AppearanceSettingsStore, BerylThemeProperty, BerylThemeRole,
    InstalledThemeId, MAX_THEME_FONT_FAMILY_BYTES, StylePropertyId, StylePropertySource,
    StylePropertyValue, ThemeDefinition, ThemeDiagnosticKind, ThemeDocument, ThemeRepositoryError,
    ThemeRepositoryStore, ThemeResolutionContext, ThemeRoleDefinition,
};

#[test]
fn missing_repository_loads_built_in_theme_without_touching_legacy_theme_file() {
    let root = unique_temp_dir();
    let store = ThemeRepositoryStore::new(&root);
    let legacy_store = AppearanceSettingsStore::new(&root);
    fs::write(legacy_store.theme_path(), b"legacy theme contents").unwrap();

    let snapshot = store.load_or_default().unwrap();

    assert_eq!(snapshot.active_theme_id().as_str(), "built-in");
    assert_eq!(snapshot.themes().len(), 1);
    assert!(snapshot.themes()[0].is_built_in());
    assert_eq!(
        fs::read(legacy_store.theme_path()).unwrap(),
        b"legacy theme contents"
    );
    assert!(!store.manifest_path().exists());
    cleanup_temp_dir(root);
}

#[test]
fn save_as_persists_multiple_themes_and_active_identity_across_reload() {
    let root = unique_temp_dir();
    let store = ThemeRepositoryStore::new(&root);

    let first = store
        .save_as_theme("Ocean", theme_definition("#102030"))
        .unwrap();
    let second = store
        .save_as_theme("Forest", theme_definition("#203010"))
        .unwrap();
    let reloaded = ThemeRepositoryStore::new(&root).load_or_default().unwrap();

    assert_eq!(first.themes().len(), 2);
    assert_eq!(second.themes().len(), 3);
    assert_eq!(reloaded.active_theme_id().as_str(), "forest");
    assert!(
        reloaded
            .themes()
            .iter()
            .any(|theme| theme.id().as_str() == "ocean" && theme.name() == "Ocean")
    );
    assert!(
        store
            .theme_document_path(&InstalledThemeId::from("forest"))
            .exists()
    );
    cleanup_temp_dir(root);
}

#[test]
fn activating_installed_theme_updates_active_projection_after_reload() {
    let root = unique_temp_dir();
    let store = ThemeRepositoryStore::new(&root);
    store
        .save_as_theme("Ocean", theme_definition("#102030"))
        .unwrap();
    store
        .save_as_theme("Forest", theme_definition("#203010"))
        .unwrap();

    let snapshot = store
        .activate_theme(&InstalledThemeId::from("ocean"))
        .unwrap();
    let reloaded = ThemeRepositoryStore::new(&root).load_or_default().unwrap();

    assert_eq!(snapshot.active_theme_id().as_str(), "ocean");
    assert_eq!(reloaded.active_theme_id().as_str(), "ocean");
    assert_eq!(
        active_foreground(&reloaded),
        StylePropertyValue::color("#102030")
    );
    cleanup_temp_dir(root);
}

#[test]
fn install_theme_persists_candidate_without_activating_it() {
    let root = unique_temp_dir();
    let store = ThemeRepositoryStore::new(&root);
    store
        .save_as_theme("Ocean", theme_definition("#102030"))
        .unwrap();

    let snapshot = store
        .install_theme("Candidate", theme_definition("#405060"))
        .unwrap();
    let reloaded = ThemeRepositoryStore::new(&root).load_or_default().unwrap();

    assert_eq!(snapshot.active_theme_id().as_str(), "ocean");
    assert_eq!(reloaded.active_theme_id().as_str(), "ocean");
    assert!(
        reloaded
            .themes()
            .iter()
            .any(|theme| theme.id().as_str() == "candidate"
                && theme.name() == "Candidate"
                && !theme.is_active())
    );
    assert_eq!(
        active_foreground(&reloaded),
        StylePropertyValue::color("#102030")
    );
    cleanup_temp_dir(root);
}

#[test]
fn deleting_active_theme_recovers_to_first_remaining_theme() {
    let root = unique_temp_dir();
    let store = ThemeRepositoryStore::new(&root);
    store
        .save_as_theme("Ocean", theme_definition("#102030"))
        .unwrap();
    store
        .save_as_theme("Forest", theme_definition("#203010"))
        .unwrap();

    let snapshot = store
        .delete_theme(&InstalledThemeId::from("forest"))
        .unwrap();
    let reloaded = ThemeRepositoryStore::new(&root).load_or_default().unwrap();

    assert_eq!(snapshot.active_theme_id().as_str(), "ocean");
    assert_eq!(reloaded.active_theme_id().as_str(), "ocean");
    assert!(
        !store
            .theme_document_path(&InstalledThemeId::from("forest"))
            .exists()
    );
    cleanup_temp_dir(root);
}

#[test]
fn corrupt_manifest_and_theme_documents_recover_to_valid_theme_set() {
    let root = unique_temp_dir();
    let store = ThemeRepositoryStore::new(&root);
    fs::create_dir_all(store.theme_documents_dir()).unwrap();
    fs::write(store.manifest_path(), b"not toml").unwrap();
    fs::write(
        store.theme_document_path(&InstalledThemeId::from("valid")),
        ThemeDocument::new(
            Some(InstalledThemeId::from("valid")),
            Some("Valid".to_string()),
            theme_definition("#112233"),
        )
        .unwrap()
        .to_toml_string()
        .unwrap(),
    )
    .unwrap();
    fs::write(
        store.theme_document_path(&InstalledThemeId::from("corrupt")),
        b"schema = 1\n[[role]]\nid = ",
    )
    .unwrap();
    fs::write(store.theme_documents_dir().join("partial.tmp"), b"ignored").unwrap();

    let snapshot = store.load_or_default().unwrap();

    assert_eq!(snapshot.active_theme_id().as_str(), "built-in");
    assert!(
        snapshot
            .themes()
            .iter()
            .any(|theme| theme.id().as_str() == "valid")
    );
    assert!(
        !snapshot
            .themes()
            .iter()
            .any(|theme| theme.id().as_str() == "corrupt")
    );
    cleanup_temp_dir(root);
}

#[test]
fn oversized_font_family_theme_document_is_skipped_on_repository_load() {
    let root = unique_temp_dir();
    let store = ThemeRepositoryStore::new(&root);
    fs::create_dir_all(store.theme_documents_dir()).unwrap();
    fs::write(
        store.theme_document_path(&InstalledThemeId::from("oversized")),
        format!(
            r##"
schema = 1
id = "oversized"
name = "Oversized"

[[role]]
id = "code_panel.body"
font_family = {{ value = "{}" }}
"##,
            "F".repeat(MAX_THEME_FONT_FAMILY_BYTES + 1)
        ),
    )
    .unwrap();

    let snapshot = store.load_or_default().unwrap();

    assert!(
        !snapshot
            .themes()
            .iter()
            .any(|theme| theme.id().as_str() == "oversized")
    );
    cleanup_temp_dir(root);
}

#[test]
fn unsupported_persisted_properties_are_ignored_and_not_reserialized() {
    let root = unique_temp_dir();
    let store = ThemeRepositoryStore::new(&root);
    fs::create_dir_all(store.theme_documents_dir()).unwrap();
    fs::write(
        store.manifest_path(),
        r#"schema = 1
active_theme_id = "legacy"

[[theme]]
id = "legacy"
name = "Legacy"
file = "legacy.toml"
"#,
    )
    .unwrap();
    fs::write(
        store.theme_document_path(&InstalledThemeId::from("legacy")),
        r##"
schema = 1
id = "legacy"
name = "Legacy"

[[role]]
id = "app.window"
foreground = { value = "#112233" }
not_a_property = { value = "#445566" }
"##,
    )
    .unwrap();

    let snapshot = store.load_or_default().unwrap();

    assert_eq!(snapshot.active_theme_id().as_str(), "legacy");
    assert_eq!(
        active_foreground(&snapshot),
        StylePropertyValue::color("#112233")
    );
    let definition = store
        .load_theme_definition(&InstalledThemeId::from("legacy"))
        .unwrap();
    let app = definition
        .roles()
        .iter()
        .find(|role| role.role_id().as_str() == BerylThemeRole::AppWindow.id())
        .unwrap();
    assert!(
        !app.properties()
            .contains_key(&StylePropertyId::from("not_a_property"))
    );

    store
        .rename_theme(&InstalledThemeId::from("legacy"), "Renamed")
        .unwrap();
    let persisted =
        fs::read_to_string(store.theme_document_path(&InstalledThemeId::from("legacy"))).unwrap();

    assert!(ThemeDocument::from_toml_str(&persisted).is_ok());
    assert!(persisted.contains("foreground = { value = \"#112233\" }"));
    assert!(!persisted.contains("not_a_property"));
    cleanup_temp_dir(root);
}

#[test]
fn old_persisted_separator_border_is_ignored_and_falls_back_to_color() {
    let root = unique_temp_dir();
    let store = ThemeRepositoryStore::new(&root);
    write_single_theme_repository(
        &store,
        "legacy-separator",
        r##"
schema = 1
id = "legacy-separator"
name = "Legacy Separator"

[[role]]
id = "main.separator"
border = { value = "#ff0000" }
"##,
    );

    let snapshot = store.load_or_default().unwrap();

    assert_eq!(snapshot.active_theme_id().as_str(), "legacy-separator");
    assert_eq!(
        active_separator_color(&snapshot),
        StylePropertyValue::color("#334155")
    );
    let definition = store
        .load_theme_definition(&InstalledThemeId::from("legacy-separator"))
        .unwrap();
    let separator = theme_role(&definition, BerylThemeRole::MainSeparator);
    assert!(
        !separator
            .properties()
            .contains_key(&StylePropertyId::from(BerylThemeProperty::Border.id()))
    );

    store
        .rename_theme(&InstalledThemeId::from("legacy-separator"), "Renamed")
        .unwrap();
    let persisted =
        fs::read_to_string(store.theme_document_path(&InstalledThemeId::from("legacy-separator")))
            .unwrap();

    assert!(ThemeDocument::from_toml_str(&persisted).is_ok());
    assert!(!role_record_text(&persisted, BerylThemeRole::MainSeparator.id()).contains("border ="));
    cleanup_temp_dir(root);
}

#[test]
fn persisted_separator_color_wins_over_old_border_on_load() {
    let root = unique_temp_dir();
    let store = ThemeRepositoryStore::new(&root);
    write_single_theme_repository(
        &store,
        "mixed-separator",
        r##"
schema = 1
id = "mixed-separator"
name = "Mixed Separator"

[[role]]
id = "main.separator"
border = { value = "#ff0000" }
color = { value = "#010203" }
"##,
    );

    let snapshot = store.load_or_default().unwrap();

    assert_eq!(snapshot.active_theme_id().as_str(), "mixed-separator");
    assert_eq!(
        active_separator_color(&snapshot),
        StylePropertyValue::color("#010203")
    );

    store
        .rename_theme(&InstalledThemeId::from("mixed-separator"), "Renamed")
        .unwrap();
    let persisted =
        fs::read_to_string(store.theme_document_path(&InstalledThemeId::from("mixed-separator")))
            .unwrap();
    let separator_record = role_record_text(&persisted, BerylThemeRole::MainSeparator.id());

    assert!(ThemeDocument::from_toml_str(&persisted).is_ok());
    assert!(separator_record.contains("color = { value = \"#010203\" }"));
    assert!(!separator_record.contains("border ="));
    cleanup_temp_dir(root);
}

#[test]
fn repository_write_paths_reject_unsupported_properties_without_mutating() {
    let root = unique_temp_dir();
    let store = ThemeRepositoryStore::new(&root);
    store
        .save_as_theme("Ocean", theme_definition("#102030"))
        .unwrap();
    let invalid = unsupported_property_definition();

    let install_error = store
        .install_theme("Unsupported Install", invalid.clone())
        .unwrap_err();
    assert_unknown_property_error(install_error);
    let save_as_error = store
        .save_as_theme("Unsupported Copy", invalid.clone())
        .unwrap_err();
    assert_unknown_property_error(save_as_error);
    let update_error = store
        .update_theme(&InstalledThemeId::from("ocean"), invalid)
        .unwrap_err();
    assert_unknown_property_error(update_error);

    let reloaded = store.load_or_default().unwrap();
    assert_eq!(reloaded.active_theme_id().as_str(), "ocean");
    assert_eq!(
        active_foreground(&reloaded),
        StylePropertyValue::color("#102030")
    );
    assert!(
        !store
            .theme_document_path(&InstalledThemeId::from("unsupported-install"))
            .exists()
    );
    assert!(
        !store
            .theme_document_path(&InstalledThemeId::from("unsupported-copy"))
            .exists()
    );
    let ocean_text =
        fs::read_to_string(store.theme_document_path(&InstalledThemeId::from("ocean"))).unwrap();
    assert!(!ocean_text.contains("not_a_property"));
    cleanup_temp_dir(root);
}

#[test]
fn duplicate_discovered_theme_names_are_recovered_deterministically() {
    let root = unique_temp_dir();
    let store = ThemeRepositoryStore::new(&root);
    fs::create_dir_all(store.theme_documents_dir()).unwrap();

    for id in ["alpha", "beta"] {
        fs::write(
            store.theme_document_path(&InstalledThemeId::from(id)),
            ThemeDocument::new(
                Some(InstalledThemeId::from(id)),
                Some("Duplicate".to_string()),
                theme_definition("#112233"),
            )
            .unwrap()
            .to_toml_string()
            .unwrap(),
        )
        .unwrap();
    }

    let names: Vec<_> = store
        .load_or_default()
        .unwrap()
        .themes()
        .iter()
        .filter(|theme| !theme.is_built_in())
        .map(|theme| theme.name().to_string())
        .collect();

    assert_eq!(names, vec!["Duplicate", "Duplicate 2"]);
    cleanup_temp_dir(root);
}

#[test]
fn legacy_theme_file_is_preserved_across_repository_operations() {
    let root = unique_temp_dir();
    let store = ThemeRepositoryStore::new(&root);
    let legacy_store = AppearanceSettingsStore::new(&root);
    fs::write(legacy_store.theme_path(), b"legacy bytes").unwrap();

    store
        .save_as_theme("Ocean", theme_definition("#102030"))
        .unwrap();
    store
        .activate_theme(&InstalledThemeId::from("ocean"))
        .unwrap();
    store
        .delete_theme(&InstalledThemeId::from("ocean"))
        .unwrap();

    assert_eq!(
        fs::read(legacy_store.theme_path()).unwrap(),
        b"legacy bytes"
    );
    cleanup_temp_dir(root);
}

fn theme_definition(foreground: &str) -> beryl_app::ThemeDefinition {
    let mut settings = AppearanceSettings::default();
    settings.general_ui.foreground = foreground.to_string();
    settings.to_theme_definition().unwrap()
}

fn active_foreground(snapshot: &beryl_app::ThemeRepositorySnapshot) -> StylePropertyValue {
    snapshot
        .active_projection()
        .resolve_property(
            BerylThemeRole::AppWindow.id(),
            BerylThemeProperty::Foreground.id(),
            &ThemeResolutionContext::new(),
        )
        .unwrap()
}

fn active_separator_color(snapshot: &beryl_app::ThemeRepositorySnapshot) -> StylePropertyValue {
    snapshot
        .active_projection()
        .resolve_property(
            BerylThemeRole::MainSeparator.id(),
            BerylThemeProperty::Color.id(),
            &ThemeResolutionContext::new(),
        )
        .unwrap()
}

fn unsupported_property_definition() -> beryl_app::ThemeDefinition {
    beryl_app::ThemeDefinition::new(vec![
        ThemeRoleDefinition::new(BerylThemeRole::AppWindow.id())
            .with_property("not_a_property", StylePropertySource::Fallback),
    ])
}

fn write_single_theme_repository(store: &ThemeRepositoryStore, id: &str, document: &str) {
    fs::create_dir_all(store.theme_documents_dir()).unwrap();
    fs::write(
        store.manifest_path(),
        format!(
            r#"schema = 1
active_theme_id = "{id}"

[[theme]]
id = "{id}"
name = "Loaded"
file = "{id}.toml"
"#
        ),
    )
    .unwrap();
    fs::write(
        store.theme_document_path(&InstalledThemeId::from(id)),
        document,
    )
    .unwrap();
}

fn theme_role(definition: &ThemeDefinition, role: BerylThemeRole) -> &ThemeRoleDefinition {
    definition
        .roles()
        .iter()
        .find(|definition_role| definition_role.role_id().as_str() == role.id())
        .expect("theme role should exist")
}

fn role_record_text<'a>(document: &'a str, role_id: &str) -> &'a str {
    let role_id_line = format!("id = \"{role_id}\"");
    document
        .split("[[role]]")
        .skip(1)
        .find(|section| section.contains(&role_id_line))
        .expect("theme document role should be present")
}

fn assert_unknown_property_error(error: ThemeRepositoryError) {
    let ThemeRepositoryError::InvalidThemeDefinition { source } = error else {
        panic!("expected invalid theme definition error");
    };
    assert!(
        source
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind() == ThemeDiagnosticKind::UnknownProperty)
    );
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-theme-repository-test-")
}

fn cleanup_temp_dir(root: tempdir_support::TestTempDir) {
    root.close().unwrap();
}
