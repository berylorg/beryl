#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::fs;

use beryl_app::{
    AppearanceRoleSettings, AppearanceSettings, AppearanceSettingsStore,
    MAX_THEME_FONT_FAMILY_BYTES,
};

#[test]
fn appearance_settings_store_ignores_legacy_theme_toml() {
    let root = unique_temp_dir();
    let store = AppearanceSettingsStore::new(&root);
    let mut settings = AppearanceSettings::default();
    settings.general_ui = AppearanceRoleSettings::new("Segoe UI", 15.0, 500, "#F8FAFC", "#020617");
    settings.code = AppearanceRoleSettings::new("Cascadia Mono", 13.0, 400, "#E2E8F0", "#0F172A");
    settings.transcript_reasoning.foreground = "#CBD5E1".to_string();
    settings.transcript_commentary.foreground = "#BAE6FD".to_string();
    settings.chrome.toolbar_background = "#111827".to_string();
    settings.chrome.primary_button.font_weight = 650;
    settings.chrome.primary_button.normal.background = "#1D4ED8".to_string();

    fs::write(store.theme_path(), b"legacy theme contents").unwrap();
    store.save(&settings).unwrap();

    let loaded = store.load_or_default().unwrap();
    assert_eq!(loaded, AppearanceSettings::default());
    assert_eq!(
        fs::read(store.theme_path()).unwrap(),
        b"legacy theme contents"
    );

    root.close().unwrap();
}

#[test]
fn appearance_settings_save_leaves_legacy_theme_file_absent() {
    let root = unique_temp_dir();
    let store = AppearanceSettingsStore::new(&root);

    store.save(&AppearanceSettings::default()).unwrap();

    assert!(!store.theme_path().exists());
    root.close().unwrap();
}

#[test]
fn appearance_settings_validate_configurable_role_fields() {
    let mut settings = AppearanceSettings::default();
    settings.emphasis.font_family = " ".to_string();
    assert!(settings.validated().is_err());

    let mut settings = AppearanceSettings::default();
    settings.emphasis.font_family = "F".repeat(MAX_THEME_FONT_FAMILY_BYTES + 1);
    assert!(settings.validated().is_err());

    let mut settings = AppearanceSettings::default();
    settings.strong_emphasis.foreground = "slate".to_string();
    assert!(settings.validated().is_err());

    let mut settings = AppearanceSettings::default();
    settings.transcript_commentary.foreground = "sky".to_string();
    assert!(settings.validated().is_err());

    let mut settings = AppearanceSettings::default();
    settings.markdown_header.font_size = 64.0;
    assert!(settings.validated().is_err());

    let mut settings = AppearanceSettings::default();
    settings.chrome.primary_button.hover.border = "blue".to_string();
    assert!(settings.validated().is_err());

    let mut settings = AppearanceSettings::default();
    settings.chrome.secondary_button.font_weight = 950;
    assert!(settings.validated().is_err());
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-appearance-settings-test-")
}
