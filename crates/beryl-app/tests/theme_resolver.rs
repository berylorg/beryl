use beryl_app::{
    MAX_THEME_VALIDATION_DIAGNOSTICS, ResolvedStyle, StylePropertyKind, StylePropertySource,
    StylePropertyValue, ThemeDefinition, ThemeDiagnosticKind, ThemePropertySchema,
    ThemeResolutionContext, ThemeResolutionError, ThemeResolver, ThemeRoleDefinition,
    ThemeRoleSchema, ThemeSchema,
};

#[test]
fn resolver_prefers_concrete_value_over_other_sources() {
    let resolver = ThemeResolver::new(
        basic_schema(),
        ThemeDefinition::new(vec![
            ThemeRoleDefinition::new("base").with_property(
                "foreground",
                StylePropertySource::Concrete(StylePropertyValue::color("#010203")),
            ),
            ThemeRoleDefinition::new("child").with_property(
                "foreground",
                StylePropertySource::Concrete(StylePropertyValue::color("#aabbcc")),
            ),
        ]),
    )
    .unwrap();
    let context = ThemeResolutionContext::new().with_ambient_parent(
        ResolvedStyle::new().with_property("foreground", StylePropertyValue::color("#ddeeff")),
    );

    assert_eq!(
        resolver
            .resolve_property("child", "foreground", &context)
            .unwrap(),
        StylePropertyValue::color("#aabbcc")
    );
}

#[test]
fn resolver_follows_static_parent_chain_per_property() {
    let resolver = ThemeResolver::new(
        basic_schema(),
        ThemeDefinition::new(vec![
            ThemeRoleDefinition::new("base").with_property(
                "foreground",
                StylePropertySource::Concrete(StylePropertyValue::color("#112233")),
            ),
            ThemeRoleDefinition::new("child")
                .with_property("foreground", StylePropertySource::StaticParent),
        ]),
    )
    .unwrap();

    assert_eq!(
        resolver
            .resolve_property("child", "foreground", &ThemeResolutionContext::new())
            .unwrap(),
        StylePropertyValue::color("#112233")
    );
}

#[test]
fn resolver_uses_runtime_ambient_parent_only_for_ambient_source() {
    let resolver = ThemeResolver::new(
        basic_schema(),
        ThemeDefinition::new(vec![
            ThemeRoleDefinition::new("inline")
                .with_property(
                    "foreground",
                    StylePropertySource::Concrete(StylePropertyValue::color("#00ffff")),
                )
                .with_property("background", StylePropertySource::AmbientParent),
        ]),
    )
    .unwrap();
    let final_answer_context = ThemeResolutionContext::new().with_ambient_parent(
        ResolvedStyle::new().with_property("background", StylePropertyValue::color("#101112")),
    );
    let user_input_context = ThemeResolutionContext::new().with_ambient_parent(
        ResolvedStyle::new().with_property("background", StylePropertyValue::color("#202122")),
    );

    assert_eq!(
        resolver
            .resolve_property("inline", "foreground", &final_answer_context)
            .unwrap(),
        StylePropertyValue::color("#00ffff")
    );
    assert_eq!(
        resolver
            .resolve_property("inline", "background", &final_answer_context)
            .unwrap(),
        StylePropertyValue::color("#101112")
    );
    assert_eq!(
        resolver
            .resolve_property("inline", "background", &user_input_context)
            .unwrap(),
        StylePropertyValue::color("#202122")
    );
}

#[test]
fn resolver_uses_fallback_for_missing_entries_and_missing_ambient_property() {
    let resolver = ThemeResolver::new(
        basic_schema(),
        ThemeDefinition::new(vec![
            ThemeRoleDefinition::new("inline")
                .with_property("background", StylePropertySource::AmbientParent)
                .with_property("font_weight", StylePropertySource::Fallback),
        ]),
    )
    .unwrap();

    assert_eq!(
        resolver
            .resolve_property("child", "foreground", &ThemeResolutionContext::new())
            .unwrap(),
        StylePropertyValue::color("#222222")
    );
    assert_eq!(
        resolver
            .resolve_property("inline", "background", &ThemeResolutionContext::new())
            .unwrap(),
        StylePropertyValue::color("#333333")
    );
    assert_eq!(
        resolver
            .resolve_property("inline", "font_weight", &ThemeResolutionContext::new())
            .unwrap(),
        StylePropertyValue::font_weight(400)
    );
}

