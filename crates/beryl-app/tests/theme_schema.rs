use std::collections::BTreeSet;

use beryl_app::{
    ActiveThemeProjection, BerylThemeProperty, BerylThemeRole, StylePropertyKind,
    StylePropertySource, StylePropertyValue, ThemeDefinition, ThemeDocument, ThemeDocumentError,
    ThemeResolutionContext, ThemeResolver, ThemeRoleDefinition, built_in_theme_definition,
    built_in_theme_resolver, built_in_theme_schema, built_in_theme_supported_properties,
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

    assert_eq!(
        inventory_ids.first().copied(),
        Some(BerylThemeRole::Root.id())
    );
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
fn foundation_surface_roles_expose_only_surface_properties() {
    for role in FOUNDATION_SURFACE_THEME_ROLES {
        assert_eq!(
            supported_property_ids(*role),
            property_set(&[
                BerylThemeProperty::Background,
                BerylThemeProperty::Border,
                BerylThemeProperty::Foreground,
            ]),
            "{} should expose only surface properties",
            role.id()
        );
    }
}

#[test]
fn foundation_text_roles_expose_coherent_text_properties() {
    for role in FOUNDATION_TEXT_THEME_ROLES {
        assert_eq!(
            supported_property_ids(*role),
            property_set(&[
                BerylThemeProperty::Foreground,
                BerylThemeProperty::TextBackground,
                BerylThemeProperty::FontFamily,
                BerylThemeProperty::FontSize,
                BerylThemeProperty::FontWeight,
            ]),
            "{} should expose a complete text property bundle",
            role.id()
        );
    }
}

#[test]
fn common_control_surface_roles_expose_only_surface_properties() {
    for role in COMMON_CONTROL_SURFACE_THEME_ROLES {
        assert_eq!(
            supported_property_ids(*role),
            surface_property_set(),
            "{} should expose only surface properties",
            role.id()
        );
    }
}

#[test]
fn common_control_text_roles_expose_coherent_text_properties() {
    for role in COMMON_CONTROL_TEXT_THEME_ROLES {
        assert_eq!(
            supported_property_ids(*role),
            text_property_set(),
            "{} should expose the common text property bundle",
            role.id()
        );
    }
}

#[test]
fn common_control_primitive_parts_expose_only_color() {
    for role in COMMON_CONTROL_PRIMITIVE_THEME_ROLES {
        assert_eq!(
            supported_property_ids(*role),
            property_set(&[BerylThemeProperty::Color]),
            "{} should expose only single primitive color",
            role.id()
        );
    }
}

#[test]
fn every_editable_role_uses_a_coherent_property_shape() {
    let allowed_shapes = [
        property_set(&[
            BerylThemeProperty::Background,
            BerylThemeProperty::Border,
            BerylThemeProperty::Color,
            BerylThemeProperty::Foreground,
            BerylThemeProperty::TextBackground,
            BerylThemeProperty::FontFamily,
            BerylThemeProperty::FontSize,
            BerylThemeProperty::FontWeight,
        ]),
        text_property_set(),
        surface_property_set(),
        property_set(&[BerylThemeProperty::Background]),
        property_set(&[BerylThemeProperty::Border]),
        property_set(&[BerylThemeProperty::Color]),
        property_set(&[BerylThemeProperty::Foreground]),
        property_set(&[BerylThemeProperty::TextBackground]),
        property_set(&[
            BerylThemeProperty::Background,
            BerylThemeProperty::Foreground,
        ]),
        property_set(&[
            BerylThemeProperty::Foreground,
            BerylThemeProperty::TextBackground,
        ]),
    ];
    let font_family = BerylThemeProperty::FontFamily.id();
    let font_size = BerylThemeProperty::FontSize.id();
    let font_weight = BerylThemeProperty::FontWeight.id();

    for role in BerylThemeRole::ALL {
        let properties = supported_property_ids(*role);
        if properties.is_empty() {
            continue;
        }

        assert!(
            allowed_shapes.contains(&properties),
            "{} exposes an incoherent property shape: {:?}",
            role.id(),
            properties
        );
        if properties.contains(font_family)
            || properties.contains(font_size)
            || properties.contains(font_weight)
        {
            assert!(
                properties == text_property_set()
                    || properties
                        == property_set(&[
                            BerylThemeProperty::Background,
                            BerylThemeProperty::Border,
                            BerylThemeProperty::Color,
                            BerylThemeProperty::Foreground,
                            BerylThemeProperty::TextBackground,
                            BerylThemeProperty::FontFamily,
                            BerylThemeProperty::FontSize,
                            BerylThemeProperty::FontWeight,
                        ]),
                "{} exposes an isolated typography axis: {:?}",
                role.id(),
                properties
            );
        }
    }
}

#[test]
fn notice_variant_roles_are_surface_only() {
    for role in [
        BerylThemeRole::NoticeInfo,
        BerylThemeRole::NoticeWarning,
        BerylThemeRole::NoticeError,
        BerylThemeRole::NoticeSuccess,
    ] {
        assert_eq!(
            supported_property_ids(role),
            surface_property_set(),
            "{} should expose the same surface-only notice shape",
            role.id()
        );
    }
}

#[test]
fn canonical_foundation_role_graph_exists_is_acyclic_and_resolves() {
    let schema = built_in_theme_schema();
    let resolver = built_in_theme_resolver();
    let context = ThemeResolutionContext::new();

    for (role, expected_parent) in FOUNDATION_PARENT_EDGES {
        let schema_role = schema
            .roles()
            .iter()
            .find(|schema_role| schema_role.role_id().as_str() == role.id())
            .expect("foundation role should exist in schema");
        assert_eq!(
            schema_role.static_parent().map(|parent| parent.as_str()),
            expected_parent.map(BerylThemeRole::id),
            "{} should have the canonical static parent",
            role.id()
        );
    }

    for role in BerylThemeRole::ALL {
        let mut seen = BTreeSet::new();
        let mut cursor = Some(*role);
        while let Some(current) = cursor {
            assert!(
                seen.insert(current),
                "{} should not participate in a static-parent cycle",
                role.id()
            );
            cursor = current.static_parent();
        }
    }

    for role in FOUNDATION_THEME_ROLES {
        let style = resolver.resolve_style(role.id(), &context).unwrap();
        assert_eq!(
            style.properties().len(),
            built_in_theme_supported_properties(*role).len(),
            "{} should resolve every supported foundation property",
            role.id()
        );
        for property in built_in_theme_supported_properties(*role) {
            assert!(
                style.property(&property.id().into()).is_some(),
                "{}.{} should resolve through inheritance or fallback",
                role.id(),
                property.id()
            );
        }
    }
}

#[test]
fn common_control_role_graph_uses_foundation_and_control_parents() {
    let schema = built_in_theme_schema();

    for (role, expected_parent) in COMMON_CONTROL_PARENT_EDGES {
        let schema_role = schema
            .roles()
            .iter()
            .find(|schema_role| schema_role.role_id().as_str() == role.id())
            .expect("common control role should exist in schema");
        assert_eq!(
            schema_role.static_parent().map(|parent| parent.as_str()),
            expected_parent.map(BerylThemeRole::id),
            "{} should have the canonical common-control static parent",
            role.id()
        );
    }
}

#[test]
fn phase51_app_roles_use_canonical_surface_text_and_primitive_parents() {
    let schema = built_in_theme_schema();

    for (role, expected_parent) in PHASE51_APP_PARENT_EDGES {
        let schema_role = schema
            .roles()
            .iter()
            .find(|schema_role| schema_role.role_id().as_str() == role.id())
            .expect("phase 51 app role should exist in schema");
        assert_eq!(
            schema_role.static_parent().map(|parent| parent.as_str()),
            expected_parent.map(BerylThemeRole::id),
            "{} should keep its split-role static parent",
            role.id()
        );
    }
}

#[test]
fn phase52_transcript_markdown_composer_and_code_roles_use_split_parents() {
    let schema = built_in_theme_schema();

    for (role, expected_parent) in PHASE52_PARENT_EDGES {
        let schema_role = schema
            .roles()
            .iter()
            .find(|schema_role| schema_role.role_id().as_str() == role.id())
            .expect("phase 52 role should exist in schema");
        assert_eq!(
            schema_role.static_parent().map(|parent| parent.as_str()),
            expected_parent.map(BerylThemeRole::id),
            "{} should keep its transcript/Markdown/code static parent",
            role.id()
        );
    }

    for role in SYNTAX_TOKEN_THEME_ROLES {
        let schema_role = schema
            .roles()
            .iter()
            .find(|schema_role| schema_role.role_id().as_str() == role.id())
            .expect("syntax role should exist in schema");
        assert_eq!(
            schema_role.static_parent().map(|parent| parent.as_str()),
            Some(BerylThemeRole::CodePanelBodyText.id()),
            "{} should inherit text color defaults from code panel body text",
            role.id()
        );
    }
}

#[test]
fn phase53_graph_checklist_and_settings_roles_use_split_parents() {
    let schema = built_in_theme_schema();

    for (role, expected_parent) in PHASE53_PARENT_EDGES {
        let schema_role = schema
            .roles()
            .iter()
            .find(|schema_role| schema_role.role_id().as_str() == role.id())
            .expect("phase 53 role should exist in schema");
        assert_eq!(
            schema_role.static_parent().map(|parent| parent.as_str()),
            expected_parent.map(BerylThemeRole::id),
            "{} should keep its graph/checklist/settings static parent",
            role.id()
        );
    }
}

#[test]
fn root_foundation_changes_flow_through_static_inheritance() {
    let definition = built_in_definition_with_root_overrides();
    let resolver = ThemeResolver::new(built_in_theme_schema(), definition).unwrap();
    let context = ThemeResolutionContext::new();

    for (role, property, value) in [
        (
            BerylThemeRole::Text,
            BerylThemeProperty::Foreground,
            StylePropertyValue::color("#abcdef"),
        ),
        (
            BerylThemeRole::SurfaceWindow,
            BerylThemeProperty::Background,
            StylePropertyValue::color("#010203"),
        ),
        (
            BerylThemeRole::Primitive,
            BerylThemeProperty::Color,
            StylePropertyValue::color("#778899"),
        ),
        (
            BerylThemeRole::Control,
            BerylThemeProperty::Border,
            StylePropertyValue::color("#223344"),
        ),
    ] {
        assert_eq!(
            resolver
                .resolve_property(role.id(), property.id(), &context)
                .unwrap(),
            value,
            "{}.{} should inherit the root override",
            role.id(),
            property.id()
        );
    }
}

#[test]
fn strict_candidate_validation_rejects_invalid_foundation_role_properties() {
    for (role_id, property) in [
        (BerylThemeRole::SurfacePanel.id(), "font_size"),
        (BerylThemeRole::Text.id(), "color"),
        (BerylThemeRole::PrimitiveSeparator.id(), "background"),
    ] {
        let text = format!(
            r##"
schema = 1
name = "Invalid"

[[role]]
id = "{role_id}"
{property} = "fallback"
"##
        );
        let error = ThemeDocument::from_toml_str(&text).unwrap_err();
        assert!(
            matches!(error, ThemeDocumentError::Validation { .. }),
            "{role_id}.{property} should be rejected by strict document validation"
        );
    }
}

#[test]
fn strict_candidate_validation_rejects_mixed_phase52_role_properties_after_splits() {
    for (role_id, property) in [
        (BerylThemeRole::TranscriptUserInput.id(), "font_size"),
        (BerylThemeRole::TranscriptSelection.id(), "background"),
        (BerylThemeRole::MarkdownBlockQuote.id(), "border"),
        (BerylThemeRole::CodePanelBody.id(), "font_family"),
        (BerylThemeRole::CodePanelBody.id(), "foreground"),
        (BerylThemeRole::CodePanelHeader.id(), "font_weight"),
        (BerylThemeRole::CodePanelHeader.id(), "foreground"),
        (BerylThemeRole::CodePanelBorder.id(), "border"),
        (BerylThemeRole::CodePanelResizeHandle.id(), "background"),
        (BerylThemeRole::SyntaxString.id(), "text_background"),
    ] {
        let text = format!(
            r##"
schema = 1
name = "Invalid Phase 52 Split"

[[role]]
id = "{role_id}"
{property} = "fallback"
"##
        );
        let error = ThemeDocument::from_toml_str(&text).unwrap_err();
        assert!(
            matches!(error, ThemeDocumentError::Validation { .. }),
            "{role_id}.{property} should be rejected by strict document validation"
        );
    }
}

#[test]
fn roles_without_single_primitive_color_do_not_expose_color_property() {
    let schema = built_in_theme_schema();

    for role in BerylThemeRole::ALL.iter().copied().filter(|role| {
        !SINGLE_PRIMITIVE_COLOR_THEME_ROLES.contains(role) && *role != BerylThemeRole::Root
    }) {
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
            ]),
        ),
        (BerylThemeRole::AppWindowTitle, text_property_set()),
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
            property_set(&[BerylThemeProperty::Foreground]),
        ),
        (
            BerylThemeRole::WorkspacePickerRowActive,
            property_set(&[BerylThemeProperty::Color]),
        ),
        (BerylThemeRole::StatusLineCell, surface_property_set()),
        (
            BerylThemeRole::ActivityIndicatorOk,
            property_set(&[BerylThemeProperty::Color]),
        ),
        (BerylThemeRole::MediaPlaceholderText, text_property_set()),
        (BerylThemeRole::PopupRowNormal, BTreeSet::new()),
        (
            BerylThemeRole::SettingsInputFocused,
            property_set(&[BerylThemeProperty::Border]),
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
fn transcript_selection_projection_resolves_text_background_only() {
    let projection = ActiveThemeProjection::built_in();
    let style = projection
        .default_style(BerylThemeRole::TranscriptSelection.id())
        .expect("transcript.selection should have a built-in default style");

    assert!(
        style
            .property(&BerylThemeProperty::TextBackground.id().into())
            .is_some()
    );
    assert!(
        style
            .property(&BerylThemeProperty::Background.id().into())
            .is_none()
    );
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
        .default_style(BerylThemeRole::TranscriptUserInputText.id())
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
        .default_style(BerylThemeRole::TranscriptUserInputText.id())
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
    BerylThemeRole::InteractionHover,
    BerylThemeRole::InteractionPressed,
    BerylThemeRole::InteractionActive,
    BerylThemeRole::InteractionSelected,
    BerylThemeRole::InteractionFocused,
    BerylThemeRole::InteractionDisabled,
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

const FOUNDATION_TEXT_THEME_ROLES: &[BerylThemeRole] = &[
    BerylThemeRole::Text,
    BerylThemeRole::TextMuted,
    BerylThemeRole::TextSubtle,
    BerylThemeRole::TextValue,
    BerylThemeRole::TextLink,
    BerylThemeRole::TextCode,
    BerylThemeRole::TextSemanticInfo,
    BerylThemeRole::TextSemanticWarning,
    BerylThemeRole::TextSemanticError,
    BerylThemeRole::TextSemanticSuccess,
];

const FOUNDATION_SURFACE_THEME_ROLES: &[BerylThemeRole] = &[
    BerylThemeRole::Surface,
    BerylThemeRole::SurfaceWindow,
    BerylThemeRole::SurfacePanel,
    BerylThemeRole::SurfaceElevated,
    BerylThemeRole::SurfaceInset,
    BerylThemeRole::SurfaceOverlay,
];

const COMMON_CONTROL_SURFACE_THEME_ROLES: &[BerylThemeRole] = &[
    BerylThemeRole::ControlButton,
    BerylThemeRole::ControlInput,
    BerylThemeRole::ControlSelection,
    BerylThemeRole::SurfaceRow,
    BerylThemeRole::ControlList,
    BerylThemeRole::ControlMenu,
    BerylThemeRole::ControlMenuItem,
    BerylThemeRole::ControlPopup,
    BerylThemeRole::ControlNotice,
    BerylThemeRole::ControlStatus,
    BerylThemeRole::ControlDropdown,
    BerylThemeRole::ControlColorInput,
    BerylThemeRole::ControlFilePicker,
    BerylThemeRole::ControlTooltip,
    BerylThemeRole::ControlScrollbar,
];

const COMMON_CONTROL_TEXT_THEME_ROLES: &[BerylThemeRole] = &[
    BerylThemeRole::ControlButtonLabel,
    BerylThemeRole::ButtonPrimaryLabel,
    BerylThemeRole::ButtonSecondaryLabel,
    BerylThemeRole::ControlInputText,
    BerylThemeRole::ControlRowLabel,
    BerylThemeRole::ControlListHeader,
    BerylThemeRole::ControlMenuItemLabel,
    BerylThemeRole::ControlPopupHeader,
    BerylThemeRole::ControlNoticeTitle,
    BerylThemeRole::ControlNoticeDetail,
    BerylThemeRole::ControlStatusLabel,
    BerylThemeRole::ControlStatusValue,
    BerylThemeRole::ControlDropdownLabel,
    BerylThemeRole::ControlColorInputLabel,
    BerylThemeRole::ControlColorInputValue,
    BerylThemeRole::ControlFilePickerLabel,
    BerylThemeRole::ControlTooltipText,
];

const COMMON_CONTROL_PRIMITIVE_THEME_ROLES: &[BerylThemeRole] = &[
    BerylThemeRole::PrimitiveCaret,
    BerylThemeRole::PrimitiveFocusRing,
    BerylThemeRole::PrimitiveScrollbarThumb,
    BerylThemeRole::InputCaret,
    BerylThemeRole::SettingsInputCaret,
    BerylThemeRole::FocusRing,
    BerylThemeRole::ScrollbarThumbNormal,
    BerylThemeRole::ScrollbarThumbHover,
    BerylThemeRole::ScrollbarThumbDragging,
];

const FOUNDATION_THEME_ROLES: &[BerylThemeRole] = &[
    BerylThemeRole::Root,
    BerylThemeRole::Text,
    BerylThemeRole::TextMuted,
    BerylThemeRole::TextSubtle,
    BerylThemeRole::TextValue,
    BerylThemeRole::TextLink,
    BerylThemeRole::TextCode,
    BerylThemeRole::TextSemanticInfo,
    BerylThemeRole::TextSemanticWarning,
    BerylThemeRole::TextSemanticError,
    BerylThemeRole::TextSemanticSuccess,
    BerylThemeRole::Surface,
    BerylThemeRole::SurfaceWindow,
    BerylThemeRole::SurfacePanel,
    BerylThemeRole::SurfaceElevated,
    BerylThemeRole::SurfaceInset,
    BerylThemeRole::SurfaceOverlay,
    BerylThemeRole::Primitive,
    BerylThemeRole::PrimitiveSeparator,
    BerylThemeRole::PrimitiveFocusRing,
    BerylThemeRole::PrimitiveCaret,
    BerylThemeRole::PrimitiveAccentMarker,
    BerylThemeRole::PrimitiveResizeHandle,
    BerylThemeRole::PrimitiveScrollbarThumb,
    BerylThemeRole::Control,
    BerylThemeRole::ControlButton,
    BerylThemeRole::ControlButtonLabel,
    BerylThemeRole::ButtonPrimaryLabel,
    BerylThemeRole::ButtonSecondaryLabel,
    BerylThemeRole::ControlInput,
    BerylThemeRole::ControlInputText,
    BerylThemeRole::ControlSelection,
    BerylThemeRole::SurfaceRow,
    BerylThemeRole::ControlRowLabel,
    BerylThemeRole::ControlList,
    BerylThemeRole::ControlListHeader,
    BerylThemeRole::ControlMenu,
    BerylThemeRole::ControlMenuItem,
    BerylThemeRole::ControlMenuItemLabel,
    BerylThemeRole::ControlPopup,
    BerylThemeRole::ControlPopupHeader,
    BerylThemeRole::ControlNotice,
    BerylThemeRole::ControlNoticeTitle,
    BerylThemeRole::ControlNoticeDetail,
    BerylThemeRole::ControlStatus,
    BerylThemeRole::ControlStatusLabel,
    BerylThemeRole::ControlStatusValue,
    BerylThemeRole::ControlDropdown,
    BerylThemeRole::ControlDropdownLabel,
    BerylThemeRole::ControlColorInput,
    BerylThemeRole::ControlColorInputLabel,
    BerylThemeRole::ControlColorInputValue,
    BerylThemeRole::ControlFilePicker,
    BerylThemeRole::ControlFilePickerLabel,
    BerylThemeRole::ControlTooltip,
    BerylThemeRole::ControlTooltipText,
    BerylThemeRole::ControlScrollbar,
    BerylThemeRole::InteractionHover,
    BerylThemeRole::InteractionPressed,
    BerylThemeRole::InteractionActive,
    BerylThemeRole::InteractionSelected,
    BerylThemeRole::InteractionFocused,
    BerylThemeRole::InteractionDisabled,
    BerylThemeRole::SemanticInfo,
    BerylThemeRole::SemanticWarning,
    BerylThemeRole::SemanticError,
    BerylThemeRole::SemanticSuccess,
];

const FOUNDATION_PARENT_EDGES: &[(BerylThemeRole, Option<BerylThemeRole>)] = &[
    (BerylThemeRole::Root, None),
    (BerylThemeRole::Text, Some(BerylThemeRole::Root)),
    (BerylThemeRole::TextMuted, Some(BerylThemeRole::Text)),
    (BerylThemeRole::TextSubtle, Some(BerylThemeRole::Text)),
    (BerylThemeRole::TextValue, Some(BerylThemeRole::Text)),
    (BerylThemeRole::TextLink, Some(BerylThemeRole::Text)),
    (BerylThemeRole::TextCode, Some(BerylThemeRole::Text)),
    (BerylThemeRole::TextSemanticInfo, Some(BerylThemeRole::Text)),
    (
        BerylThemeRole::TextSemanticWarning,
        Some(BerylThemeRole::Text),
    ),
    (
        BerylThemeRole::TextSemanticError,
        Some(BerylThemeRole::Text),
    ),
    (
        BerylThemeRole::TextSemanticSuccess,
        Some(BerylThemeRole::Text),
    ),
    (BerylThemeRole::Surface, Some(BerylThemeRole::Root)),
    (BerylThemeRole::SurfaceWindow, Some(BerylThemeRole::Surface)),
    (BerylThemeRole::SurfacePanel, Some(BerylThemeRole::Surface)),
    (
        BerylThemeRole::SurfaceElevated,
        Some(BerylThemeRole::Surface),
    ),
    (BerylThemeRole::SurfaceInset, Some(BerylThemeRole::Surface)),
    (
        BerylThemeRole::SurfaceOverlay,
        Some(BerylThemeRole::Surface),
    ),
    (BerylThemeRole::Primitive, Some(BerylThemeRole::Root)),
    (
        BerylThemeRole::PrimitiveSeparator,
        Some(BerylThemeRole::Primitive),
    ),
    (
        BerylThemeRole::PrimitiveFocusRing,
        Some(BerylThemeRole::Primitive),
    ),
    (
        BerylThemeRole::PrimitiveCaret,
        Some(BerylThemeRole::Primitive),
    ),
    (
        BerylThemeRole::PrimitiveAccentMarker,
        Some(BerylThemeRole::Primitive),
    ),
    (
        BerylThemeRole::PrimitiveResizeHandle,
        Some(BerylThemeRole::Primitive),
    ),
    (
        BerylThemeRole::PrimitiveScrollbarThumb,
        Some(BerylThemeRole::Primitive),
    ),
    (BerylThemeRole::Control, Some(BerylThemeRole::Root)),
    (BerylThemeRole::ControlButton, Some(BerylThemeRole::Control)),
    (
        BerylThemeRole::ControlButtonLabel,
        Some(BerylThemeRole::Text),
    ),
    (BerylThemeRole::ControlInput, Some(BerylThemeRole::Control)),
    (BerylThemeRole::ControlInputText, Some(BerylThemeRole::Text)),
    (
        BerylThemeRole::ControlSelection,
        Some(BerylThemeRole::InteractionSelected),
    ),
    (BerylThemeRole::SurfaceRow, Some(BerylThemeRole::Control)),
    (BerylThemeRole::ControlRowLabel, Some(BerylThemeRole::Text)),
    (BerylThemeRole::ControlList, Some(BerylThemeRole::Control)),
    (
        BerylThemeRole::ControlListHeader,
        Some(BerylThemeRole::Text),
    ),
    (BerylThemeRole::ControlMenu, Some(BerylThemeRole::Control)),
    (
        BerylThemeRole::ControlMenuItem,
        Some(BerylThemeRole::SurfaceRow),
    ),
    (
        BerylThemeRole::ControlMenuItemLabel,
        Some(BerylThemeRole::Text),
    ),
    (BerylThemeRole::ControlPopup, Some(BerylThemeRole::Control)),
    (
        BerylThemeRole::ControlPopupHeader,
        Some(BerylThemeRole::Text),
    ),
    (BerylThemeRole::ControlNotice, Some(BerylThemeRole::Control)),
    (
        BerylThemeRole::ControlNoticeTitle,
        Some(BerylThemeRole::Text),
    ),
    (
        BerylThemeRole::ControlNoticeDetail,
        Some(BerylThemeRole::TextSubtle),
    ),
    (BerylThemeRole::ControlStatus, Some(BerylThemeRole::Control)),
    (
        BerylThemeRole::ControlStatusLabel,
        Some(BerylThemeRole::TextMuted),
    ),
    (
        BerylThemeRole::ControlStatusValue,
        Some(BerylThemeRole::TextValue),
    ),
    (
        BerylThemeRole::ControlDropdown,
        Some(BerylThemeRole::ControlInput),
    ),
    (
        BerylThemeRole::ControlDropdownLabel,
        Some(BerylThemeRole::Text),
    ),
    (
        BerylThemeRole::ControlColorInput,
        Some(BerylThemeRole::ControlInput),
    ),
    (
        BerylThemeRole::ControlColorInputLabel,
        Some(BerylThemeRole::Text),
    ),
    (
        BerylThemeRole::ControlColorInputValue,
        Some(BerylThemeRole::TextCode),
    ),
    (
        BerylThemeRole::ControlFilePicker,
        Some(BerylThemeRole::ControlInput),
    ),
    (
        BerylThemeRole::ControlFilePickerLabel,
        Some(BerylThemeRole::Text),
    ),
    (
        BerylThemeRole::ControlTooltip,
        Some(BerylThemeRole::ControlPopup),
    ),
    (
        BerylThemeRole::ControlTooltipText,
        Some(BerylThemeRole::TextSubtle),
    ),
    (
        BerylThemeRole::ControlScrollbar,
        Some(BerylThemeRole::Control),
    ),
    (
        BerylThemeRole::InteractionHover,
        Some(BerylThemeRole::SurfaceRow),
    ),
    (
        BerylThemeRole::InteractionPressed,
        Some(BerylThemeRole::ControlButton),
    ),
    (
        BerylThemeRole::InteractionActive,
        Some(BerylThemeRole::Control),
    ),
    (
        BerylThemeRole::InteractionSelected,
        Some(BerylThemeRole::SurfaceRow),
    ),
    (
        BerylThemeRole::InteractionFocused,
        Some(BerylThemeRole::ControlInput),
    ),
    (
        BerylThemeRole::InteractionDisabled,
        Some(BerylThemeRole::Control),
    ),
    (
        BerylThemeRole::SemanticInfo,
        Some(BerylThemeRole::ControlNotice),
    ),
    (
        BerylThemeRole::SemanticWarning,
        Some(BerylThemeRole::ControlNotice),
    ),
    (
        BerylThemeRole::SemanticError,
        Some(BerylThemeRole::ControlNotice),
    ),
    (
        BerylThemeRole::SemanticSuccess,
        Some(BerylThemeRole::ControlNotice),
    ),
];

const COMMON_CONTROL_PARENT_EDGES: &[(BerylThemeRole, Option<BerylThemeRole>)] = &[
    (BerylThemeRole::ControlButton, Some(BerylThemeRole::Control)),
    (
        BerylThemeRole::ControlButtonLabel,
        Some(BerylThemeRole::Text),
    ),
    (BerylThemeRole::ControlInput, Some(BerylThemeRole::Control)),
    (BerylThemeRole::ControlInputText, Some(BerylThemeRole::Text)),
    (
        BerylThemeRole::ControlSelection,
        Some(BerylThemeRole::InteractionSelected),
    ),
    (BerylThemeRole::SurfaceRow, Some(BerylThemeRole::Control)),
    (BerylThemeRole::ControlRowLabel, Some(BerylThemeRole::Text)),
    (BerylThemeRole::ControlList, Some(BerylThemeRole::Control)),
    (
        BerylThemeRole::ControlListHeader,
        Some(BerylThemeRole::Text),
    ),
    (BerylThemeRole::ControlMenu, Some(BerylThemeRole::Control)),
    (
        BerylThemeRole::ControlMenuItem,
        Some(BerylThemeRole::SurfaceRow),
    ),
    (
        BerylThemeRole::ControlMenuItemLabel,
        Some(BerylThemeRole::Text),
    ),
    (BerylThemeRole::ControlPopup, Some(BerylThemeRole::Control)),
    (
        BerylThemeRole::ControlPopupHeader,
        Some(BerylThemeRole::Text),
    ),
    (BerylThemeRole::ControlNotice, Some(BerylThemeRole::Control)),
    (
        BerylThemeRole::ControlNoticeTitle,
        Some(BerylThemeRole::Text),
    ),
    (
        BerylThemeRole::ControlNoticeDetail,
        Some(BerylThemeRole::TextSubtle),
    ),
    (BerylThemeRole::ControlStatus, Some(BerylThemeRole::Control)),
    (
        BerylThemeRole::ControlStatusLabel,
        Some(BerylThemeRole::TextMuted),
    ),
    (
        BerylThemeRole::ControlStatusValue,
        Some(BerylThemeRole::TextValue),
    ),
    (
        BerylThemeRole::ControlDropdown,
        Some(BerylThemeRole::ControlInput),
    ),
    (
        BerylThemeRole::ControlDropdownLabel,
        Some(BerylThemeRole::Text),
    ),
    (
        BerylThemeRole::ControlColorInput,
        Some(BerylThemeRole::ControlInput),
    ),
    (
        BerylThemeRole::ControlColorInputLabel,
        Some(BerylThemeRole::Text),
    ),
    (
        BerylThemeRole::ControlColorInputValue,
        Some(BerylThemeRole::TextCode),
    ),
    (
        BerylThemeRole::ControlFilePicker,
        Some(BerylThemeRole::ControlInput),
    ),
    (
        BerylThemeRole::ControlFilePickerLabel,
        Some(BerylThemeRole::Text),
    ),
    (
        BerylThemeRole::ControlTooltip,
        Some(BerylThemeRole::ControlPopup),
    ),
    (
        BerylThemeRole::ControlTooltipText,
        Some(BerylThemeRole::TextSubtle),
    ),
    (
        BerylThemeRole::ControlScrollbar,
        Some(BerylThemeRole::Control),
    ),
    (
        BerylThemeRole::SurfaceRowHover,
        Some(BerylThemeRole::InteractionHover),
    ),
    (
        BerylThemeRole::SurfaceRowSelected,
        Some(BerylThemeRole::InteractionSelected),
    ),
    (
        BerylThemeRole::SurfaceRowDisabled,
        Some(BerylThemeRole::InteractionDisabled),
    ),
    (
        BerylThemeRole::SurfaceRowUnavailable,
        Some(BerylThemeRole::InteractionDisabled),
    ),
    (
        BerylThemeRole::SurfaceRowError,
        Some(BerylThemeRole::SemanticError),
    ),
    (
        BerylThemeRole::SurfaceRowWarning,
        Some(BerylThemeRole::SemanticWarning),
    ),
    (
        BerylThemeRole::SurfaceRowInfo,
        Some(BerylThemeRole::SemanticInfo),
    ),
    (
        BerylThemeRole::SurfaceRowSuccess,
        Some(BerylThemeRole::SemanticSuccess),
    ),
    (
        BerylThemeRole::ButtonPrimaryNormal,
        Some(BerylThemeRole::ControlButton),
    ),
    (
        BerylThemeRole::ButtonPrimaryLabel,
        Some(BerylThemeRole::ControlButtonLabel),
    ),
    (
        BerylThemeRole::ButtonSecondaryNormal,
        Some(BerylThemeRole::ControlButton),
    ),
    (
        BerylThemeRole::ButtonSecondaryLabel,
        Some(BerylThemeRole::ControlButtonLabel),
    ),
    (
        BerylThemeRole::InputField,
        Some(BerylThemeRole::ControlInput),
    ),
    (
        BerylThemeRole::InputFieldFocused,
        Some(BerylThemeRole::InteractionFocused),
    ),
    (
        BerylThemeRole::InputSelection,
        Some(BerylThemeRole::ControlSelection),
    ),
    (
        BerylThemeRole::InputCaret,
        Some(BerylThemeRole::PrimitiveCaret),
    ),
    (
        BerylThemeRole::InputError,
        Some(BerylThemeRole::SemanticError),
    ),
    (
        BerylThemeRole::PopupSurface,
        Some(BerylThemeRole::ControlPopup),
    ),
    (
        BerylThemeRole::ScrollbarThumbNormal,
        Some(BerylThemeRole::PrimitiveScrollbarThumb),
    ),
    (
        BerylThemeRole::ScrollbarThumbHover,
        Some(BerylThemeRole::PrimitiveScrollbarThumb),
    ),
    (
        BerylThemeRole::ScrollbarThumbDragging,
        Some(BerylThemeRole::PrimitiveScrollbarThumb),
    ),
    (
        BerylThemeRole::FocusRing,
        Some(BerylThemeRole::PrimitiveFocusRing),
    ),
];

const PHASE51_APP_PARENT_EDGES: &[(BerylThemeRole, Option<BerylThemeRole>)] = &[
    (
        BerylThemeRole::AppWindow,
        Some(BerylThemeRole::SurfaceWindow),
    ),
    (
        BerylThemeRole::AppWindowTitle,
        Some(BerylThemeRole::TextValue),
    ),
    (
        BerylThemeRole::MainToolbar,
        Some(BerylThemeRole::SurfaceWindow),
    ),
    (
        BerylThemeRole::MainToolbarTitle,
        Some(BerylThemeRole::TextValue),
    ),
    (
        BerylThemeRole::MainThreadStrip,
        Some(BerylThemeRole::SurfaceWindow),
    ),
    (
        BerylThemeRole::MainThreadStripActiveThread,
        Some(BerylThemeRole::ControlButton),
    ),
    (
        BerylThemeRole::MainThreadStripActiveThreadLabel,
        Some(BerylThemeRole::ControlButtonLabel),
    ),
    (
        BerylThemeRole::MainSeparator,
        Some(BerylThemeRole::PrimitiveSeparator),
    ),
    (
        BerylThemeRole::StructuralSeparator,
        Some(BerylThemeRole::PrimitiveSeparator),
    ),
    (
        BerylThemeRole::ThreadSelectorSurface,
        Some(BerylThemeRole::PopupSurface),
    ),
    (
        BerylThemeRole::ThreadSelectorColumn,
        Some(BerylThemeRole::ColumnSelectorColumn),
    ),
    (
        BerylThemeRole::ThreadSelectorColumnHeader,
        Some(BerylThemeRole::ColumnSelectorHeader),
    ),
    (
        BerylThemeRole::ThreadSelectorColumnHeaderText,
        Some(BerylThemeRole::ControlListHeader),
    ),
    (
        BerylThemeRole::ThreadSelectorRowLabel,
        Some(BerylThemeRole::ControlRowLabel),
    ),
    (
        BerylThemeRole::ThreadSelectorRow,
        Some(BerylThemeRole::ColumnSelectorRow),
    ),
    (
        BerylThemeRole::ThreadSelectorRowMeta,
        Some(BerylThemeRole::TextMuted),
    ),
    (
        BerylThemeRole::ThreadSelectorRowSelected,
        Some(BerylThemeRole::ColumnSelectorRowSelected),
    ),
    (
        BerylThemeRole::ThreadSelectorRowActive,
        Some(BerylThemeRole::ThreadSelectorRowSelected),
    ),
    (
        BerylThemeRole::ThreadSelectorRowActiveText,
        Some(BerylThemeRole::ThreadSelectorRowSelectedText),
    ),
    (
        BerylThemeRole::WorkspacePickerWorkspaceRow,
        Some(BerylThemeRole::SurfaceRow),
    ),
    (
        BerylThemeRole::WorkspacePickerWorkspaceRowTitle,
        Some(BerylThemeRole::ControlRowLabel),
    ),
    (
        BerylThemeRole::WorkspacePickerWorkspaceRowPath,
        Some(BerylThemeRole::TextCode),
    ),
    (
        BerylThemeRole::WorkspacePickerMemberRow,
        Some(BerylThemeRole::SurfaceRow),
    ),
    (
        BerylThemeRole::WorkspacePickerMemberRowTitle,
        Some(BerylThemeRole::ControlRowLabel),
    ),
    (
        BerylThemeRole::WorkspacePickerRuntimeRow,
        Some(BerylThemeRole::ControlMenuItem),
    ),
    (
        BerylThemeRole::WorkspacePickerRowActive,
        Some(BerylThemeRole::PrimitiveAccentMarker),
    ),
    (
        BerylThemeRole::ColumnSelectorColumn,
        Some(BerylThemeRole::ControlList),
    ),
    (
        BerylThemeRole::ColumnSelectorRow,
        Some(BerylThemeRole::SurfaceRow),
    ),
    (
        BerylThemeRole::ColumnSelectorRowSelected,
        Some(BerylThemeRole::SurfaceRowSelected),
    ),
    (
        BerylThemeRole::ColumnSelectorAccent,
        Some(BerylThemeRole::PrimitiveAccentMarker),
    ),
    (
        BerylThemeRole::StatusLineCell,
        Some(BerylThemeRole::ControlStatus),
    ),
    (
        BerylThemeRole::StatusLineLabel,
        Some(BerylThemeRole::ControlStatusLabel),
    ),
    (
        BerylThemeRole::StatusLineValue,
        Some(BerylThemeRole::ControlStatusValue),
    ),
    (
        BerylThemeRole::ActivityIndicatorOk,
        Some(BerylThemeRole::PrimitiveAccentMarker),
    ),
    (
        BerylThemeRole::ActivityResizeHandle,
        Some(BerylThemeRole::PrimitiveResizeHandle),
    ),
    (
        BerylThemeRole::MediaPlaceholder,
        Some(BerylThemeRole::TranscriptShell),
    ),
    (
        BerylThemeRole::MediaPlaceholderText,
        Some(BerylThemeRole::TextMuted),
    ),
    (
        BerylThemeRole::MediaPlaceholderUnavailableText,
        Some(BerylThemeRole::TextSemanticWarning),
    ),
    (
        BerylThemeRole::MediaCaption,
        Some(BerylThemeRole::MediaPlaceholderText),
    ),
];

const PHASE52_PARENT_EDGES: &[(BerylThemeRole, Option<BerylThemeRole>)] = &[
    (
        BerylThemeRole::InputFieldText,
        Some(BerylThemeRole::ControlInputText),
    ),
    (
        BerylThemeRole::TranscriptShell,
        Some(BerylThemeRole::SurfacePanel),
    ),
    (
        BerylThemeRole::TranscriptAssistantFinal,
        Some(BerylThemeRole::Text),
    ),
    (
        BerylThemeRole::TranscriptAssistantCommentary,
        Some(BerylThemeRole::TranscriptAssistantFinal),
    ),
    (
        BerylThemeRole::TranscriptAssistantReasoning,
        Some(BerylThemeRole::TextSubtle),
    ),
    (
        BerylThemeRole::TranscriptUserInput,
        Some(BerylThemeRole::SurfacePanel),
    ),
    (
        BerylThemeRole::TranscriptUserInputText,
        Some(BerylThemeRole::Text),
    ),
    (
        BerylThemeRole::TranscriptActivityCaret,
        Some(BerylThemeRole::PrimitiveCaret),
    ),
    (
        BerylThemeRole::TranscriptSelection,
        Some(BerylThemeRole::ControlSelection),
    ),
    (
        BerylThemeRole::TranscriptQuotePopup,
        Some(BerylThemeRole::PopupSurface),
    ),
    (
        BerylThemeRole::TranscriptQuotePopupText,
        Some(BerylThemeRole::ControlButtonLabel),
    ),
    (
        BerylThemeRole::TranscriptContextMenu,
        Some(BerylThemeRole::PopupSurface),
    ),
    (
        BerylThemeRole::TranscriptContextMenuHeaderText,
        Some(BerylThemeRole::ControlPopupHeader),
    ),
    (
        BerylThemeRole::TranscriptPending,
        Some(BerylThemeRole::TranscriptShell),
    ),
    (
        BerylThemeRole::TranscriptUnavailable,
        Some(BerylThemeRole::TranscriptShell),
    ),
    (
        BerylThemeRole::MarkdownParagraph,
        Some(BerylThemeRole::Text),
    ),
    (
        BerylThemeRole::MarkdownHeading,
        Some(BerylThemeRole::TextValue),
    ),
    (
        BerylThemeRole::MarkdownEmphasis,
        Some(BerylThemeRole::MarkdownParagraph),
    ),
    (
        BerylThemeRole::MarkdownStrongEmphasis,
        Some(BerylThemeRole::MarkdownParagraph),
    ),
    (
        BerylThemeRole::MarkdownInlineCode,
        Some(BerylThemeRole::TextCode),
    ),
    (BerylThemeRole::MarkdownLink, Some(BerylThemeRole::TextLink)),
    (
        BerylThemeRole::MarkdownBlockQuote,
        Some(BerylThemeRole::PrimitiveSeparator),
    ),
    (
        BerylThemeRole::MarkdownListMarker,
        Some(BerylThemeRole::TextMuted),
    ),
    (
        BerylThemeRole::MarkdownThematicBreak,
        Some(BerylThemeRole::PrimitiveSeparator),
    ),
    (
        BerylThemeRole::MarkdownUnsupportedFallback,
        Some(BerylThemeRole::TextCode),
    ),
    (
        BerylThemeRole::CodePanelContainer,
        Some(BerylThemeRole::SurfaceInset),
    ),
    (
        BerylThemeRole::CodePanelHeader,
        Some(BerylThemeRole::SurfaceInset),
    ),
    (
        BerylThemeRole::CodePanelHeaderText,
        Some(BerylThemeRole::TextCode),
    ),
    (
        BerylThemeRole::CodePanelBody,
        Some(BerylThemeRole::SurfaceInset),
    ),
    (
        BerylThemeRole::CodePanelBodyText,
        Some(BerylThemeRole::TextCode),
    ),
    (
        BerylThemeRole::CodePanelBorder,
        Some(BerylThemeRole::PrimitiveSeparator),
    ),
    (
        BerylThemeRole::CodePanelSelection,
        Some(BerylThemeRole::ControlSelection),
    ),
    (
        BerylThemeRole::CodePanelResizeHandle,
        Some(BerylThemeRole::PrimitiveResizeHandle),
    ),
];

const PHASE53_PARENT_EDGES: &[(BerylThemeRole, Option<BerylThemeRole>)] = &[
    (
        BerylThemeRole::SettingsSidebar,
        Some(BerylThemeRole::ControlList),
    ),
    (
        BerylThemeRole::SettingsSidebarRowNormal,
        Some(BerylThemeRole::SurfaceRow),
    ),
    (
        BerylThemeRole::SettingsSidebarRowText,
        Some(BerylThemeRole::ControlRowLabel),
    ),
    (
        BerylThemeRole::SettingsGroupHeaderText,
        Some(BerylThemeRole::ControlListHeader),
    ),
    (
        BerylThemeRole::SettingsRowNormal,
        Some(BerylThemeRole::SurfaceRow),
    ),
    (
        BerylThemeRole::SettingsRowLabel,
        Some(BerylThemeRole::ControlRowLabel),
    ),
    (
        BerylThemeRole::SettingsRowDisabledText,
        Some(BerylThemeRole::TextMuted),
    ),
    (
        BerylThemeRole::SettingsInputText,
        Some(BerylThemeRole::ControlInputText),
    ),
    (
        BerylThemeRole::SettingsInputCaret,
        Some(BerylThemeRole::InputCaret),
    ),
    (
        BerylThemeRole::SettingsButtonPrimaryLabel,
        Some(BerylThemeRole::ButtonPrimaryLabel),
    ),
    (
        BerylThemeRole::SettingsButtonSecondaryLabel,
        Some(BerylThemeRole::ButtonSecondaryLabel),
    ),
    (
        BerylThemeRole::GraphColumn,
        Some(BerylThemeRole::ColumnSelectorColumn),
    ),
    (
        BerylThemeRole::GraphColumnHeader,
        Some(BerylThemeRole::ColumnSelectorHeader),
    ),
    (
        BerylThemeRole::GraphColumnHeaderText,
        Some(BerylThemeRole::ColumnSelectorHeaderText),
    ),
    (
        BerylThemeRole::GraphRowTopic,
        Some(BerylThemeRole::ColumnSelectorRow),
    ),
    (
        BerylThemeRole::GraphRowTopicText,
        Some(BerylThemeRole::ControlRowLabel),
    ),
    (
        BerylThemeRole::GraphRowThreadRefText,
        Some(BerylThemeRole::TextLink),
    ),
    (
        BerylThemeRole::GraphRowSoftLinkText,
        Some(BerylThemeRole::MarkdownEmphasis),
    ),
    (
        BerylThemeRole::ChecklistSidebar,
        Some(BerylThemeRole::SurfacePanel),
    ),
    (
        BerylThemeRole::ChecklistHeader,
        Some(BerylThemeRole::ControlListHeader),
    ),
    (
        BerylThemeRole::ChecklistRow,
        Some(BerylThemeRole::SurfaceRow),
    ),
    (
        BerylThemeRole::ChecklistStatusTodo,
        Some(BerylThemeRole::PrimitiveAccentMarker),
    ),
    (
        BerylThemeRole::ChecklistStatusInProgress,
        Some(BerylThemeRole::PrimitiveAccentMarker),
    ),
    (
        BerylThemeRole::ChecklistStatusDone,
        Some(BerylThemeRole::PrimitiveAccentMarker),
    ),
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

fn built_in_definition_with_root_overrides() -> ThemeDefinition {
    ThemeDefinition::new(
        built_in_theme_definition()
            .roles()
            .iter()
            .map(|role| {
                let mut role_definition = ThemeRoleDefinition::new(role.role_id().clone());
                for (property_id, property_source) in role.properties() {
                    let next_source = if role.role_id().as_str() == BerylThemeRole::Root.id() {
                        match property_id.as_str() {
                            "background" => {
                                StylePropertySource::Concrete(StylePropertyValue::color("#010203"))
                            }
                            "border" => {
                                StylePropertySource::Concrete(StylePropertyValue::color("#223344"))
                            }
                            "color" => {
                                StylePropertySource::Concrete(StylePropertyValue::color("#778899"))
                            }
                            "foreground" => {
                                StylePropertySource::Concrete(StylePropertyValue::color("#abcdef"))
                            }
                            _ => property_source.clone(),
                        }
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
        BerylThemeRole::Root => property_set(&[
            BerylThemeProperty::Background,
            BerylThemeProperty::Border,
            BerylThemeProperty::Color,
            BerylThemeProperty::Foreground,
            BerylThemeProperty::TextBackground,
            BerylThemeProperty::FontFamily,
            BerylThemeProperty::FontSize,
            BerylThemeProperty::FontWeight,
        ]),
        BerylThemeRole::Text
        | BerylThemeRole::TextMuted
        | BerylThemeRole::TextSubtle
        | BerylThemeRole::TextValue
        | BerylThemeRole::TextLink
        | BerylThemeRole::TextCode
        | BerylThemeRole::TextSemanticInfo
        | BerylThemeRole::TextSemanticWarning
        | BerylThemeRole::TextSemanticError
        | BerylThemeRole::TextSemanticSuccess
        | BerylThemeRole::ControlButtonLabel
        | BerylThemeRole::ButtonPrimaryLabel
        | BerylThemeRole::ButtonSecondaryLabel
        | BerylThemeRole::ControlInputText
        | BerylThemeRole::ControlRowLabel
        | BerylThemeRole::ControlListHeader
        | BerylThemeRole::ControlMenuItemLabel
        | BerylThemeRole::ControlPopupHeader
        | BerylThemeRole::ControlNoticeTitle
        | BerylThemeRole::ControlNoticeDetail
        | BerylThemeRole::ControlStatusLabel
        | BerylThemeRole::ControlStatusValue
        | BerylThemeRole::ControlDropdownLabel
        | BerylThemeRole::ControlColorInputLabel
        | BerylThemeRole::ControlColorInputValue
        | BerylThemeRole::ControlFilePickerLabel
        | BerylThemeRole::ControlTooltipText
        | BerylThemeRole::InputFieldText
        | BerylThemeRole::SettingsSidebarRowText
        | BerylThemeRole::SettingsGroupHeaderText
        | BerylThemeRole::SettingsRowLabel
        | BerylThemeRole::SettingsRowValue
        | BerylThemeRole::SettingsRowDisabledText
        | BerylThemeRole::SettingsInputText
        | BerylThemeRole::SettingsButtonPrimaryLabel
        | BerylThemeRole::SettingsButtonSecondaryLabel
        | BerylThemeRole::GraphColumnHeaderText
        | BerylThemeRole::GraphRowTopicText
        | BerylThemeRole::GraphRowChecklistText
        | BerylThemeRole::GraphRowChecklistItemText
        | BerylThemeRole::GraphRowThreadRefText
        | BerylThemeRole::GraphRowThreadRefMeta
        | BerylThemeRole::GraphRowSoftLinkText
        | BerylThemeRole::GraphRowSelectedText
        | BerylThemeRole::GraphRowPendingText
        | BerylThemeRole::GraphRowInvalidText
        | BerylThemeRole::GraphRowErrorText
        | BerylThemeRole::ChecklistHeader
        | BerylThemeRole::ChecklistRowNumberText
        | BerylThemeRole::ChecklistRowText
        | BerylThemeRole::ChecklistStatusTodoText
        | BerylThemeRole::ChecklistStatusInProgressText
        | BerylThemeRole::ChecklistStatusDoneText => property_set(&[
            BerylThemeProperty::Foreground,
            BerylThemeProperty::TextBackground,
            BerylThemeProperty::FontFamily,
            BerylThemeProperty::FontSize,
            BerylThemeProperty::FontWeight,
        ]),
        BerylThemeRole::Surface
        | BerylThemeRole::SurfaceWindow
        | BerylThemeRole::SurfacePanel
        | BerylThemeRole::SurfaceElevated
        | BerylThemeRole::SurfaceInset
        | BerylThemeRole::SurfaceOverlay
        | BerylThemeRole::Control
        | BerylThemeRole::ControlButton
        | BerylThemeRole::ControlInput
        | BerylThemeRole::ControlSelection
        | BerylThemeRole::SurfaceRow
        | BerylThemeRole::ControlList
        | BerylThemeRole::ControlMenu
        | BerylThemeRole::ControlMenuItem
        | BerylThemeRole::ControlPopup
        | BerylThemeRole::ControlNotice
        | BerylThemeRole::ControlStatus
        | BerylThemeRole::ControlDropdown
        | BerylThemeRole::ControlColorInput
        | BerylThemeRole::ControlFilePicker
        | BerylThemeRole::ControlTooltip
        | BerylThemeRole::ControlScrollbar
        | BerylThemeRole::InteractionHover
        | BerylThemeRole::InteractionPressed
        | BerylThemeRole::InteractionActive
        | BerylThemeRole::InteractionSelected
        | BerylThemeRole::InteractionFocused
        | BerylThemeRole::InteractionDisabled
        | BerylThemeRole::SemanticInfo
        | BerylThemeRole::SemanticWarning
        | BerylThemeRole::SemanticError
        | BerylThemeRole::SemanticSuccess => property_set(&[
            BerylThemeProperty::Background,
            BerylThemeProperty::Border,
            BerylThemeProperty::Foreground,
        ]),
        BerylThemeRole::Primitive
        | BerylThemeRole::PrimitiveSeparator
        | BerylThemeRole::PrimitiveFocusRing
        | BerylThemeRole::PrimitiveCaret
        | BerylThemeRole::PrimitiveAccentMarker
        | BerylThemeRole::PrimitiveResizeHandle
        | BerylThemeRole::PrimitiveScrollbarThumb => property_set(&[BerylThemeProperty::Color]),
        BerylThemeRole::AppWindow => property_set(&[
            BerylThemeProperty::Background,
            BerylThemeProperty::Foreground,
        ]),
        BerylThemeRole::AppWindowTitle
        | BerylThemeRole::MainToolbarTitle
        | BerylThemeRole::MainThreadStripActiveThreadLabel => text_property_set(),
        BerylThemeRole::MainToolbar => property_set(&[BerylThemeProperty::Background]),
        BerylThemeRole::MainThreadStripActiveThread => surface_property_set(),
        BerylThemeRole::MainThreadStrip | BerylThemeRole::InputPanel => {
            property_set(&[BerylThemeProperty::Background])
        }
        BerylThemeRole::MainSeparator | BerylThemeRole::StructuralSeparator => {
            property_set(&[BerylThemeProperty::Color])
        }
        BerylThemeRole::Panel
        | BerylThemeRole::SurfaceRowInfo
        | BerylThemeRole::SurfaceRowDisabled
        | BerylThemeRole::ButtonPrimaryNormal
        | BerylThemeRole::ButtonPrimaryHover
        | BerylThemeRole::ButtonPrimaryActive
        | BerylThemeRole::ButtonPrimaryDisabled
        | BerylThemeRole::ButtonSecondaryNormal
        | BerylThemeRole::ButtonSecondaryHover
        | BerylThemeRole::ButtonSecondaryActive
        | BerylThemeRole::ButtonSecondaryDisabled
        | BerylThemeRole::CodePanelButtonNormal
        | BerylThemeRole::CodePanelButtonHover
        | BerylThemeRole::CodePanelButtonActive
        | BerylThemeRole::InputField
        | BerylThemeRole::SettingsSidebar
        | BerylThemeRole::SettingsSidebarRowNormal
        | BerylThemeRole::SettingsSidebarRowSelected
        | BerylThemeRole::SettingsPage
        | BerylThemeRole::SettingsGroup
        | BerylThemeRole::SettingsRowNormal
        | BerylThemeRole::SettingsPopup
        | BerylThemeRole::SettingsButtonPrimary
        | BerylThemeRole::SettingsButtonSecondary
        | BerylThemeRole::SettingsInputNormal
        | BerylThemeRole::TranscriptQuotePopup
        | BerylThemeRole::TranscriptContextMenu
        | BerylThemeRole::TranscriptPending
        | BerylThemeRole::TranscriptUnavailable
        | BerylThemeRole::ChecklistSidebar
        | BerylThemeRole::ThreadSelectorRowSelected
        | BerylThemeRole::ThreadSelectorRowUnavailable
        | BerylThemeRole::PopupSurface
        | BerylThemeRole::NoticeInfo
        | BerylThemeRole::NoticeWarning
        | BerylThemeRole::NoticeError
        | BerylThemeRole::NoticeSuccess
        | BerylThemeRole::GraphOverlay
        | BerylThemeRole::GraphColumn
        | BerylThemeRole::GraphColumnHeader
        | BerylThemeRole::GraphRowTopic
        | BerylThemeRole::GraphRowChecklist
        | BerylThemeRole::GraphRowChecklistItem
        | BerylThemeRole::GraphRowThreadRef
        | BerylThemeRole::GraphRowSoftLink
        | BerylThemeRole::GraphRowSelected
        | BerylThemeRole::GraphRowInvalid
        | BerylThemeRole::GraphRowError
        | BerylThemeRole::ChecklistRow => property_set(&[
            BerylThemeProperty::Background,
            BerylThemeProperty::Border,
            BerylThemeProperty::Foreground,
        ]),
        BerylThemeRole::SurfaceRowHover => property_set(&[BerylThemeProperty::Background]),
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
        | BerylThemeRole::InputError
        | BerylThemeRole::SettingsRowDisabled
        | BerylThemeRole::GraphRowPending
        | BerylThemeRole::GraphRowDisabled
        | BerylThemeRole::GraphRowDisabledText
        | BerylThemeRole::CodePanelSelection
        | BerylThemeRole::ComposerImageMarker
        | BerylThemeRole::PopupRowNormal
        | BerylThemeRole::PopupRowDisabled
        | BerylThemeRole::OverlayBackdrop
        | BerylThemeRole::DiagnosticSurface
        | BerylThemeRole::DiagnosticRow
        | BerylThemeRole::DiagnosticError
        | BerylThemeRole::DiagnosticWarning => BTreeSet::new(),
        BerylThemeRole::SettingsWindow
        | BerylThemeRole::SettingsSidebarRowHover
        | BerylThemeRole::SettingsRowHover
        | BerylThemeRole::SettingsRowModified
        | BerylThemeRole::CodePanelContainer
        | BerylThemeRole::GraphRowHover
        | BerylThemeRole::PopupRowHover
        | BerylThemeRole::PopupRowSelected => property_set(&[BerylThemeProperty::Background]),
        BerylThemeRole::StatusValueWorking
        | BerylThemeRole::StatusValueOk
        | BerylThemeRole::StatusValueError
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
        BerylThemeRole::SettingsInputFocused => property_set(&[BerylThemeProperty::Border]),
        BerylThemeRole::TranscriptShell
        | BerylThemeRole::StatusLine
        | BerylThemeRole::ActivityPanel => property_set(&[
            BerylThemeProperty::Background,
            BerylThemeProperty::Foreground,
        ]),
        BerylThemeRole::MediaPlaceholder
        | BerylThemeRole::MediaPlaceholderLoading
        | BerylThemeRole::MediaPlaceholderUnavailable => {
            property_set(&[BerylThemeProperty::Background])
        }
        BerylThemeRole::TranscriptAssistantFinal
        | BerylThemeRole::TranscriptAssistantCommentary
        | BerylThemeRole::TranscriptAssistantReasoning
        | BerylThemeRole::TranscriptUserInputText
        | BerylThemeRole::TranscriptQuotePopupText
        | BerylThemeRole::TranscriptContextMenuHeaderText
        | BerylThemeRole::MediaPlaceholderText
        | BerylThemeRole::MediaPlaceholderLoadingText
        | BerylThemeRole::MediaPlaceholderUnavailableText
        | BerylThemeRole::MediaCaption
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
        ]),
        BerylThemeRole::TranscriptActivityCaret
        | BerylThemeRole::MarkdownBlockQuote
        | BerylThemeRole::MarkdownThematicBreak
        | BerylThemeRole::CodePanelBorder
        | BerylThemeRole::CodePanelResizeHandle
        | BerylThemeRole::InputCaret
        | BerylThemeRole::SettingsInputCaret
        | BerylThemeRole::ChecklistStatusTodo
        | BerylThemeRole::ChecklistStatusInProgress
        | BerylThemeRole::ChecklistStatusDone
        | BerylThemeRole::ScrollbarThumbNormal
        | BerylThemeRole::ScrollbarThumbHover
        | BerylThemeRole::ScrollbarThumbDragging
        | BerylThemeRole::FocusRing => property_set(&[BerylThemeProperty::Color]),
        BerylThemeRole::InputSelection
        | BerylThemeRole::TranscriptSelection
        | BerylThemeRole::SettingsInputSelection => {
            property_set(&[BerylThemeProperty::TextBackground])
        }
        BerylThemeRole::MediaBorder | BerylThemeRole::SettingsInputError => {
            property_set(&[BerylThemeProperty::Border])
        }
        BerylThemeRole::MarkdownListMarker
        | BerylThemeRole::CodePanelHeaderText
        | BerylThemeRole::CodePanelBodyText => text_property_set(),
        BerylThemeRole::CodePanelHeader | BerylThemeRole::CodePanelBody => {
            property_set(&[BerylThemeProperty::Background])
        }
        BerylThemeRole::TranscriptImageMarker => property_set(&[
            BerylThemeProperty::Foreground,
            BerylThemeProperty::TextBackground,
        ]),
        BerylThemeRole::ThreadSelectorSurface
        | BerylThemeRole::ThreadSelectorColumn
        | BerylThemeRole::ThreadSelectorColumnHeader
        | BerylThemeRole::ThreadSelectorRow
        | BerylThemeRole::ThreadSelectorRowActive
        | BerylThemeRole::WorkspacePickerSurface
        | BerylThemeRole::WorkspacePickerWorkspaceRow
        | BerylThemeRole::WorkspacePickerMemberRow
        | BerylThemeRole::WorkspacePickerRuntimeRow
        | BerylThemeRole::ColumnSelectorColumn
        | BerylThemeRole::ColumnSelectorHeader
        | BerylThemeRole::ColumnSelectorRow
        | BerylThemeRole::ColumnSelectorRowSelected
        | BerylThemeRole::StatusLineCell
        | BerylThemeRole::ActivityRow => surface_property_set(),
        BerylThemeRole::ThreadSelectorHeaderText
        | BerylThemeRole::ThreadSelectorColumnHeaderText
        | BerylThemeRole::ThreadSelectorRowLabel
        | BerylThemeRole::ThreadSelectorRowMeta
        | BerylThemeRole::ThreadSelectorRowSelectedText
        | BerylThemeRole::ThreadSelectorRowActiveText
        | BerylThemeRole::ThreadSelectorRowUnavailableText
        | BerylThemeRole::WorkspacePickerHeaderText
        | BerylThemeRole::WorkspacePickerHeaderDetail
        | BerylThemeRole::WorkspacePickerWorkspaceRowTitle
        | BerylThemeRole::WorkspacePickerWorkspaceRowPath
        | BerylThemeRole::WorkspacePickerMemberRowTitle
        | BerylThemeRole::WorkspacePickerMemberRowPath
        | BerylThemeRole::WorkspacePickerRuntimeRowText
        | BerylThemeRole::WorkspacePickerUnavailableText
        | BerylThemeRole::ColumnSelectorHeaderText
        | BerylThemeRole::StatusLineLabel
        | BerylThemeRole::StatusLineValue
        | BerylThemeRole::ActivityLabel
        | BerylThemeRole::ActivityValue => text_property_set(),
        BerylThemeRole::WorkspacePickerRowActive
        | BerylThemeRole::ColumnSelectorAccent
        | BerylThemeRole::ActivityIndicatorRunning
        | BerylThemeRole::ActivityIndicatorOk
        | BerylThemeRole::ActivityIndicatorError
        | BerylThemeRole::ActivityResizeHandle => property_set(&[BerylThemeProperty::Color]),
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

fn surface_property_set() -> BTreeSet<&'static str> {
    property_set(&[
        BerylThemeProperty::Background,
        BerylThemeProperty::Border,
        BerylThemeProperty::Foreground,
    ])
}

fn text_property_set() -> BTreeSet<&'static str> {
    property_set(&[
        BerylThemeProperty::Foreground,
        BerylThemeProperty::TextBackground,
        BerylThemeProperty::FontFamily,
        BerylThemeProperty::FontSize,
        BerylThemeProperty::FontWeight,
    ])
}

const SINGLE_PRIMITIVE_COLOR_THEME_ROLES: &[BerylThemeRole] = &[
    BerylThemeRole::Primitive,
    BerylThemeRole::PrimitiveSeparator,
    BerylThemeRole::PrimitiveFocusRing,
    BerylThemeRole::PrimitiveCaret,
    BerylThemeRole::PrimitiveAccentMarker,
    BerylThemeRole::PrimitiveResizeHandle,
    BerylThemeRole::PrimitiveScrollbarThumb,
    BerylThemeRole::MainSeparator,
    BerylThemeRole::StructuralSeparator,
    BerylThemeRole::WorkspacePickerRowActive,
    BerylThemeRole::ColumnSelectorAccent,
    BerylThemeRole::ActivityIndicatorRunning,
    BerylThemeRole::ActivityIndicatorOk,
    BerylThemeRole::ActivityIndicatorError,
    BerylThemeRole::ActivityResizeHandle,
    BerylThemeRole::InputCaret,
    BerylThemeRole::TranscriptActivityCaret,
    BerylThemeRole::MarkdownBlockQuote,
    BerylThemeRole::MarkdownThematicBreak,
    BerylThemeRole::CodePanelBorder,
    BerylThemeRole::CodePanelResizeHandle,
    BerylThemeRole::ChecklistStatusTodo,
    BerylThemeRole::ChecklistStatusInProgress,
    BerylThemeRole::ChecklistStatusDone,
    BerylThemeRole::SettingsInputCaret,
    BerylThemeRole::ScrollbarThumbNormal,
    BerylThemeRole::ScrollbarThumbHover,
    BerylThemeRole::ScrollbarThumbDragging,
    BerylThemeRole::FocusRing,
];
