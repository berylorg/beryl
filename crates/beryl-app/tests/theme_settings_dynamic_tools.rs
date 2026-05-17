#![allow(dead_code)]

#[path = "support/tempdir.rs"]
mod tempdir_support;

use beryl_app::{
    AgentPreferences, AppearanceSettings, BerylThemeProperty, BerylThemeRole, GuiPreferences,
    INSTALL_THEME_TOOL, MAX_THEME_ACTIVE_DOCUMENT_RESPONSE_BYTES, MAX_THEME_TOOL_NAME_BYTES,
    NotificationPreferences, OperationPreferences, PREVIEW_THEME_TOOL,
    READ_THEME_AUTHORING_GUIDE_TOOL, SAVE_THEME_AS_TOOL, SettingsDynamicToolRequest,
    ThemeAuthoringGuideSection, ThemeDocument, ThemeDynamicToolRequest, ThemeRepositoryStore,
    ThemeSaveAsSource, UPDATE_GUI_SETTINGS_TOOL, UPDATE_THEME_TOOL,
    VALIDATE_GUI_SETTINGS_UPDATE_TOOL, VALIDATE_THEME_DOCUMENT_TOOL,
    built_in_theme_supported_properties, gui_settings_snapshot_value,
    parse_beryl_settings_dynamic_tool_request, parse_beryl_theme_dynamic_tool_request,
    settings_validation_value, theme_authoring_guide_value, theme_repository_value,
    theme_schema_value, validate_theme_document_value,
};
use beryl_backend::{DynamicToolCallRequest, parse_dynamic_tool_call_request};
use serde_json::{Value, json};

#[test]
fn theme_save_as_parser_requires_exactly_one_source() {
    let document = theme_document_text("#102030");
    let document_request = dynamic_tool_request(
        SAVE_THEME_AS_TOOL,
        json!({
            "name": "Copied",
            "document": document,
        }),
    );
    let source_request = dynamic_tool_request(
        SAVE_THEME_AS_TOOL,
        json!({
            "name": "Copied",
            "sourceThemeId": "built-in",
        }),
    );
    let ambiguous_request = dynamic_tool_request(
        SAVE_THEME_AS_TOOL,
        json!({
            "name": "Copied",
            "document": theme_document_text("#102030"),
            "sourceThemeId": "built-in",
        }),
    );

    match parse_beryl_theme_dynamic_tool_request(&document_request).unwrap() {
        ThemeDynamicToolRequest::SaveAs { name, source } => {
            assert_eq!(name, "Copied");
            assert!(matches!(source, ThemeSaveAsSource::Document(_)));
        }
        other => panic!("expected SaveAs request, got {other:?}"),
    }
    match parse_beryl_theme_dynamic_tool_request(&source_request).unwrap() {
        ThemeDynamicToolRequest::SaveAs { source, .. } => {
            assert!(matches!(
                source,
                ThemeSaveAsSource::ExistingTheme(id)
                    if id.as_str() == "built-in"
            ));
        }
        other => panic!("expected SaveAs request, got {other:?}"),
    }

    let error = parse_beryl_theme_dynamic_tool_request(&ambiguous_request).unwrap_err();
    assert_eq!(error.kind(), "invalid_arguments");
    assert!(error.to_string().contains("exactly one"));
}

#[test]
fn theme_tool_parser_rejects_invalid_documents_and_oversized_names() {
    let invalid_document = r##"
schema = 1

[[role]]
id = "not.a.real.role"
foreground = { value = "#112233" }
"##;
    let invalid_document_request = dynamic_tool_request(
        PREVIEW_THEME_TOOL,
        json!({
            "document": invalid_document,
        }),
    );
    let oversized_name_request = dynamic_tool_request(
        INSTALL_THEME_TOOL,
        json!({
            "name": "x".repeat(MAX_THEME_TOOL_NAME_BYTES + 1),
            "document": theme_document_text("#102030"),
        }),
    );

    let invalid_document_error =
        parse_beryl_theme_dynamic_tool_request(&invalid_document_request).unwrap_err();
    assert_eq!(invalid_document_error.kind(), "invalid_theme_document");

    let oversized_name_error =
        parse_beryl_theme_dynamic_tool_request(&oversized_name_request).unwrap_err();
    assert_eq!(oversized_name_error.kind(), "invalid_arguments");
}

