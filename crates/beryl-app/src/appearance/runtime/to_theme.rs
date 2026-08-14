use std::collections::BTreeMap;

use super::super::{
    ActiveThemeProjection, AppearanceButtonSettings, AppearanceButtonStateSettings,
    AppearanceInputSettings, AppearanceRoleSettings, AppearanceSettings, AppearanceSettingsError,
    AppearanceSurfaceSettings, BerylThemeProperty, BerylThemeRole, StylePropertySource,
    StylePropertyValue, ThemeDefinition, ThemeResolver, ThemeRoleDefinition,
    built_in_theme_definition, built_in_theme_schema, built_in_theme_supports_property,
};

impl AppearanceSettings {
    pub fn to_active_theme_projection(
        &self,
    ) -> Result<ActiveThemeProjection, AppearanceSettingsError> {
        let settings = self.validated()?;
        Ok(theme_projection_from_definition(
            appearance_theme_definition(&settings),
        ))
    }

    pub fn to_theme_definition(&self) -> Result<ThemeDefinition, AppearanceSettingsError> {
        let settings = self.validated()?;
        Ok(appearance_theme_definition(&settings))
    }
}

fn theme_projection_from_definition(definition: ThemeDefinition) -> ActiveThemeProjection {
    let resolver = ThemeResolver::new(built_in_theme_schema(), definition)
        .expect("appearance settings must produce a valid active theme definition");
    ActiveThemeProjection::from_built_in_resolver(resolver)
        .expect("validated active theme definition must resolve built-in roles")
}

fn appearance_theme_definition(settings: &AppearanceSettings) -> ThemeDefinition {
    let overrides = appearance_theme_overrides(settings);
    let roles = built_in_theme_definition()
        .roles()
        .iter()
        .map(|role| {
            let mut definition = ThemeRoleDefinition::new(role.role_id().as_str());
            if let Some(parent) = role.static_parent() {
                definition = definition.with_static_parent(parent.as_str());
            }

            for (property_id, source) in role.properties() {
                let source = overrides
                    .get(&(
                        role.role_id().as_str().to_string(),
                        property_id.as_str().to_string(),
                    ))
                    .cloned()
                    .unwrap_or_else(|| source.clone());
                definition = definition.with_property(property_id.as_str(), source);
            }

            definition
        })
        .collect();
    ThemeDefinition::new(roles)
}

fn appearance_theme_overrides(
    settings: &AppearanceSettings,
) -> BTreeMap<(String, String), StylePropertySource> {
    let mut overrides = BTreeMap::new();

    insert_role_settings(
        &mut overrides,
        BerylThemeRole::AppWindow,
        &settings.general_ui,
    );
    insert_role_settings(
        &mut overrides,
        BerylThemeRole::TranscriptAssistantFinal,
        &settings.conversation_text,
    );
    insert_foreground(
        &mut overrides,
        BerylThemeRole::TranscriptAssistantReasoning,
        &settings.transcript_reasoning.foreground,
    );
    insert_foreground(
        &mut overrides,
        BerylThemeRole::TranscriptAssistantCommentary,
        &settings.transcript_commentary.foreground,
    );
    insert_role_settings(
        &mut overrides,
        BerylThemeRole::MarkdownHeading,
        &settings.markdown_header,
    );
    insert_code_settings(&mut overrides, &settings.code);
    insert_role_settings(
        &mut overrides,
        BerylThemeRole::MarkdownEmphasis,
        &settings.emphasis,
    );
    insert_role_settings(
        &mut overrides,
        BerylThemeRole::MarkdownStrongEmphasis,
        &settings.strong_emphasis,
    );

    insert_color(
        &mut overrides,
        BerylThemeRole::MainToolbar,
        BerylThemeProperty::Background,
        &settings.chrome.toolbar_background,
    );
    insert_color(
        &mut overrides,
        BerylThemeRole::MainThreadStrip,
        BerylThemeProperty::Background,
        &settings.chrome.conversation_thread_strip_background,
    );
    insert_color(
        &mut overrides,
        BerylThemeRole::MainSeparator,
        BerylThemeProperty::Color,
        &settings.chrome.separator,
    );
    insert_button_settings(
        &mut overrides,
        &settings.chrome.primary_button,
        BerylThemeRole::ButtonPrimaryNormal,
        BerylThemeRole::ButtonPrimaryLabel,
        BerylThemeRole::ButtonPrimaryHover,
        BerylThemeRole::ButtonPrimaryActive,
        BerylThemeRole::ButtonPrimaryDisabled,
    );
    insert_button_settings(
        &mut overrides,
        &settings.chrome.secondary_button,
        BerylThemeRole::ButtonSecondaryNormal,
        BerylThemeRole::ButtonSecondaryLabel,
        BerylThemeRole::ButtonSecondaryHover,
        BerylThemeRole::ButtonSecondaryActive,
        BerylThemeRole::ButtonSecondaryDisabled,
    );
    insert_input_settings(
        &mut overrides,
        BerylThemeRole::InputField,
        Some(BerylThemeRole::InputFieldText),
        Some(BerylThemeRole::InputPanel),
        &settings.chrome.input,
    );
    insert_input_settings(
        &mut overrides,
        BerylThemeRole::SettingsInputNormal,
        Some(BerylThemeRole::SettingsInputText),
        None,
        &settings.chrome.input,
    );
    insert_color(
        &mut overrides,
        BerylThemeRole::TranscriptShell,
        BerylThemeProperty::Background,
        &settings.chrome.transcript_shell.background,
    );
    insert_color(
        &mut overrides,
        BerylThemeRole::TranscriptShell,
        BerylThemeProperty::Foreground,
        &settings.chrome.transcript_shell.foreground,
    );
    insert_color(
        &mut overrides,
        BerylThemeRole::StatusLine,
        BerylThemeProperty::Background,
        &settings.chrome.status_line.background,
    );
    insert_color(
        &mut overrides,
        BerylThemeRole::StatusLine,
        BerylThemeProperty::Foreground,
        &settings.chrome.status_line.title_foreground,
    );
    insert_color(
        &mut overrides,
        BerylThemeRole::StatusValueOk,
        BerylThemeProperty::Foreground,
        &settings.chrome.status_line.value_foreground,
    );
    insert_surface_settings(
        &mut overrides,
        &settings.chrome.surfaces,
        &settings.general_ui.foreground,
    );

    overrides
}

