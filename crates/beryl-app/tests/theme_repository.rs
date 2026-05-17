#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::fs;

use beryl_app::{
    AppearanceSettings, AppearanceSettingsStore, BerylThemeProperty, BerylThemeRole,
    InstalledThemeId, MAX_THEME_FONT_FAMILY_BYTES, StylePropertyValue, ThemeDocument,
    ThemeRepositoryStore, ThemeResolutionContext,
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
id = "app.window"
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

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-theme-repository-test-")
}

fn cleanup_temp_dir(root: tempdir_support::TestTempDir) {
    root.close().unwrap();
}