#[test]
fn theme_tool_parser_rejects_unsupported_properties_for_document_mutations() {
    let unsupported_property_documents = [
        r##"
schema = 1
name = "Unsupported Property"

[[role]]
id = "app.window"
not_a_property = { value = "#112233" }
"##,
        r##"
schema = 1
name = "Old Separator Border"

[[role]]
id = "main.separator"
border = { value = "#112233" }
"##,
        r##"
schema = 1
name = "Non Separator Color"

[[role]]
id = "app.window"
color = { value = "#112233" }
"##,
    ];

    for document in unsupported_property_documents {
        let cases = [
            dynamic_tool_request(
                PREVIEW_THEME_TOOL,
                json!({
                    "document": document,
                }),
            ),
            dynamic_tool_request(
                INSTALL_THEME_TOOL,
                json!({
                    "name": "Unsupported Property",
                    "document": document,
                }),
            ),
            dynamic_tool_request(
                UPDATE_THEME_TOOL,
                json!({
                    "themeId": "some-theme",
                    "document": document,
                }),
            ),
            dynamic_tool_request(
                SAVE_THEME_AS_TOOL,
                json!({
                    "name": "Unsupported Property Copy",
                    "document": document,
                }),
            ),
        ];

        for request in cases {
            let error = parse_beryl_theme_dynamic_tool_request(&request).unwrap_err();
            assert_eq!(error.kind(), "invalid_theme_document");
        }
    }
}

#[test]
fn theme_repository_read_value_bounds_metadata_and_active_document() {
    let root = unique_temp_dir();
    let store = ThemeRepositoryStore::new(&root);
    let snapshot = store
        .save_as_theme("Ocean", compact_theme_definition("#102030"))
        .unwrap();

    let metadata_only = theme_repository_value(&snapshot, false).unwrap();
    let with_document = theme_repository_value(&snapshot, true).unwrap();
    let active_document = &with_document["activeDocument"];
    let active_document_text = active_document["text"].as_str().unwrap();

    assert_eq!(metadata_only["activeThemeId"], "ocean");
    assert_eq!(metadata_only["activeDocument"], Value::Null);
    assert_eq!(metadata_only["themesTruncated"], false);
    assert_eq!(active_document["themeId"], "ocean");
    assert_eq!(active_document["name"], "Ocean");
    assert_eq!(active_document["truncated"], false);
    assert_eq!(
        active_document["byteLimit"],
        MAX_THEME_ACTIVE_DOCUMENT_RESPONSE_BYTES
    );
    assert_eq!(
        active_document["byteLength"],
        active_document_text.len() as u64
    );
    assert_eq!(
        active_document["retainedByteLength"],
        active_document_text.len() as u64
    );
    assert_eq!(active_document["omittedByteLength"], 0);
    assert!(active_document_text.contains("[[role]]"));
    assert!(active_document_text.contains("id = \"ocean\""));

    root.close().unwrap();
}

#[test]
fn theme_schema_read_value_supports_bounded_discovery() {
    let value = theme_schema_value(None, 1).unwrap();

    assert_eq!(value["roles"].as_array().unwrap().len(), 1);
    assert!(value["roleCount"].as_u64().unwrap() > 1);
    assert_eq!(value["rolesTruncated"], true);
    assert_eq!(
        value["supportedSources"],
        json!([
            "static_parent",
            "ambient_parent",
            "fallback",
            "concrete_value"
        ])
    );
}

