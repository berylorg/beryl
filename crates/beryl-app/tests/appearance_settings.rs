use beryl_app::{AppearanceSettings, MAX_THEME_FONT_FAMILY_BYTES};

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
