use super::super::{
    ActiveThemeProjection, AppearanceButtonSettings, AppearanceButtonStateSettings,
    AppearanceChromeSettings, AppearanceForegroundSettings, AppearanceInputSettings,
    AppearanceRoleSettings, AppearanceSettings, AppearanceStatusLineSettings,
    AppearanceSurfaceSettings, AppearanceTranscriptShellSettings, BerylThemeProperty,
    BerylThemeRole, StylePropertyValue, ThemeResolutionContext,
};

impl AppearanceSettings {
    pub fn from_active_theme(theme: &ActiveThemeProjection) -> Self {
        Self {
            general_ui: role_settings(theme, BerylThemeRole::AppWindow),
            conversation_text: role_settings(theme, BerylThemeRole::TranscriptAssistantFinal),
            transcript_reasoning: foreground_settings(
                theme,
                BerylThemeRole::TranscriptAssistantReasoning,
            ),
            transcript_commentary: foreground_settings(
                theme,
                BerylThemeRole::TranscriptAssistantCommentary,
            ),
            markdown_header: role_settings(theme, BerylThemeRole::MarkdownHeading),
            code: role_settings(theme, BerylThemeRole::CodePanelBody),
            emphasis: role_settings(theme, BerylThemeRole::MarkdownEmphasis),
            strong_emphasis: role_settings(theme, BerylThemeRole::MarkdownStrongEmphasis),
            chrome: AppearanceChromeSettings {
                toolbar_background: color(
                    theme,
                    BerylThemeRole::MainToolbar,
                    BerylThemeProperty::Background,
                    "#020617",
                ),
                conversation_thread_strip_background: color(
                    theme,
                    BerylThemeRole::MainThreadStrip,
                    BerylThemeProperty::Background,
                    "#091220",
                ),
                separator: color(
                    theme,
                    BerylThemeRole::MainSeparator,
                    BerylThemeProperty::Border,
                    "#1e293b",
                ),
                primary_button: button_settings(
                    theme,
                    BerylThemeRole::ButtonPrimaryNormal,
                    BerylThemeRole::ButtonPrimaryHover,
                    BerylThemeRole::ButtonPrimaryActive,
                    BerylThemeRole::ButtonPrimaryDisabled,
                ),
                secondary_button: button_settings(
                    theme,
                    BerylThemeRole::ButtonSecondaryNormal,
                    BerylThemeRole::ButtonSecondaryHover,
                    BerylThemeRole::ButtonSecondaryActive,
                    BerylThemeRole::ButtonSecondaryDisabled,
                ),
                input: AppearanceInputSettings {
                    panel_background: color(
                        theme,
                        BerylThemeRole::InputPanel,
                        BerylThemeProperty::Background,
                        "#020617",
                    ),
                    input_background: color(
                        theme,
                        BerylThemeRole::InputField,
                        BerylThemeProperty::Background,
                        "#0f172a",
                    ),
                    input_border: color(
                        theme,
                        BerylThemeRole::InputField,
                        BerylThemeProperty::Border,
                        "#334155",
                    ),
                    input_foreground: color(
                        theme,
                        BerylThemeRole::InputField,
                        BerylThemeProperty::Foreground,
                        "#e2e8f0",
                    ),
                },
                transcript_shell: AppearanceTranscriptShellSettings {
                    background: color(
                        theme,
                        BerylThemeRole::TranscriptShell,
                        BerylThemeProperty::Background,
                        "#091220",
                    ),
                    foreground: color(
                        theme,
                        BerylThemeRole::TranscriptShell,
                        BerylThemeProperty::Foreground,
                        "#e2e8f0",
                    ),
                },
                status_line: AppearanceStatusLineSettings {
                    background: color(
                        theme,
                        BerylThemeRole::StatusLine,
                        BerylThemeProperty::Background,
                        "#020617",
                    ),
                    title_foreground: color(
                        theme,
                        BerylThemeRole::StatusLine,
                        BerylThemeProperty::Foreground,
                        "#94a3b8",
                    ),
                    value_foreground: color(
                        theme,
                        BerylThemeRole::StatusValueOk,
                        BerylThemeProperty::Foreground,
                        "#e2e8f0",
                    ),
                },
                surfaces: AppearanceSurfaceSettings {
                    panel_background: color(
                        theme,
                        BerylThemeRole::Panel,
                        BerylThemeProperty::Background,
                        "#111827",
                    ),
                    row_background: color(
                        theme,
                        BerylThemeRole::SurfaceRow,
                        BerylThemeProperty::Background,
                        "#1f2937",
                    ),
                    popup_background: color(
                        theme,
                        BerylThemeRole::PopupSurface,
                        BerylThemeProperty::Background,
                        "#111827",
                    ),
                    border: color(
                        theme,
                        BerylThemeRole::Panel,
                        BerylThemeProperty::Border,
                        "#374151",
                    ),
                    muted_foreground: color(
                        theme,
                        BerylThemeRole::SurfaceRowDisabled,
                        BerylThemeProperty::Foreground,
                        "#94a3b8",
                    ),
                },
            },
        }
    }
}