#[test]
fn theme_schema_read_value_reports_role_specific_supported_properties() {
    let thematic_break = theme_schema_value(Some(BerylThemeRole::MarkdownThematicBreak.id()), 8)
        .expect("schema read should succeed");
    let thematic_role = first_role(&thematic_break);
    let thematic_properties = property_ids(&thematic_role["properties"]);

    assert_eq!(
        thematic_properties,
        vec![BerylThemeProperty::Color.id().to_string()]
    );
    assert_eq!(thematic_role["properties"][0]["kind"], "color");
    assert!(!thematic_properties.contains(&BerylThemeProperty::Border.id().to_string()));
    assert!(!thematic_properties.contains(&BerylThemeProperty::Foreground.id().to_string()));

    let code_panel_body = theme_schema_value(Some(BerylThemeRole::CodePanelBody.id()), 8).unwrap();
    let code_panel_properties = property_ids(&first_role(&code_panel_body)["properties"]);
    let mut expected = built_in_theme_supported_properties(BerylThemeRole::CodePanelBody)
        .iter()
        .map(|property| property.id().to_string())
        .collect::<Vec<_>>();
    expected.sort();

    assert_eq!(code_panel_properties, expected);
    assert!(!code_panel_properties.contains(&BerylThemeProperty::Border.id().to_string()));
    assert!(!code_panel_properties.contains(&BerylThemeProperty::TextBackground.id().to_string()));
}

#[test]
fn theme_authoring_guide_parser_accepts_defaults_and_rejects_bad_arguments() {
    let default_request = dynamic_tool_request(READ_THEME_AUTHORING_GUIDE_TOOL, json!({}));
    let invalid_section_request = dynamic_tool_request(
        READ_THEME_AUTHORING_GUIDE_TOOL,
        json!({
            "section": "not_a_section",
        }),
    );
    let overlong_prefix_request = dynamic_tool_request(
        READ_THEME_AUTHORING_GUIDE_TOOL,
        json!({
            "rolePrefix": "x".repeat(MAX_THEME_TOOL_NAME_BYTES + 1),
        }),
    );
    let unknown_field_request = dynamic_tool_request(
        READ_THEME_AUTHORING_GUIDE_TOOL,
        json!({
            "unknown": true,
        }),
    );

    match parse_beryl_theme_dynamic_tool_request(&default_request).unwrap() {
        ThemeDynamicToolRequest::ReadAuthoringGuide {
            section,
            role_prefix,
            limit,
        } => {
            assert_eq!(section, ThemeAuthoringGuideSection::All);
            assert_eq!(role_prefix, None);
            assert_eq!(limit, 24);
        }
        other => panic!("expected ReadAuthoringGuide request, got {other:?}"),
    }

    assert_eq!(
        parse_beryl_theme_dynamic_tool_request(&invalid_section_request)
            .unwrap_err()
            .kind(),
        "invalid_arguments"
    );
    assert_eq!(
        parse_beryl_theme_dynamic_tool_request(&overlong_prefix_request)
            .unwrap_err()
            .kind(),
        "invalid_arguments"
    );
    assert_eq!(
        parse_beryl_theme_dynamic_tool_request(&unknown_field_request)
            .unwrap_err()
            .kind(),
        "invalid_arguments"
    );
}

#[test]
fn theme_validation_parser_accepts_raw_documents_and_bounds_inputs() {
    let invalid_candidate = "schema = [";
    let request = dynamic_tool_request(
        VALIDATE_THEME_DOCUMENT_TOOL,
        json!({
            "document": invalid_candidate,
            "includeSummary": false,
            "explainRoles": ["app.window"],
            "roleExplanationLimit": 99,
        }),
    );
    let overlarge_document_request = dynamic_tool_request(
        VALIDATE_THEME_DOCUMENT_TOOL,
        json!({
            "document": "x".repeat(64 * 1024 + 1),
        }),
    );
    let too_many_roles_request = dynamic_tool_request(
        VALIDATE_THEME_DOCUMENT_TOOL,
        json!({
            "document": invalid_candidate,
            "explainRoles": (0..33).map(|index| format!("role.{index}")).collect::<Vec<_>>(),
        }),
    );
    let unknown_field_request = dynamic_tool_request(
        VALIDATE_THEME_DOCUMENT_TOOL,
        json!({
            "document": invalid_candidate,
            "unknown": true,
        }),
    );

    match parse_beryl_theme_dynamic_tool_request(&request).unwrap() {
        ThemeDynamicToolRequest::ValidateDocument {
            document,
            include_summary,
            explain_roles,
            role_explanation_limit,
        } => {
            assert_eq!(document, invalid_candidate);
            assert!(!include_summary);
            assert_eq!(explain_roles, vec!["app.window"]);
            assert_eq!(role_explanation_limit, 32);
        }
        other => panic!("expected ValidateDocument request, got {other:?}"),
    }

    assert_eq!(
        parse_beryl_theme_dynamic_tool_request(&overlarge_document_request)
            .unwrap_err()
            .kind(),
        "invalid_arguments"
    );
    assert_eq!(
        parse_beryl_theme_dynamic_tool_request(&too_many_roles_request)
            .unwrap_err()
            .kind(),
        "invalid_arguments"
    );
    assert_eq!(
        parse_beryl_theme_dynamic_tool_request(&unknown_field_request)
            .unwrap_err()
            .kind(),
        "invalid_arguments"
    );
}

