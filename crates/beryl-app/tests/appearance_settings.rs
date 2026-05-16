#[path = "support/tempdir.rs"]
mod tempdir_support;

#[cfg(windows)]
use std::fs;

use beryl_app::{AppearanceRoleSettings, AppearanceSettings, AppearanceSettingsStore};

#[test]
fn appearance_settings_roundtrip_through_theme_toml() {
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

    store.save(&settings).unwrap();

    let loaded = store.load_or_default().unwrap();
    assert_eq!(loaded.general_ui.font_family, "Segoe UI");
    assert_eq!(loaded.general_ui.foreground, "#f8fafc");
    assert_eq!(loaded.general_ui.background, "#020617");
    assert_eq!(loaded.code.font_family, "Cascadia Mono");
    assert_eq!(loaded.transcript_reasoning.foreground, "#cbd5e1");
    assert_eq!(loaded.transcript_commentary.foreground, "#bae6fd");
    assert_eq!(loaded.chrome.toolbar_background, "#111827");
    assert_eq!(loaded.chrome.primary_button.font_weight, 650);
    assert_eq!(loaded.chrome.primary_button.normal.background, "#1d4ed8");

    root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn appearance_settings_failed_persist_preserves_existing_theme_file() {
    let root = unique_temp_dir();
    let store = AppearanceSettingsStore::new(&root);
    let mut original = AppearanceSettings::default();
    original.general_ui = AppearanceRoleSettings::new("Segoe UI", 15.0, 500, "#f8fafc", "#020617");
    original.code = AppearanceRoleSettings::new("Cascadia Mono", 13.0, 400, "#e2e8f0", "#0f172a");
    let mut replacement = original.clone();
    replacement.general_ui = AppearanceRoleSettings::new("Inter", 16.0, 600, "#ffffff", "#111827");

    store.save(&original).unwrap();
    let original = original.validated().unwrap();
    let original_text = fs::read_to_string(store.theme_path()).unwrap();
    let lock = tempdir_support::lock_file_against_replacement(&store.theme_path()).unwrap();

    assert!(store.save(&replacement).is_err());
    drop(lock);

    assert_eq!(store.load_or_default().unwrap(), original);
    assert_eq!(
        fs::read_to_string(store.theme_path()).unwrap(),
        original_text
    );
    root.close().unwrap();
}

#[test]
fn appearance_settings_validate_configurable_role_fields() {
    let mut settings = AppearanceSettings::default();
    settings.emphasis.font_family = " ".to_string();
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
