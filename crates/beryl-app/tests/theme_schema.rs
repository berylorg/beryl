use std::collections::BTreeSet;

use beryl_app::{
    ActiveThemeProjection, BerylThemeProperty, BerylThemeRole, StylePropertyKind,
    StylePropertySource, StylePropertyValue, ThemeDefinition, ThemeResolutionContext,
    ThemeResolver, ThemeRoleDefinition, built_in_theme_definition, built_in_theme_resolver,
    built_in_theme_schema, built_in_theme_supported_properties,
};

#[test]
fn built_in_schema_constructs_valid_resolver() {
    let _resolver = built_in_theme_resolver();

    built_in_theme_resolver()
        .resolve_style(
            BerylThemeRole::AppWindow.id(),
            &ThemeResolutionContext::new(),
        )
        .unwrap();
}

#[test]
fn built_in_schema_matches_role_inventory() {
    let schema = built_in_theme_schema();
    let schema_ids: Vec<_> = schema
        .roles()
        .iter()
        .map(|role| role.role_id().as_str())
        .collect();
    let inventory_ids: Vec<_> = BerylThemeRole::ALL.iter().map(|role| role.id()).collect();
    let unique_schema_ids: BTreeSet<_> = schema_ids.iter().copied().collect();

    assert_eq!(schema_ids, inventory_ids);
    assert_eq!(unique_schema_ids.len(), BerylThemeRole::ALL.len());
}

#[test]
fn every_schema_role_matches_capability_source() {
    let schema = built_in_theme_schema();

    for role in BerylThemeRole::ALL {
        let schema_role = schema
            .roles()
            .iter()
            .find(|schema_role| schema_role.role_id().as_str() == role.id())
            .expect("schema role should exist");
        let capability_properties: BTreeSet<_> = built_in_theme_supported_properties(*role)
            .iter()
            .map(|property| property.id())
            .collect();
        let actual: BTreeSet<_> = schema_role
            .properties()
            .keys()
            .map(|property| property.as_str())
            .collect();
        assert_eq!(
            capability_properties,
            expected_supported_property_ids(*role),
            "schema capabilities must match expected inventory for {}",
            role.id()
        );
        assert_eq!(
            actual,
            capability_properties,
            "schema properties must come from capability source for {}",
            role.id()
        );
    }
}

#[test]
fn single_primitive_roles_expose_only_single_color_property() {
    let schema = built_in_theme_schema();

    for role in SINGLE_PRIMITIVE_COLOR_THEME_ROLES {
        let schema_role = schema
            .roles()
            .iter()
            .find(|schema_role| schema_role.role_id().as_str() == role.id())
            .expect("single primitive role should exist");
        let properties: Vec<_> = schema_role
            .properties()
            .iter()
            .map(|(property_id, schema)| (property_id.as_str(), schema.kind()))
            .collect();

        assert_eq!(properties, vec![("color", StylePropertyKind::Color)]);
        assert_eq!(
            built_in_theme_supported_properties(*role),
            &[BerylThemeProperty::Color]
        );
    }
}

#[test]
fn roles_without_single_primitive_color_do_not_expose_color_property() {
    let schema = built_in_theme_schema();

    for role in BerylThemeRole::ALL
        .iter()
        .copied()
        .filter(|role| !SINGLE_PRIMITIVE_COLOR_THEME_ROLES.contains(role))
    {
        let schema_role = schema
            .roles()
            .iter()
            .find(|schema_role| schema_role.role_id().as_str() == role.id())
            .expect("schema role should exist");

        assert!(
            !schema_role
                .properties()
                .contains_key(&BerylThemeProperty::Color.id().into()),
            "{} must not expose single primitive color",
            role.id()
        );
    }
}