#[test]
fn theme_authoring_guide_contains_required_guidance_without_private_settings() {
    let value =
        theme_authoring_guide_value(ThemeAuthoringGuideSection::All, Some("transcript."), 2);
    let encoded = serde_json::to_string(&value).unwrap();

    assert_eq!(value["roleHints"]["roles"].as_array().unwrap().len(), 2);
    assert_eq!(value["roleHints"]["rolesTruncated"], true);
    assert!(encoded.contains("beryl-theme"));
    assert!(encoded.contains("schema = 1"));
    assert!(encoded.contains("static_parent"));
    assert!(encoded.contains("ambient_parent"));
    assert!(encoded.contains("fallback"));
    assert!(encoded.contains("transcript.turn.assistant.commentary"));
    assert!(encoded.contains("code_panel.body"));
    assert!(encoded.contains("settings.row"));
    assert!(encoded.contains("troubleshooting"));
    assert!(!encoded.contains("developerInstructions"));
    assert!(!encoded.contains("endTurnSoundPath"));
}

#[test]
fn theme_authoring_guide_role_hints_include_supported_properties() {
    let value = theme_authoring_guide_value(
        ThemeAuthoringGuideSection::Overview,
        Some(BerylThemeRole::CodePanelBody.id()),
        4,
    );
    let role = first_role(&value["roleHints"]);
    let properties = property_ids(&role["supportedProperties"]);

    assert_eq!(role["id"], BerylThemeRole::CodePanelBody.id());
    let mut expected = built_in_theme_supported_properties(BerylThemeRole::CodePanelBody)
        .iter()
        .map(|property| property.id().to_string())
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(properties, expected);
    assert_eq!(
        role["supportedPropertyCount"],
        built_in_theme_supported_properties(BerylThemeRole::CodePanelBody).len()
    );
    assert!(!properties.contains(&BerylThemeProperty::Border.id().to_string()));
}

#[test]
fn theme_validation_accepts_valid_documents_and_explains_sources() {
    let root = unique_temp_dir();
    let snapshot = ThemeRepositoryStore::new(&root).load_or_default().unwrap();
    let document = r##"
schema = 1
name = "Validation Theme"

[[role]]
id = "app.window"
foreground = { value = "#112233" }

[[role]]
id = "panel"
foreground = "static_parent"

[[role]]
id = "markdown.inline_code"
text_background = "ambient_parent"

[[role]]
id = "settings.row.normal"
foreground = "fallback"

[[role]]
id = "markdown.thematic_break"
color = { value = "#445566" }
"##;

    let value = validate_theme_document_value(
        document,
        true,
        &[
            "app.window".to_string(),
            "panel".to_string(),
            "markdown.inline_code".to_string(),
            "settings.row.normal".to_string(),
            "markdown.thematic_break".to_string(),
        ],
        5,
        &snapshot,
    );
    let encoded = serde_json::to_string(&value).unwrap();

    assert_eq!(value["valid"], true);
    assert_eq!(value["summary"]["name"], "Validation Theme");
    assert!(encoded.contains("concrete_value"));
    assert!(encoded.contains("static_parent"));
    assert!(encoded.contains("ambient_parent"));
    assert!(encoded.contains("fallback"));
    assert!(encoded.contains("resolvedWithoutAmbient"));
    assert_eq!(value["roleExplanationsTruncated"], false);
    let thematic_explanation = role_by_id(
        &value["roleExplanations"],
        BerylThemeRole::MarkdownThematicBreak.id(),
    );
    assert_eq!(
        property_ids(&thematic_explanation["properties"]),
        vec![BerylThemeProperty::Color.id().to_string()]
    );
    let thematic_summary = role_by_id(
        &value["summary"]["roles"],
        BerylThemeRole::MarkdownThematicBreak.id(),
    );
    assert_eq!(
        property_ids(&thematic_summary["supportedProperties"]),
        vec![BerylThemeProperty::Color.id().to_string()]
    );
    assert_eq!(
        property_ids(&thematic_summary["properties"]),
        vec![BerylThemeProperty::Color.id().to_string()]
    );

    root.close().unwrap();
}