fn role_settings(theme: &ActiveThemeProjection, role: BerylThemeRole) -> AppearanceRoleSettings {
    AppearanceRoleSettings::new(
        font_family(theme, role, "Inter"),
        font_size(theme, role, 14.0),
        font_weight(theme, role, 400),
        color(theme, role, BerylThemeProperty::Foreground, "#e2e8f0"),
        color(theme, role, BerylThemeProperty::Background, "#020617"),
    )
}

fn foreground_settings(
    theme: &ActiveThemeProjection,
    role: BerylThemeRole,
) -> AppearanceForegroundSettings {
    AppearanceForegroundSettings::new(color(
        theme,
        role,
        BerylThemeProperty::Foreground,
        "#e2e8f0",
    ))
}

fn button_settings(
    theme: &ActiveThemeProjection,
    normal: BerylThemeRole,
    hover: BerylThemeRole,
    active: BerylThemeRole,
    disabled: BerylThemeRole,
) -> AppearanceButtonSettings {
    AppearanceButtonSettings {
        font_weight: font_weight(theme, normal, 500),
        normal: button_state_settings(theme, normal),
        hover: button_state_settings(theme, hover),
        active: button_state_settings(theme, active),
        disabled: button_state_settings(theme, disabled),
    }
}

fn button_state_settings(
    theme: &ActiveThemeProjection,
    role: BerylThemeRole,
) -> AppearanceButtonStateSettings {
    AppearanceButtonStateSettings::new(
        color(theme, role, BerylThemeProperty::Background, "#1e293b"),
        color(theme, role, BerylThemeProperty::Border, "#475569"),
        color(theme, role, BerylThemeProperty::Foreground, "#e2e8f0"),
    )
}

fn color(
    theme: &ActiveThemeProjection,
    role: BerylThemeRole,
    property: BerylThemeProperty,
    fallback: &'static str,
) -> String {
    match theme.resolve_property(role.id(), property.id(), &ThemeResolutionContext::new()) {
        Ok(StylePropertyValue::Color(value)) => value,
        _ => fallback.to_string(),
    }
}

fn font_family(
    theme: &ActiveThemeProjection,
    role: BerylThemeRole,
    fallback: &'static str,
) -> String {
    match theme.resolve_property(
        role.id(),
        BerylThemeProperty::FontFamily.id(),
        &ThemeResolutionContext::new(),
    ) {
        Ok(StylePropertyValue::FontFamily(value)) => value,
        _ => fallback.to_string(),
    }
}

fn font_size(theme: &ActiveThemeProjection, role: BerylThemeRole, fallback: f32) -> f32 {
    match theme.resolve_property(
        role.id(),
        BerylThemeProperty::FontSize.id(),
        &ThemeResolutionContext::new(),
    ) {
        Ok(StylePropertyValue::LogicalPixels(value)) => value,
        _ => fallback,
    }
}

fn font_weight(theme: &ActiveThemeProjection, role: BerylThemeRole, fallback: u16) -> u16 {
    match theme.resolve_property(
        role.id(),
        BerylThemeProperty::FontWeight.id(),
        &ThemeResolutionContext::new(),
    ) {
        Ok(StylePropertyValue::FontWeight(value)) => value,
        _ => fallback,
    }
}
