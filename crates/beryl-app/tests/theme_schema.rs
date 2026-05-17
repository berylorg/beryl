use std::collections::BTreeSet;

use beryl_app::{
    ActiveThemeProjection, BerylThemeProperty, BerylThemeRole, StylePropertySource,
    ThemeDefinition, ThemeResolutionContext, ThemeResolver, ThemeRoleDefinition,
    built_in_theme_definition, built_in_theme_resolver, built_in_theme_schema,
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
fn every_schema_role_declares_every_required_property() {
    let schema = built_in_theme_schema();
    let required: BTreeSet<_> = BerylThemeProperty::ALL
        .iter()
        .map(|property| property.id())
        .collect();

    for role in schema.roles() {
        let actual: BTreeSet<_> = role
            .properties()
            .keys()
            .map(|property| property.as_str())
            .collect();
        assert_eq!(actual, required, "missing property on {}", role.role_id());
    }
}

#[test]
fn every_built_in_role_and_required_property_resolves() {
    let resolver = built_in_theme_resolver();
    let context = ThemeResolutionContext::new();

    for role in BerylThemeRole::ALL {
        let style = resolver.resolve_style(role.id(), &context).unwrap();
        assert_eq!(style.properties().len(), BerylThemeProperty::ALL.len());

        for property in BerylThemeProperty::ALL {
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
        assert_eq!(style.properties().len(), BerylThemeProperty::ALL.len());
    }
}

#[test]
fn inline_code_uses_runtime_ambient_background() {
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
        final_code.property(&BerylThemeProperty::Background.id().into()),
        final_answer.property(&BerylThemeProperty::Background.id().into())
    );
    assert_eq!(
        user_code.property(&BerylThemeProperty::Background.id().into()),
        user_input.property(&BerylThemeProperty::Background.id().into())
    );
    assert_eq!(
        settings_code.property(&BerylThemeProperty::Background.id().into()),
        settings_row.property(&BerylThemeProperty::Background.id().into())
    );
    assert_eq!(
        popup_code.property(&BerylThemeProperty::Background.id().into()),
        popup.property(&BerylThemeProperty::Background.id().into())
    );
    assert_ne!(
        final_code.property(&BerylThemeProperty::Background.id().into()),
        user_code.property(&BerylThemeProperty::Background.id().into())
    );
    assert_eq!(
        final_code.property(&BerylThemeProperty::Foreground.id().into()),
        user_code.property(&BerylThemeProperty::Foreground.id().into())
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
fn built_in_theme_definition_uses_ambient_source_for_inline_code_backgrounds() {
    let definition = built_in_theme_definition();
    let inline_code = definition
        .roles()
        .iter()
        .find(|role| role.role_id().as_str() == BerylThemeRole::MarkdownInlineCode.id())
        .unwrap();

    assert!(matches!(
        inline_code
            .properties()
            .get(&BerylThemeProperty::Background.id().into()),
        Some(beryl_app::StylePropertySource::AmbientParent)
    ));
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
    let background_id = BerylThemeProperty::Background.id().into();
    let inline_background = ambient_projection
        .default_style(BerylThemeRole::MarkdownInlineCode.id())
        .unwrap()
        .property(&background_id)
        .unwrap()
        .clone();
    let concrete_definition = built_in_definition_with_inline_code_background(
        StylePropertySource::Concrete(inline_background.clone()),
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
        ambient_inline.property(&background_id),
        user_input.property(&background_id)
    );
    assert_eq!(
        concrete_inline.property(&background_id),
        Some(&inline_background)
    );
    assert_ne!(
        ambient_inline.property(&background_id),
        concrete_inline.property(&background_id)
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

fn built_in_definition_with_inline_code_background(source: StylePropertySource) -> ThemeDefinition {
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
                        && property_id.as_str() == BerylThemeProperty::Background.id()
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