#[test]
fn theme_validation_reports_document_and_resolver_errors_without_mutation() {
    let root = unique_temp_dir();
    let snapshot = ThemeRepositoryStore::new(&root).load_or_default().unwrap();
    let cases = [
        ("schema = [", "parse_toml"),
        ("schema = 2", "invalid_schema"),
        (
            r##"
schema = 1

[[role]]
id = "app.window"
foreground = "not_a_source"
"##,
            "invalid_property_source",
        ),
        (
            r##"
schema = 1

[[role]]
id = "app.window"
foreground = { value = "blue" }
"##,
            "invalid_property_value",
        ),
        (
            r##"
schema = 1

[[role]]
id = "missing.role"
foreground = { value = "#112233" }
"##,
            "unknown_role",
        ),
        (
            r##"
schema = 1

[[role]]
id = "app.window"
not_a_property = { value = "#112233" }
"##,
            "unknown_property",
        ),
        (
            r##"
schema = 1

[[role]]
id = "app.window"
foreground = { value = "#112233" }

[[role]]
id = "app.window"
background = { value = "#223344" }
"##,
            "duplicate_role",
        ),
        (
            r##"
schema = 1

[[role]]
id = "app.window"
static_parent = "missing.role"
"##,
            "missing_static_parent",
        ),
        (
            r##"
schema = 1

[[role]]
id = "app.window"
static_parent = "main.toolbar"

[[role]]
id = "main.toolbar"
static_parent = "app.window"
"##,
            "static_parent_cycle",
        ),
        (
            r##"
schema = 1
id = "built-in"

[[role]]
id = "app.window"
foreground = { value = "#112233" }
"##,
            "duplicate_embedded_theme_id",
        ),
    ];

    for (document, expected_kind) in cases {
        let value = validate_theme_document_value(document, true, &[], 8, &snapshot);
        let diagnostics = value["diagnostics"].as_array().unwrap();

        assert_eq!(value["valid"], false, "{expected_kind}");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic["kind"] == expected_kind),
            "{expected_kind}: {diagnostics:?}"
        );
        assert!(value["diagnosticCount"].as_u64().unwrap() >= 1);
    }

    root.close().unwrap();
}

#[test]
fn theme_validation_path_is_source_level_non_mutating() {
    let validation_source = include_str!("../src/theme_dynamic_tools/validation.rs");
    let dynamic_theme_source = include_str!("../src/shell/dynamic_theme.rs");
    let immediate_body = rust_function_body(
        dynamic_theme_source,
        "fn handle_beryl_theme_immediate_tool_result",
    );

    assert!(immediate_body.contains("ThemeDynamicToolRequest::ValidateDocument"));
    assert!(immediate_body.contains("validate_theme_document_value("));
    assert!(!validation_source.contains("handle_dynamic_theme_preview"));
    assert!(!validation_source.contains("theme_candidate_state"));
    assert!(!validation_source.contains(".install_theme("));
    assert!(!validation_source.contains(".update_theme("));
    assert!(!validation_source.contains(".save_as_theme("));
    assert!(!validation_source.contains(".activate_theme("));
}