#[test]
fn resolver_supports_every_phase_two_property_kind() {
    let resolver = ThemeResolver::new(basic_schema(), ThemeDefinition::empty()).unwrap();
    let style = resolver
        .resolve_style("base", &ThemeResolutionContext::new())
        .unwrap();

    assert_eq!(
        style.property(&"foreground".into()),
        Some(&StylePropertyValue::color("#111111"))
    );
    assert_eq!(
        style.property(&"font_family".into()),
        Some(&StylePropertyValue::font_family("Inter"))
    );
    assert_eq!(
        style.property(&"font_size".into()),
        Some(&StylePropertyValue::logical_pixels(14.0))
    );
    assert_eq!(
        style.property(&"font_weight".into()),
        Some(&StylePropertyValue::font_weight(400))
    );
}

#[test]
fn resolver_rejects_static_parent_cycles() {
    let schema = ThemeSchema::new(vec![
        ThemeRoleSchema::new("a")
            .with_static_parent("b")
            .with_property(
                "foreground",
                ThemePropertySchema::new(
                    StylePropertyKind::Color,
                    StylePropertyValue::color("#111111"),
                ),
            ),
        ThemeRoleSchema::new("b")
            .with_static_parent("a")
            .with_property(
                "foreground",
                ThemePropertySchema::new(
                    StylePropertyKind::Color,
                    StylePropertyValue::color("#222222"),
                ),
            ),
    ]);

    let diagnostics = ThemeResolver::new(schema, ThemeDefinition::empty()).unwrap_err();

    assert!(
        diagnostics
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind() == ThemeDiagnosticKind::StaticParentCycle)
    );
}

#[test]
fn resolver_rejects_missing_static_parent_references() {
    let diagnostics = ThemeResolver::new(
        basic_schema(),
        ThemeDefinition::new(vec![
            ThemeRoleDefinition::new("child").with_static_parent("missing"),
        ]),
    )
    .unwrap_err();

    assert_eq!(
        diagnostics.diagnostics()[0].kind(),
        ThemeDiagnosticKind::MissingStaticParent
    );
    assert_eq!(
        diagnostics.diagnostics()[0]
            .role_id()
            .map(|role| role.as_str()),
        Some("child")
    );
}

#[test]
fn resolver_rejects_unknown_roles_and_properties() {
    let diagnostics = ThemeResolver::new(
        basic_schema(),
        ThemeDefinition::new(vec![
            ThemeRoleDefinition::new("unknown"),
            ThemeRoleDefinition::new("child").with_property(
                "unknown_property",
                StylePropertySource::Concrete(StylePropertyValue::color("#ffffff")),
            ),
        ]),
    )
    .unwrap_err();
    let kinds: Vec<_> = diagnostics
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.kind())
        .collect();

    assert_eq!(
        kinds,
        vec![
            ThemeDiagnosticKind::UnknownRole,
            ThemeDiagnosticKind::UnknownProperty
        ]
    );
}

#[test]
fn resolver_rejects_properties_outside_role_capabilities() {
    let schema = ThemeSchema::new(vec![ThemeRoleSchema::new("text").with_property(
        "foreground",
        ThemePropertySchema::new(
            StylePropertyKind::Color,
            StylePropertyValue::color("#111111"),
        ),
    )]);

    let diagnostics = ThemeResolver::new(
        schema.clone(),
        ThemeDefinition::new(vec![ThemeRoleDefinition::new("text").with_property(
            "background",
            StylePropertySource::Concrete(StylePropertyValue::color("#222222")),
        )]),
    )
    .unwrap_err();

    assert_eq!(
        diagnostics.diagnostics()[0].kind(),
        ThemeDiagnosticKind::UnknownProperty
    );
    assert_eq!(
        diagnostics.diagnostics()[0]
            .property_id()
            .map(|property| property.as_str()),
        Some("background")
    );

    let resolver = ThemeResolver::new(schema, ThemeDefinition::empty()).unwrap();
    let style = resolver
        .resolve_style("text", &ThemeResolutionContext::new())
        .unwrap();

    assert_eq!(style.properties().len(), 1);
    assert_eq!(
        style.property(&"foreground".into()),
        Some(&StylePropertyValue::color("#111111"))
    );
    assert!(style.property(&"background".into()).is_none());
    assert_eq!(
        resolver
            .resolve_property("text", "background", &ThemeResolutionContext::new())
            .unwrap_err(),
        ThemeResolutionError::UnknownProperty {
            role_id: "text".into(),
            property_id: "background".into(),
        }
    );
}