fn insert_role_settings(
    overrides: &mut BTreeMap<(String, String), StylePropertySource>,
    role: BerylThemeRole,
    settings: &AppearanceRoleSettings,
) {
    insert_color(
        overrides,
        role,
        BerylThemeProperty::Foreground,
        &settings.foreground,
    );
    insert_color(
        overrides,
        role,
        BerylThemeProperty::Background,
        &settings.background,
    );
    insert_color(
        overrides,
        role,
        BerylThemeProperty::TextBackground,
        &settings.background,
    );
    insert_font_family(overrides, role, &settings.font_family);
    insert_font_size(overrides, role, settings.font_size);
    insert_font_weight(overrides, role, settings.font_weight);
}

fn insert_code_settings(
    overrides: &mut BTreeMap<(String, String), StylePropertySource>,
    settings: &AppearanceRoleSettings,
) {
    insert_color(
        overrides,
        BerylThemeRole::CodePanelBody,
        BerylThemeProperty::Background,
        &settings.background,
    );
    insert_color(
        overrides,
        BerylThemeRole::CodePanelBodyText,
        BerylThemeProperty::TextBackground,
        &settings.background,
    );
    insert_color(
        overrides,
        BerylThemeRole::CodePanelBodyText,
        BerylThemeProperty::Foreground,
        &settings.foreground,
    );
    insert_font_family(
        overrides,
        BerylThemeRole::CodePanelBodyText,
        &settings.font_family,
    );
    insert_font_size(
        overrides,
        BerylThemeRole::CodePanelBodyText,
        settings.font_size,
    );
    insert_font_weight(
        overrides,
        BerylThemeRole::CodePanelBodyText,
        settings.font_weight,
    );
}

fn insert_foreground(
    overrides: &mut BTreeMap<(String, String), StylePropertySource>,
    role: BerylThemeRole,
    value: &str,
) {
    insert_color(overrides, role, BerylThemeProperty::Foreground, value);
}

fn insert_button_settings(
    overrides: &mut BTreeMap<(String, String), StylePropertySource>,
    settings: &AppearanceButtonSettings,
    normal: BerylThemeRole,
    label: BerylThemeRole,
    hover: BerylThemeRole,
    active: BerylThemeRole,
    disabled: BerylThemeRole,
) {
    insert_font_weight(overrides, label, settings.font_weight);
    insert_button_state(overrides, normal, &settings.normal);
    insert_button_state(overrides, hover, &settings.hover);
    insert_button_state(overrides, active, &settings.active);
    insert_button_state(overrides, disabled, &settings.disabled);
}

fn insert_button_state(
    overrides: &mut BTreeMap<(String, String), StylePropertySource>,
    role: BerylThemeRole,
    settings: &AppearanceButtonStateSettings,
) {
    insert_color(
        overrides,
        role,
        BerylThemeProperty::Background,
        &settings.background,
    );
    insert_color(
        overrides,
        role,
        BerylThemeProperty::Border,
        &settings.border,
    );
    insert_color(
        overrides,
        role,
        BerylThemeProperty::Foreground,
        &settings.foreground,
    );
    insert_color(
        overrides,
        role,
        BerylThemeProperty::TextBackground,
        &settings.background,
    );
}