#[test]
fn settings_snapshot_redacts_local_paths_and_developer_instruction_text() {
    let root = unique_temp_dir();
    let secret_sound_path = root.path().join("top-secret-sound.wav");
    let developer_instructions = "do not expose this instruction";
    let preferences = GuiPreferences {
        operations: OperationPreferences::with_context_compaction_timeout_seconds(120).unwrap(),
        notifications: NotificationPreferences::with_end_turn_sound_path(Some(
            secret_sound_path.clone(),
        ))
        .unwrap(),
        agent: AgentPreferences::with_developer_instructions(Some(
            developer_instructions.to_string(),
        )),
    };
    let themes = ThemeRepositoryStore::new(&root).load_or_default().unwrap();

    let snapshot = gui_settings_snapshot_value(&preferences, &themes);
    let encoded = serde_json::to_string(&snapshot).unwrap();

    assert_eq!(
        snapshot["notifications"]["endTurnSoundPath"]["configured"],
        true
    );
    assert_eq!(
        snapshot["notifications"]["endTurnSoundPath"]["extension"],
        "wav"
    );
    assert!(snapshot["notifications"]["endTurnSoundPath"]["pathByteLength"].is_number());
    assert_eq!(
        snapshot["agent"]["developerInstructions"]["characterCount"],
        developer_instructions.chars().count()
    );
    assert_eq!(snapshot["appearance"]["installedThemeCount"], 1);
    assert_eq!(snapshot["appearance"]["installedThemesTruncated"], false);
    assert!(snapshot["agent"]["developerInstructions"]["fingerprint"].is_string());
    assert!(!encoded.contains("top-secret-sound"));
    assert!(!encoded.contains(developer_instructions));

    root.close().unwrap();
}

#[test]
fn settings_update_parser_preserves_explicit_null_and_reports_noop_validation() {
    let root = unique_temp_dir();
    let secret_sound_path = root.path().join("notify.wav");
    let current = GuiPreferences {
        operations: OperationPreferences::with_context_compaction_timeout_seconds(120).unwrap(),
        notifications: NotificationPreferences::with_end_turn_sound_path(Some(secret_sound_path))
            .unwrap(),
        agent: AgentPreferences::with_developer_instructions(Some("current".to_string())),
    };
    let clear_request = dynamic_tool_request(
        UPDATE_GUI_SETTINGS_TOOL,
        json!({
            "notifications": {
                "endTurnSoundPath": null
            },
            "agent": {
                "developerInstructions": null
            }
        }),
    );
    let noop_request = dynamic_tool_request(
        VALIDATE_GUI_SETTINGS_UPDATE_TOOL,
        json!({
            "operations": {
                "contextCompactionTimeoutSeconds": 120
            }
        }),
    );

    let SettingsDynamicToolRequest::Update { update } =
        parse_beryl_settings_dynamic_tool_request(&clear_request).unwrap()
    else {
        panic!("expected settings update request");
    };
    let cleared = update.apply_to(&current).unwrap();
    assert_eq!(cleared.notifications.end_turn_sound_path(), None);
    assert_eq!(cleared.agent.developer_instructions(), None);

    let SettingsDynamicToolRequest::Validate { update } =
        parse_beryl_settings_dynamic_tool_request(&noop_request).unwrap()
    else {
        panic!("expected settings validation request");
    };
    let validation = settings_validation_value(&current, &update).unwrap();
    assert_eq!(validation["valid"], true);
    assert_eq!(validation["changed"], false);

    root.close().unwrap();
}