#[test]
fn representative_roles_expose_only_render_consumed_properties() {
    for (role, expected) in [
        (
            BerylThemeRole::AppWindow,
            property_set(&[
                BerylThemeProperty::Background,
                BerylThemeProperty::Foreground,
                BerylThemeProperty::FontWeight,
            ]),
        ),
        (
            BerylThemeRole::MarkdownParagraph,
            property_set(&[
                BerylThemeProperty::Foreground,
                BerylThemeProperty::TextBackground,
                BerylThemeProperty::FontFamily,
                BerylThemeProperty::FontSize,
                BerylThemeProperty::FontWeight,
            ]),
        ),
        (
            BerylThemeRole::SyntaxString,
            property_set(&[BerylThemeProperty::Foreground]),
        ),
        (
            BerylThemeRole::MediaBorder,
            property_set(&[BerylThemeProperty::Border]),
        ),
        (
            BerylThemeRole::ScrollbarThumbNormal,
            property_set(&[BerylThemeProperty::Color]),
        ),
        (
            BerylThemeRole::StatusValueWorking,
            property_set(&[BerylThemeProperty::Border, BerylThemeProperty::Foreground]),
        ),
        (BerylThemeRole::PopupRowNormal, BTreeSet::new()),
        (
            BerylThemeRole::SettingsInputFocused,
            property_set(&[BerylThemeProperty::Border, BerylThemeProperty::Foreground]),
        ),
        (BerylThemeRole::SurfaceRowSelected, BTreeSet::new()),
    ] {
        assert_eq!(
            supported_property_ids(role),
            expected,
            "{} should expose only its render-consumed properties",
            role.id()
        );
    }
}

#[test]
fn built_in_definition_matches_schema_capability_properties() {
    let schema = built_in_theme_schema();
    let definition = built_in_theme_definition();

    for role in BerylThemeRole::ALL {
        let schema_role = schema
            .roles()
            .iter()
            .find(|schema_role| schema_role.role_id().as_str() == role.id())
            .expect("schema role should exist");
        let definition_role = definition
            .roles()
            .iter()
            .find(|definition_role| definition_role.role_id().as_str() == role.id())
            .expect("built-in definition role should exist");
        let schema_properties: BTreeSet<_> = schema_role.properties().keys().collect();
        let definition_properties: BTreeSet<_> = definition_role.properties().keys().collect();

        assert_eq!(
            definition_properties,
            schema_properties,
            "built-in definition properties must match schema capabilities for {}",
            role.id()
        );
    }
}

#[test]
fn every_built_in_role_and_required_property_resolves() {
    let resolver = built_in_theme_resolver();
    let context = ThemeResolutionContext::new();

    for role in BerylThemeRole::ALL {
        let style = resolver.resolve_style(role.id(), &context).unwrap();
        let supported_properties = built_in_theme_supported_properties(*role);
        assert_eq!(
            supported_property_ids(*role),
            expected_supported_property_ids(*role)
        );
        assert_eq!(style.properties().len(), supported_properties.len());

        for property in supported_properties {
            let value = resolver
                .resolve_property(role.id(), property.id(), &context)
                .unwrap();
            assert_eq!(style.property(&property.id().into()), Some(&value));
        }
    }
}

#[test]
fn built_in_active_projection_resolves_all_inventory_roles() {
    let projection = ActiveThemeProjection::built_in();

    assert_eq!(projection.default_styles().len(), BerylThemeRole::ALL.len());

    for role in BerylThemeRole::ALL {
        let style = projection.default_style(role.id()).unwrap();
        let supported_properties = built_in_theme_supported_properties(*role);
        assert_eq!(
            supported_property_ids(*role),
            expected_supported_property_ids(*role)
        );
        assert_eq!(style.properties().len(), supported_properties.len());
    }
}

#[test]
fn separator_color_resolves_and_legacy_visual_properties_are_unsupported() {
    let resolver = built_in_theme_resolver();
    let context = ThemeResolutionContext::new();

    assert_eq!(
        resolver
            .resolve_property(
                BerylThemeRole::MainSeparator.id(),
                BerylThemeProperty::Color.id(),
                &context,
            )
            .unwrap(),
        StylePropertyValue::color("#334155")
    );

    for property in [
        BerylThemeProperty::Background,
        BerylThemeProperty::Border,
        BerylThemeProperty::Foreground,
        BerylThemeProperty::TextBackground,
        BerylThemeProperty::FontFamily,
        BerylThemeProperty::FontSize,
        BerylThemeProperty::FontWeight,
    ] {
        assert!(
            resolver
                .resolve_property(BerylThemeRole::MainSeparator.id(), property.id(), &context,)
                .is_err(),
            "main.separator must not support {}",
            property.id()
        );
    }
}