fn insert_input_settings(
    overrides: &mut BTreeMap<(String, String), StylePropertySource>,
    role: BerylThemeRole,
    text_role: Option<BerylThemeRole>,
    panel_role: Option<BerylThemeRole>,
    settings: &AppearanceInputSettings,
) {
    if let Some(panel_role) = panel_role {
        insert_color(
            overrides,
            panel_role,
            BerylThemeProperty::Background,
            &settings.panel_background,
        );
    }
    insert_color(
        overrides,
        role,
        BerylThemeProperty::Background,
        &settings.input_background,
    );
    insert_color(
        overrides,
        text_role.unwrap_or(role),
        BerylThemeProperty::TextBackground,
        &settings.input_background,
    );
    insert_color(
        overrides,
        role,
        BerylThemeProperty::Border,
        &settings.input_border,
    );
    insert_color(
        overrides,
        text_role.unwrap_or(role),
        BerylThemeProperty::Foreground,
        &settings.input_foreground,
    );
}

fn insert_surface_settings(
    overrides: &mut BTreeMap<(String, String), StylePropertySource>,
    settings: &AppearanceSurfaceSettings,
    foreground: &str,
) {
    for role in [BerylThemeRole::Panel, BerylThemeRole::SettingsGroup] {
        insert_color(
            overrides,
            role,
            BerylThemeProperty::Background,
            &settings.panel_background,
        );
        insert_color(
            overrides,
            role,
            BerylThemeProperty::Border,
            &settings.border,
        );
        insert_color(overrides, role, BerylThemeProperty::Foreground, foreground);
    }
    for role in [
        BerylThemeRole::SurfaceRow,
        BerylThemeRole::SettingsRowNormal,
    ] {
        insert_color(
            overrides,
            role,
            BerylThemeProperty::Background,
            &settings.row_background,
        );
        insert_color(
            overrides,
            role,
            BerylThemeProperty::TextBackground,
            &settings.row_background,
        );
        insert_color(
            overrides,
            role,
            BerylThemeProperty::Border,
            &settings.border,
        );
        insert_color(overrides, role, BerylThemeProperty::Foreground, foreground);
    }
    for role in [BerylThemeRole::PopupSurface, BerylThemeRole::SettingsPopup] {
        insert_color(
            overrides,
            role,
            BerylThemeProperty::Background,
            &settings.popup_background,
        );
        insert_color(
            overrides,
            role,
            BerylThemeProperty::TextBackground,
            &settings.popup_background,
        );
        insert_color(
            overrides,
            role,
            BerylThemeProperty::Border,
            &settings.border,
        );
        insert_color(overrides, role, BerylThemeProperty::Foreground, foreground);
    }
    for role in [
        BerylThemeRole::SurfaceRowDisabled,
        BerylThemeRole::SettingsRowDisabledText,
        BerylThemeRole::TextMuted,
    ] {
        insert_color(
            overrides,
            role,
            BerylThemeProperty::Foreground,
            &settings.muted_foreground,
        );
    }
}

fn insert_color(
    overrides: &mut BTreeMap<(String, String), StylePropertySource>,
    role: BerylThemeRole,
    property: BerylThemeProperty,
    value: &str,
) {
    insert_source(
        overrides,
        role,
        property,
        StylePropertySource::Concrete(StylePropertyValue::color(value)),
    );
}

fn insert_font_family(
    overrides: &mut BTreeMap<(String, String), StylePropertySource>,
    role: BerylThemeRole,
    value: &str,
) {
    insert_source(
        overrides,
        role,
        BerylThemeProperty::FontFamily,
        StylePropertySource::Concrete(StylePropertyValue::font_family(value)),
    );
}

fn insert_font_size(
    overrides: &mut BTreeMap<(String, String), StylePropertySource>,
    role: BerylThemeRole,
    value: f32,
) {
    insert_source(
        overrides,
        role,
        BerylThemeProperty::FontSize,
        StylePropertySource::Concrete(StylePropertyValue::logical_pixels(value)),
    );
}

fn insert_font_weight(
    overrides: &mut BTreeMap<(String, String), StylePropertySource>,
    role: BerylThemeRole,
    value: u16,
) {
    insert_source(
        overrides,
        role,
        BerylThemeProperty::FontWeight,
        StylePropertySource::Concrete(StylePropertyValue::font_weight(value)),
    );
}

fn insert_source(
    overrides: &mut BTreeMap<(String, String), StylePropertySource>,
    role: BerylThemeRole,
    property: BerylThemeProperty,
    source: StylePropertySource,
) {
    if !built_in_theme_supports_property(role, property) {
        return;
    }
    overrides.insert((role.id().to_string(), property.id().to_string()), source);
}