#[test]
fn settings_update_parser_rejects_unknown_keys_and_invalid_values() {
    let unknown_key_request = dynamic_tool_request(
        UPDATE_GUI_SETTINGS_TOOL,
        json!({
            "workspace": {}
        }),
    );
    let invalid_timeout_request = dynamic_tool_request(
        UPDATE_GUI_SETTINGS_TOOL,
        json!({
            "operations": {
                "contextCompactionTimeoutSeconds": 0
            }
        }),
    );
    let oversized_timeout_request = dynamic_tool_request(
        UPDATE_GUI_SETTINGS_TOOL,
        json!({
            "operations": {
                "contextCompactionTimeoutSeconds": "1".repeat(33)
            }
        }),
    );
    let invalid_sound_path_request = dynamic_tool_request(
        UPDATE_GUI_SETTINGS_TOOL,
        json!({
            "notifications": {
                "endTurnSoundPath": "relative.mp3"
            }
        }),
    );

    let unknown_key_error =
        parse_beryl_settings_dynamic_tool_request(&unknown_key_request).unwrap_err();
    assert_eq!(unknown_key_error.kind(), "invalid_arguments");

    let invalid_timeout_error =
        parse_beryl_settings_dynamic_tool_request(&invalid_timeout_request).unwrap_err();
    assert_eq!(invalid_timeout_error.kind(), "invalid_field");

    let oversized_timeout_error =
        parse_beryl_settings_dynamic_tool_request(&oversized_timeout_request).unwrap_err();
    assert_eq!(oversized_timeout_error.kind(), "invalid_field");

    let invalid_sound_path_error =
        parse_beryl_settings_dynamic_tool_request(&invalid_sound_path_request).unwrap_err();
    assert_eq!(invalid_sound_path_error.kind(), "invalid_field");
}

fn first_role(value: &Value) -> &Value {
    value["roles"]
        .as_array()
        .and_then(|roles| roles.first())
        .expect("value should contain a first role")
}

fn role_by_id<'a>(roles: &'a Value, role_id: &str) -> &'a Value {
    roles
        .as_array()
        .and_then(|roles| {
            roles
                .iter()
                .find(|role| role["id"] == role_id || role["roleId"] == role_id)
        })
        .unwrap_or_else(|| panic!("missing role {role_id}"))
}

fn property_ids(properties: &Value) -> Vec<String> {
    let mut property_ids = properties
        .as_array()
        .expect("properties should be an array")
        .iter()
        .map(|property| property["id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    property_ids.sort();
    property_ids
}

fn dynamic_tool_request(tool: &str, arguments: Value) -> DynamicToolCallRequest {
    parse_dynamic_tool_call_request(
        json!("dynamic-request-1"),
        "item/tool/call",
        Some(json!({
            "threadId": "thread_1",
            "turnId": "turn_1",
            "callId": "call_1",
            "namespace": "beryl",
            "tool": tool,
            "arguments": arguments
        })),
    )
    .unwrap()
    .unwrap()
}

fn rust_function_body<'a>(source: &'a str, function_signature: &str) -> &'a str {
    let signature_index = source
        .find(function_signature)
        .unwrap_or_else(|| panic!("missing function {function_signature}"));
    let after_signature = &source[signature_index..];
    let open_offset = after_signature
        .find('{')
        .unwrap_or_else(|| panic!("missing body for function {function_signature}"));
    let body_start = signature_index + open_offset;
    let mut depth = 0usize;
    for (offset, ch) in source[body_start..].char_indices() {
        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                return &source[body_start..=body_start + offset];
            }
        }
    }
    panic!("unterminated body for function {function_signature}");
}

fn theme_document_text(foreground: &str) -> String {
    ThemeDocument::new(
        None,
        Some("Tool Theme".to_string()),
        theme_definition(foreground),
    )
    .unwrap()
    .to_toml_string()
    .unwrap()
}

fn theme_definition(foreground: &str) -> beryl_app::ThemeDefinition {
    let mut settings = AppearanceSettings::default();
    settings.general_ui.foreground = foreground.to_string();
    settings.to_theme_definition().unwrap()
}

fn compact_theme_definition(foreground: &str) -> beryl_app::ThemeDefinition {
    ThemeDocument::from_toml_str(&format!(
        r##"
schema = 1

[[role]]
id = "app.window"
foreground = {{ value = "{foreground}" }}
"##
    ))
    .unwrap()
    .into_definition()
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-theme-settings-dynamic-tools-test-")
}