#[test]
fn separator_color_controls_runtime_projection() {
    let projection = ActiveThemeProjection::from_built_in_resolver(
        ThemeResolver::new(
            built_in_theme_schema(),
            ThemeDefinition::new(vec![
                ThemeRoleDefinition::new(BerylThemeRole::MainSeparator.id()).with_property(
                    BerylThemeProperty::Color.id(),
                    StylePropertySource::Concrete(StylePropertyValue::color("#abcdef")),
                ),
            ]),
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(
        projection
            .resolve_property(
                BerylThemeRole::MainSeparator.id(),
                BerylThemeProperty::Color.id(),
                &ThemeResolutionContext::new(),
            )
            .unwrap(),
        StylePropertyValue::color("#abcdef")
    );
    assert_eq!(
        beryl_app::AppearanceSettings::from_active_theme(&projection)
            .chrome
            .separator,
        "#abcdef"
    );
}

#[test]
fn inline_code_uses_runtime_ambient_text_background() {
    let projection = ActiveThemeProjection::built_in();
    let final_answer = projection
        .default_style(BerylThemeRole::TranscriptAssistantFinal.id())
        .unwrap()
        .clone();
    let user_input = projection
        .default_style(BerylThemeRole::TranscriptUserInput.id())
        .unwrap()
        .clone();
    let settings_row = projection
        .default_style(BerylThemeRole::SettingsRowNormal.id())
        .unwrap()
        .clone();
    let popup = projection
        .default_style(BerylThemeRole::PopupSurface.id())
        .unwrap()
        .clone();

    let final_code = projection
        .resolve_style(
            BerylThemeRole::MarkdownInlineCode.id(),
            &ThemeResolutionContext::new().with_ambient_parent(final_answer.clone()),
        )
        .unwrap();
    let user_code = projection
        .resolve_style(
            BerylThemeRole::MarkdownInlineCode.id(),
            &ThemeResolutionContext::new().with_ambient_parent(user_input.clone()),
        )
        .unwrap();
    let settings_code = projection
        .resolve_style(
            BerylThemeRole::MarkdownInlineCode.id(),
            &ThemeResolutionContext::new().with_ambient_parent(settings_row.clone()),
        )
        .unwrap();
    let popup_code = projection
        .resolve_style(
            BerylThemeRole::MarkdownInlineCode.id(),
            &ThemeResolutionContext::new().with_ambient_parent(popup.clone()),
        )
        .unwrap();

    assert_eq!(
        final_code.property(&BerylThemeProperty::TextBackground.id().into()),
        final_answer.property(&BerylThemeProperty::TextBackground.id().into())
    );
    assert_eq!(
        user_code.property(&BerylThemeProperty::TextBackground.id().into()),
        user_input.property(&BerylThemeProperty::TextBackground.id().into())
    );
    assert_eq!(
        settings_code.property(&BerylThemeProperty::TextBackground.id().into()),
        projection
            .default_style(BerylThemeRole::MarkdownInlineCode.id())
            .unwrap()
            .property(&BerylThemeProperty::TextBackground.id().into())
    );
    assert_eq!(
        popup_code.property(&BerylThemeProperty::TextBackground.id().into()),
        projection
            .default_style(BerylThemeRole::MarkdownInlineCode.id())
            .unwrap()
            .property(&BerylThemeProperty::TextBackground.id().into())
    );
    assert_eq!(
        final_code.property(&BerylThemeProperty::Foreground.id().into()),
        user_code.property(&BerylThemeProperty::Foreground.id().into())
    );
    assert!(
        final_code
            .property(&BerylThemeProperty::Background.id().into())
            .is_none()
    );
}

#[test]
fn syntax_token_theme_roles_are_in_schema_inventory() {
    let resolver = built_in_theme_resolver();
    let context = ThemeResolutionContext::new();

    for role in SYNTAX_TOKEN_THEME_ROLES {
        resolver.resolve_style(role.id(), &context).unwrap();
    }
}

#[test]
fn supported_interaction_state_roles_are_in_schema_inventory() {
    let resolver = built_in_theme_resolver();
    let context = ThemeResolutionContext::new();

    for role in INTERACTION_STATE_THEME_ROLES {
        resolver.resolve_style(role.id(), &context).unwrap();
    }
}

#[test]
fn built_in_theme_definition_uses_ambient_source_for_inline_code_text_backgrounds() {
    let definition = built_in_theme_definition();
    let inline_code = definition
        .roles()
        .iter()
        .find(|role| role.role_id().as_str() == BerylThemeRole::MarkdownInlineCode.id())
        .unwrap();

    assert!(
        !inline_code
            .properties()
            .contains_key(&BerylThemeProperty::Background.id().into())
    );
    assert!(matches!(
        inline_code
            .properties()
            .get(&BerylThemeProperty::TextBackground.id().into()),
        Some(beryl_app::StylePropertySource::AmbientParent)
    ));
}

#[test]
fn active_projection_revision_changes_when_source_semantics_change() {
    let ambient_projection = ActiveThemeProjection::from_built_in_resolver(
        ThemeResolver::new(built_in_theme_schema(), built_in_theme_definition()).unwrap(),
    )
    .unwrap();
    let text_background_id = BerylThemeProperty::TextBackground.id().into();
    let inline_text_background = ambient_projection
        .default_style(BerylThemeRole::MarkdownInlineCode.id())
        .unwrap()
        .property(&text_background_id)
        .unwrap()
        .clone();
    let concrete_definition = built_in_definition_with_inline_code_text_background(
        StylePropertySource::Concrete(inline_text_background.clone()),
    );
    let concrete_projection = ActiveThemeProjection::from_built_in_resolver(
        ThemeResolver::new(built_in_theme_schema(), concrete_definition).unwrap(),
    )
    .unwrap();

    assert_eq!(
        ambient_projection.default_styles(),
        concrete_projection.default_styles()
    );
    assert_ne!(
        ambient_projection.style_revision(),
        concrete_projection.style_revision()
    );

    let user_input = ambient_projection
        .default_style(BerylThemeRole::TranscriptUserInput.id())
        .unwrap()
        .clone();
    let ambient_inline = ambient_projection
        .resolve_style(
            BerylThemeRole::MarkdownInlineCode.id(),
            &ThemeResolutionContext::new().with_ambient_parent(user_input.clone()),
        )
        .unwrap();
    let concrete_inline = concrete_projection
        .resolve_style(
            BerylThemeRole::MarkdownInlineCode.id(),
            &ThemeResolutionContext::new().with_ambient_parent(user_input.clone()),
        )
        .unwrap();

    assert_eq!(
        ambient_inline.property(&text_background_id),
        user_input.property(&text_background_id)
    );
    assert_eq!(
        concrete_inline.property(&text_background_id),
        Some(&inline_text_background)
    );
    assert_ne!(
        ambient_inline.property(&text_background_id),
        concrete_inline.property(&text_background_id)
    );
}

const SYNTAX_TOKEN_THEME_ROLES: &[BerylThemeRole] = &[
    BerylThemeRole::SyntaxMarkupHeadingMarker,
    BerylThemeRole::SyntaxMarkupQuoteMarker,
    BerylThemeRole::SyntaxMarkupListMarker,
    BerylThemeRole::SyntaxMarkupThematicBreak,
    BerylThemeRole::SyntaxMarkupFenceDelimiter,
    BerylThemeRole::SyntaxMarkupFenceInfo,
    BerylThemeRole::SyntaxMarkupCodeBlock,
    BerylThemeRole::SyntaxMarkupCodeSpanDelimiter,
    BerylThemeRole::SyntaxMarkupCodeSpan,
    BerylThemeRole::SyntaxMarkupEmphasisDelimiter,
    BerylThemeRole::SyntaxMarkupStrongDelimiter,
    BerylThemeRole::SyntaxMarkupLinkText,
    BerylThemeRole::SyntaxMarkupLinkDestination,
    BerylThemeRole::SyntaxMarkupImageMarker,
    BerylThemeRole::SyntaxMarkupPunctuation,
    BerylThemeRole::SyntaxMarkupHtml,
    BerylThemeRole::SyntaxEscape,
    BerylThemeRole::SyntaxStructuralPunctuation,
    BerylThemeRole::SyntaxKey,
    BerylThemeRole::SyntaxString,
    BerylThemeRole::SyntaxNumber,
    BerylThemeRole::SyntaxBoolean,
    BerylThemeRole::SyntaxNull,
    BerylThemeRole::SyntaxDateTime,
    BerylThemeRole::SyntaxComment,
    BerylThemeRole::SyntaxSectionHeader,
    BerylThemeRole::SyntaxAssignment,
    BerylThemeRole::SyntaxTokenEscape,
    BerylThemeRole::SyntaxError,
];

const INTERACTION_STATE_THEME_ROLES: &[BerylThemeRole] = &[
    BerylThemeRole::SurfaceRowHover,
    BerylThemeRole::ButtonPrimaryPressed,
    BerylThemeRole::ButtonPrimaryActive,
    BerylThemeRole::ButtonPrimaryDisabled,
    BerylThemeRole::SurfaceRowSelected,
    BerylThemeRole::InputFieldFocused,
    BerylThemeRole::SurfaceRowWarning,
    BerylThemeRole::SurfaceRowError,
    BerylThemeRole::SurfaceRowInfo,
    BerylThemeRole::SurfaceRowPending,
    BerylThemeRole::StatusValueStreaming,
    BerylThemeRole::SurfaceRowUnavailable,
    BerylThemeRole::NoticeSuccess,
    BerylThemeRole::GraphRowInvalid,
    BerylThemeRole::ThreadSelectorRowUnavailable,
    BerylThemeRole::FocusRing,
];

fn built_in_definition_with_inline_code_text_background(
    source: StylePropertySource,
) -> ThemeDefinition {
    ThemeDefinition::new(
        built_in_theme_definition()
            .roles()
            .iter()
            .map(|role| {
                let mut role_definition = ThemeRoleDefinition::new(role.role_id().clone());
                if let Some(static_parent) = role.static_parent() {
                    role_definition = role_definition.with_static_parent(static_parent.clone());
                }

                for (property_id, property_source) in role.properties() {
                    let next_source = if role.role_id().as_str()
                        == BerylThemeRole::MarkdownInlineCode.id()
                        && property_id.as_str() == BerylThemeProperty::TextBackground.id()
                    {
                        source.clone()
                    } else {
                        property_source.clone()
                    };
                    role_definition =
                        role_definition.with_property(property_id.clone(), next_source);
                }

                role_definition
            })
            .collect(),
    )
}

fn expected_supported_property_ids(role: BerylThemeRole) -> BTreeSet<&'static str> {
    match role {
        BerylThemeRole::AppWindow => property_set(&[
            BerylThemeProperty::Background,
            BerylThemeProperty::Foreground,
            BerylThemeProperty::FontWeight,
        ]),
        BerylThemeRole::MainToolbar => property_set(&[
            BerylThemeProperty::Background,
            BerylThemeProperty::FontWeight,
        ]),
        BerylThemeRole::MainThreadStrip | BerylThemeRole::InputPanel => {
            property_set(&[BerylThemeProperty::Background])
        }
        BerylThemeRole::MainSeparator | BerylThemeRole::StructuralSeparator => {
            property_set(&[BerylThemeProperty::Color])
        }
        BerylThemeRole::Panel
        | BerylThemeRole::SurfaceRowDisabled
        | BerylThemeRole::ButtonPrimaryHover
        | BerylThemeRole::ButtonPrimaryActive
        | BerylThemeRole::ButtonPrimaryDisabled
        | BerylThemeRole::ButtonSecondaryHover
        | BerylThemeRole::ButtonSecondaryActive
        | BerylThemeRole::ButtonSecondaryDisabled
        | BerylThemeRole::CodePanelButtonNormal
        | BerylThemeRole::CodePanelButtonHover
        | BerylThemeRole::CodePanelButtonActive
        | BerylThemeRole::InputField
        | BerylThemeRole::SettingsGroup
        | BerylThemeRole::SettingsRowNormal
        | BerylThemeRole::SettingsPopup
        | BerylThemeRole::SettingsInputNormal
        | BerylThemeRole::TranscriptQuotePopup
        | BerylThemeRole::TranscriptPending
        | BerylThemeRole::TranscriptUnavailable
        | BerylThemeRole::ChecklistSidebar
        | BerylThemeRole::ThreadSelectorRowSelected
        | BerylThemeRole::ThreadSelectorRowUnavailable
        | BerylThemeRole::NoticeWarning
        | BerylThemeRole::StatusValueOk
        | BerylThemeRole::StatusValueError => property_set(&[
            BerylThemeProperty::Background,
            BerylThemeProperty::Border,
            BerylThemeProperty::Foreground,
        ]),
        BerylThemeRole::SurfaceRow | BerylThemeRole::SurfaceRowHover => {
            property_set(&[BerylThemeProperty::Background])
        }
        BerylThemeRole::SurfaceRowInfo
        | BerylThemeRole::ChecklistHeader
        | BerylThemeRole::ChecklistStatusTodo
        | BerylThemeRole::ChecklistStatusInProgress
        | BerylThemeRole::ChecklistStatusDone => property_set(&[
            BerylThemeProperty::Foreground,
            BerylThemeProperty::FontWeight,
        ]),
        BerylThemeRole::SurfaceRowSelected
        | BerylThemeRole::SurfaceRowPending
        | BerylThemeRole::SurfaceRowUnavailable
        | BerylThemeRole::SurfaceRowError
        | BerylThemeRole::SurfaceRowWarning
        | BerylThemeRole::SurfaceRowSuccess
        | BerylThemeRole::ButtonPrimaryPressed
        | BerylThemeRole::ButtonSecondaryPressed
        | BerylThemeRole::CodePanelButtonDisabled
        | BerylThemeRole::InputFieldFocused
        | BerylThemeRole::InputSelection
        | BerylThemeRole::InputCaret
        | BerylThemeRole::InputError
        | BerylThemeRole::SettingsSidebar
        | BerylThemeRole::SettingsSidebarRowNormal
        | BerylThemeRole::SettingsSidebarRowHover
        | BerylThemeRole::SettingsSidebarRowSelected
        | BerylThemeRole::SettingsPage
        | BerylThemeRole::SettingsRowHover
        | BerylThemeRole::SettingsRowModified
        | BerylThemeRole::TranscriptContextMenu
        | BerylThemeRole::CodePanelSelection
        | BerylThemeRole::ComposerImageMarker
        | BerylThemeRole::ThreadSelectorRow
        | BerylThemeRole::ColumnSelectorColumn
        | BerylThemeRole::ColumnSelectorHeader
        | BerylThemeRole::ColumnSelectorRow
        | BerylThemeRole::ColumnSelectorRowSelected
        | BerylThemeRole::ColumnSelectorAccent
        | BerylThemeRole::PopupRowNormal
        | BerylThemeRole::PopupRowDisabled
        | BerylThemeRole::OverlayBackdrop
        | BerylThemeRole::DiagnosticSurface
        | BerylThemeRole::DiagnosticRow
        | BerylThemeRole::DiagnosticError
        | BerylThemeRole::DiagnosticWarning
        | BerylThemeRole::ScrollbarThumbHover
        | BerylThemeRole::ScrollbarThumbDragging
        | BerylThemeRole::FocusRing => BTreeSet::new(),
        BerylThemeRole::ButtonPrimaryNormal
        | BerylThemeRole::ButtonSecondaryNormal
        | BerylThemeRole::SettingsButtonPrimary
        | BerylThemeRole::SettingsButtonSecondary => property_set(&[
            BerylThemeProperty::Background,
            BerylThemeProperty::Border,
            BerylThemeProperty::Foreground,
            BerylThemeProperty::FontWeight,
        ]),
        BerylThemeRole::SettingsWindow
        | BerylThemeRole::SettingsInputSelection
        | BerylThemeRole::TranscriptSelection
        | BerylThemeRole::CodePanelContainer
        | BerylThemeRole::GraphRowHover
        | BerylThemeRole::PopupRowHover
        | BerylThemeRole::PopupRowSelected => property_set(&[BerylThemeProperty::Background]),
        BerylThemeRole::SettingsRowDisabled
        | BerylThemeRole::GraphRowPending
        | BerylThemeRole::GraphRowDisabled
        | BerylThemeRole::MediaCaption
        | BerylThemeRole::NoticeSuccess
        | BerylThemeRole::StatusValueCompacting
        | BerylThemeRole::StatusValuePending
        | BerylThemeRole::StatusValueUnavailable
        | BerylThemeRole::StatusValueStreaming
        | BerylThemeRole::SyntaxMarkupHeadingMarker
        | BerylThemeRole::SyntaxMarkupQuoteMarker
        | BerylThemeRole::SyntaxMarkupListMarker
        | BerylThemeRole::SyntaxMarkupThematicBreak
        | BerylThemeRole::SyntaxMarkupFenceDelimiter
        | BerylThemeRole::SyntaxMarkupFenceInfo
        | BerylThemeRole::SyntaxMarkupCodeBlock
        | BerylThemeRole::SyntaxMarkupCodeSpanDelimiter
        | BerylThemeRole::SyntaxMarkupCodeSpan
        | BerylThemeRole::SyntaxMarkupEmphasisDelimiter
        | BerylThemeRole::SyntaxMarkupStrongDelimiter
        | BerylThemeRole::SyntaxMarkupLinkText
        | BerylThemeRole::SyntaxMarkupLinkDestination
        | BerylThemeRole::SyntaxMarkupImageMarker
        | BerylThemeRole::SyntaxMarkupPunctuation
        | BerylThemeRole::SyntaxMarkupHtml
        | BerylThemeRole::SyntaxEscape
        | BerylThemeRole::SyntaxStructuralPunctuation
        | BerylThemeRole::SyntaxKey
        | BerylThemeRole::SyntaxString
        | BerylThemeRole::SyntaxNumber
        | BerylThemeRole::SyntaxBoolean
        | BerylThemeRole::SyntaxNull
        | BerylThemeRole::SyntaxDateTime
        | BerylThemeRole::SyntaxComment
        | BerylThemeRole::SyntaxSectionHeader
        | BerylThemeRole::SyntaxAssignment
        | BerylThemeRole::SyntaxTokenEscape
        | BerylThemeRole::SyntaxError => property_set(&[BerylThemeProperty::Foreground]),
        BerylThemeRole::SettingsInputFocused | BerylThemeRole::StatusValueWorking => {
            property_set(&[BerylThemeProperty::Border, BerylThemeProperty::Foreground])
        }
        BerylThemeRole::TranscriptShell
        | BerylThemeRole::MediaPlaceholder
        | BerylThemeRole::MediaPlaceholderLoading
        | BerylThemeRole::MediaPlaceholderUnavailable
        | BerylThemeRole::GraphRowError
        | BerylThemeRole::StatusLine => property_set(&[
            BerylThemeProperty::Background,
            BerylThemeProperty::Foreground,
        ]),
        BerylThemeRole::TranscriptAssistantFinal
        | BerylThemeRole::TranscriptAssistantCommentary
        | BerylThemeRole::TranscriptAssistantReasoning
        | BerylThemeRole::MarkdownParagraph
        | BerylThemeRole::MarkdownHeading
        | BerylThemeRole::MarkdownEmphasis
        | BerylThemeRole::MarkdownStrongEmphasis
        | BerylThemeRole::MarkdownInlineCode
        | BerylThemeRole::MarkdownLink
        | BerylThemeRole::MarkdownUnsupportedFallback => property_set(&[
            BerylThemeProperty::Foreground,
            BerylThemeProperty::TextBackground,
            BerylThemeProperty::FontFamily,
            BerylThemeProperty::FontSize,
            BerylThemeProperty::FontWeight,
        ]),
        BerylThemeRole::TranscriptUserInput => property_set(&[
            BerylThemeProperty::Background,
            BerylThemeProperty::Border,
            BerylThemeProperty::Foreground,
            BerylThemeProperty::TextBackground,
            BerylThemeProperty::FontFamily,
            BerylThemeProperty::FontSize,
            BerylThemeProperty::FontWeight,
        ]),
        BerylThemeRole::TranscriptActivityCaret
        | BerylThemeRole::MarkdownThematicBreak
        | BerylThemeRole::CodePanelResizeHandle
        | BerylThemeRole::ScrollbarThumbNormal => property_set(&[BerylThemeProperty::Color]),
        BerylThemeRole::MarkdownBlockQuote
        | BerylThemeRole::CodePanelBorder
        | BerylThemeRole::MediaBorder
        | BerylThemeRole::SettingsInputError => property_set(&[BerylThemeProperty::Border]),
        BerylThemeRole::MarkdownListMarker | BerylThemeRole::CodePanelHeader => property_set(&[
            BerylThemeProperty::Foreground,
            BerylThemeProperty::FontFamily,
            BerylThemeProperty::FontSize,
            BerylThemeProperty::FontWeight,
        ]),
        BerylThemeRole::CodePanelBody => property_set(&[
            BerylThemeProperty::Background,
            BerylThemeProperty::Foreground,
            BerylThemeProperty::FontFamily,
            BerylThemeProperty::FontSize,
            BerylThemeProperty::FontWeight,
        ]),
        BerylThemeRole::TranscriptImageMarker => property_set(&[
            BerylThemeProperty::Foreground,
            BerylThemeProperty::TextBackground,
        ]),
        BerylThemeRole::GraphOverlay | BerylThemeRole::GraphColumn => {
            property_set(&[BerylThemeProperty::Background, BerylThemeProperty::Border])
        }
        BerylThemeRole::GraphColumnHeader
        | BerylThemeRole::GraphRowTopic
        | BerylThemeRole::GraphRowChecklist
        | BerylThemeRole::GraphRowChecklistItem
        | BerylThemeRole::GraphRowThreadRef
        | BerylThemeRole::GraphRowSoftLink
        | BerylThemeRole::GraphRowSelected
        | BerylThemeRole::GraphRowInvalid
        | BerylThemeRole::ChecklistRow
        | BerylThemeRole::PopupSurface
        | BerylThemeRole::NoticeInfo
        | BerylThemeRole::NoticeError => property_set(&[
            BerylThemeProperty::Background,
            BerylThemeProperty::Border,
            BerylThemeProperty::Foreground,
            BerylThemeProperty::FontWeight,
        ]),
        BerylThemeRole::ThreadSelectorSurface | BerylThemeRole::WorkspacePickerSurface => {
            property_set(&[BerylThemeProperty::FontWeight])
        }
        BerylThemeRole::WorkspacePickerWorkspaceRow | BerylThemeRole::WorkspacePickerMemberRow => {
            property_set(&[
                BerylThemeProperty::FontFamily,
                BerylThemeProperty::FontWeight,
            ])
        }
        BerylThemeRole::WorkspacePickerRowActive => property_set(&[
            BerylThemeProperty::Border,
            BerylThemeProperty::Foreground,
            BerylThemeProperty::FontWeight,
        ]),
    }
}

fn supported_property_ids(role: BerylThemeRole) -> BTreeSet<&'static str> {
    built_in_theme_supported_properties(role)
        .iter()
        .map(|property| property.id())
        .collect()
}

fn property_set(properties: &[BerylThemeProperty]) -> BTreeSet<&'static str> {
    properties.iter().map(|property| property.id()).collect()
}

const SINGLE_PRIMITIVE_COLOR_THEME_ROLES: &[BerylThemeRole] = &[
    BerylThemeRole::MainSeparator,
    BerylThemeRole::StructuralSeparator,
    BerylThemeRole::TranscriptActivityCaret,
    BerylThemeRole::MarkdownThematicBreak,
    BerylThemeRole::CodePanelResizeHandle,
    BerylThemeRole::ScrollbarThumbNormal,
];