#[test]
fn resolver_rejects_invalid_property_types_and_values() {
    let diagnostics = ThemeResolver::new(
        basic_schema(),
        ThemeDefinition::new(vec![
            ThemeRoleDefinition::new("child")
                .with_property(
                    "foreground",
                    StylePropertySource::Concrete(StylePropertyValue::font_weight(400)),
                )
                .with_property(
                    "background",
                    StylePropertySource::Concrete(StylePropertyValue::color("blue")),
                ),
        ]),
    )
    .unwrap_err();
    let kinds: Vec<_> = diagnostics
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.kind())
        .collect();

    assert_eq!(
        kinds,
        vec![
            ThemeDiagnosticKind::InvalidPropertyValue,
            ThemeDiagnosticKind::InvalidPropertyType,
        ]
    );
}

#[test]
fn resolver_returns_runtime_errors_for_unknown_resolution_inputs() {
    let resolver = ThemeResolver::new(basic_schema(), ThemeDefinition::empty()).unwrap();

    assert_eq!(
        resolver
            .resolve_property("missing", "foreground", &ThemeResolutionContext::new())
            .unwrap_err(),
        ThemeResolutionError::UnknownRole {
            role_id: "missing".into()
        }
    );
    assert_eq!(
        resolver
            .resolve_property("child", "missing", &ThemeResolutionContext::new())
            .unwrap_err(),
        ThemeResolutionError::UnknownProperty {
            role_id: "child".into(),
            property_id: "missing".into(),
        }
    );
}

#[test]
fn resolver_validation_diagnostics_are_bounded_and_stable() {
    let roles = (0..MAX_THEME_VALIDATION_DIAGNOSTICS + 3)
        .map(|index| ThemeRoleDefinition::new(format!("unknown-{index:03}")))
        .collect();

    let diagnostics = ThemeResolver::new(basic_schema(), ThemeDefinition::new(roles)).unwrap_err();

    assert_eq!(
        diagnostics.diagnostics().len(),
        MAX_THEME_VALIDATION_DIAGNOSTICS
    );
    assert_eq!(diagnostics.truncated_count(), 3);
    assert_eq!(
        diagnostics.diagnostics()[0]
            .role_id()
            .map(|role| role.as_str()),
        Some("unknown-000")
    );
    assert!(
        diagnostics
            .diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.kind() == ThemeDiagnosticKind::UnknownRole)
    );
}

fn basic_schema() -> ThemeSchema {
    ThemeSchema::new(vec![
        all_properties_role("base", "#111111", "#000000"),
        all_properties_role("child", "#222222", "#111111").with_static_parent("base"),
        all_properties_role("inline", "#00ffff", "#333333").with_static_parent("base"),
    ])
}

fn all_properties_role(
    role_id: &'static str,
    foreground: &'static str,
    background: &'static str,
) -> ThemeRoleSchema {
    ThemeRoleSchema::new(role_id)
        .with_property(
            "foreground",
            ThemePropertySchema::new(
                StylePropertyKind::Color,
                StylePropertyValue::color(foreground),
            ),
        )
        .with_property(
            "background",
            ThemePropertySchema::new(
                StylePropertyKind::Color,
                StylePropertyValue::color(background),
            ),
        )
        .with_property(
            "font_family",
            ThemePropertySchema::new(
                StylePropertyKind::FontFamily,
                StylePropertyValue::font_family("Inter"),
            ),
        )
        .with_property(
            "font_size",
            ThemePropertySchema::new(
                StylePropertyKind::LogicalPixels,
                StylePropertyValue::logical_pixels(14.0),
            ),
        )
        .with_property(
            "font_weight",
            ThemePropertySchema::new(
                StylePropertyKind::FontWeight,
                StylePropertyValue::font_weight(400),
            ),
        )
}
