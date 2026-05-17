#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::{
    env,
    ffi::OsString,
    fs,
    panic::{self, AssertUnwindSafe},
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

pub use beryl_app::{
    ActiveThemeProjection, AgentPreferences, AppearanceButtonSettings,
    AppearanceButtonStateSettings, AppearanceForegroundSettings, AppearanceInputSettings,
    AppearanceRoleSettings, AppearanceSettings, AppearanceSettingsStore,
    AppearanceStatusLineSettings, AppearanceSurfaceSettings, AppearanceTranscriptShellSettings,
    BUILT_IN_INSTALLED_THEME_ID, BerylThemeProperty, BerylThemeRole, ContextCompactionTimeoutError,
    GuiPreferences, GuiPreferencesStore, InstalledThemeId, NotificationPreferences,
    NotificationSoundPathError, OperationPreferences, StylePropertyId, StylePropertyKind,
    StylePropertySource, StylePropertyValue, StyleRoleId, ThemeDefinition, ThemeDocument,
    ThemeRepositorySnapshot, ThemeRepositoryStore, ThemeResolutionContext, ThemeResolver,
    ThemeRoleDefinition, ThemeRoleSchema, built_in_theme_schema,
    built_in_theme_supported_properties, normalize_developer_instructions_text,
    parse_context_compaction_timeout_seconds_text, parse_notification_sound_path_text,
    validate_notification_sound_path,
};
use gpui_settings_window::{
    SettingsFieldId, SettingsFieldKind, SettingsPageActionId, SettingsPageId,
    SettingsPageSplitItemId, SettingsRowActionId, SettingsRowDetailField, SettingsWindowModel,
};

#[allow(dead_code)]
#[path = "../src/shell/settings.rs"]
mod settings;

#[test]
fn settings_model_maps_theme_editor_split_and_selected_role_rows() {
    let mut state = settings_state(AppearanceSettings::default());
    let model = state.model();

    assert_eq!(model.sections().len(), 4);
    assert_eq!(model.selected_section_id().as_str(), "themes");
    assert_eq!(model.selected_page_id().as_str(), "themes");

    let active_theme = model
        .row(&SettingsFieldId::from("themes.active"))
        .expect("active theme row should exist");
    assert_eq!(
        active_theme.navigation_target_page_id(),
        Some(&SettingsPageId::from("themes.editor"))
    );
    assert_eq!(active_theme.actions().len(), 2);
    assert_eq!(
        active_theme.actions()[0].action_id(),
        &SettingsRowActionId::from("save")
    );
    assert!(!active_theme.actions()[0].is_enabled());
    assert_eq!(
        active_theme.actions()[1].action_id(),
        &SettingsRowActionId::from("save_as")
    );
    assert!(!active_theme.actions()[1].is_enabled());

    let themes = model
        .sections()
        .iter()
        .find(|section| section.section_id().as_str() == "themes")
        .expect("themes section should exist");
    assert_eq!(themes.root_page().title(), "Themes");
    assert!(
        themes
            .subpages()
            .iter()
            .any(|page| page.page_id().as_str() == "themes.editor")
    );

    let editor = model
        .page(&SettingsPageId::from("themes.editor"))
        .expect("theme editor page should exist");
    assert!(
        editor.local_split().is_none(),
        "theme editor split should be built only while the editor page is selected"
    );
    assert!(
        model
            .row(&theme_property_field_id(
                BerylThemeRole::AppWindow,
                BerylThemeProperty::Foreground,
            ))
            .is_none(),
        "unselected theme editor rows should not participate in ordinary page sync"
    );

    state.select_page(SettingsPageId::from("themes.editor"));
    let model = state.model();
    let editor = model
        .page(&SettingsPageId::from("themes.editor"))
        .expect("theme editor page should exist");
    let split = editor
        .local_split()
        .expect("theme editor should carry a role split");
    assert_eq!(split.items().len(), BerylThemeRole::ALL.len());
    assert_eq!(
        split.selected_item().map(|item| item.item_id().as_str()),
        Some(BerylThemeRole::AppWindow.id())
    );
    assert_eq!(
        model.selected_rows().len(),
        1 + built_in_theme_supported_properties(BerylThemeRole::AppWindow).len(),
        "Theme Editor detail rows should stay bounded to Save As plus selected-role properties"
    );
    assert!(
        model
            .row(&SettingsFieldId::from("general_ui.foreground"))
            .is_none(),
        "theme editor must not expose obsolete flat appearance rows"
    );

    let foreground_source = model
        .row(&theme_property_source_field_id(
            BerylThemeRole::AppWindow,
            BerylThemeProperty::Foreground,
        ))
        .expect("selected role foreground source row should exist");
    assert_eq!(foreground_source.kind(), SettingsFieldKind::Choice);
    assert_eq!(foreground_source.value(), "value");
    assert_eq!(foreground_source.choices().len(), 4);
    assert_eq!(foreground_source.subtext(), None);
    let foreground = foreground_source
        .detail_field()
        .expect("concrete foreground detail field should exist");
    assert_eq!(foreground.kind(), SettingsFieldKind::Color);
    assert_eq!(foreground.value(), "#e2e8f0");
    assert!(
        model
            .row(&theme_property_field_id(
                BerylThemeRole::AppWindow,
                BerylThemeProperty::Foreground,
            ))
            .is_none(),
        "concrete value editor should be nested inside the source row"
    );

    let general_font_weight = theme_property_detail_field(
        &model,
        BerylThemeRole::AppWindow,
        BerylThemeProperty::FontWeight,
    )
    .expect("selected role font weight detail field should exist");
    assert_eq!(general_font_weight.kind(), SettingsFieldKind::Number);
    assert_eq!(general_font_weight.value(), "400");

    let background = theme_property_detail_field(
        &model,
        BerylThemeRole::AppWindow,
        BerylThemeProperty::Background,
    )
    .expect("selected role background detail field should exist");
    assert_eq!(background.kind(), SettingsFieldKind::Color);
    assert_eq!(background.value(), "#020617");
}

#[test]
fn settings_theme_editor_role_selection_updates_property_rows_only() {
    let mut state = settings_state(AppearanceSettings::default());

    state.select_page(SettingsPageId::from("themes.editor"));
    state.select_theme_editor_role(SettingsPageSplitItemId::from(
        BerylThemeRole::CodePanelBody.id(),
    ));

    let model = state.model();
    assert_eq!(model.selected_section_id().as_str(), "themes");
    assert_eq!(model.selected_page_id().as_str(), "themes.editor");
    let editor = model.selected_page();
    let split = editor
        .local_split()
        .expect("theme editor should carry a role split");
    assert_eq!(
        split.selected_item().map(|item| item.item_id().as_str()),
        Some(BerylThemeRole::CodePanelBody.id())
    );
    assert!(
        model
            .row(&theme_property_field_id(
                BerylThemeRole::AppWindow,
                BerylThemeProperty::Foreground,
            ))
            .is_none(),
        "unselected role rows should not remain in the detail pane"
    );
    let background = theme_property_detail_field(
        &model,
        BerylThemeRole::CodePanelBody,
        BerylThemeProperty::Background,
    )
    .expect("selected code-panel role background detail field should exist");
    assert_eq!(background.kind(), SettingsFieldKind::Color);
    assert_eq!(background.value(), "#0f172a");
    let font_source = model
        .row(&theme_property_source_field_id(
            BerylThemeRole::CodePanelBody,
            BerylThemeProperty::FontFamily,
        ))
        .expect("selected code-panel role font source row should exist");
    assert_eq!(font_source.value(), "value");
    assert_eq!(
        model.selected_rows().len(),
        1 + built_in_theme_supported_properties(BerylThemeRole::CodePanelBody).len()
    );
    assert!(
        model
            .row(&theme_property_source_field_id(
                BerylThemeRole::CodePanelBody,
                BerylThemeProperty::Border,
            ))
            .is_none(),
        "theme editor must not expose unsupported code-panel body border"
    );
}

#[test]
fn settings_theme_editor_exposes_only_color_for_single_color_roles() {
    let mut state = settings_state(AppearanceSettings::default());

    state.select_page(SettingsPageId::from("themes.editor"));
    state.select_theme_editor_role(SettingsPageSplitItemId::from(
        BerylThemeRole::MarkdownThematicBreak.id(),
    ));

    let model = state.model();
    assert_eq!(
        model.selected_rows().len(),
        1 + built_in_theme_supported_properties(BerylThemeRole::MarkdownThematicBreak).len()
    );
    let color = theme_property_detail_field(
        &model,
        BerylThemeRole::MarkdownThematicBreak,
        BerylThemeProperty::Color,
    )
    .expect("single-color role color detail field should exist");
    assert_eq!(color.kind(), SettingsFieldKind::Color);
    assert!(
        model
            .row(&theme_property_source_field_id(
                BerylThemeRole::MarkdownThematicBreak,
                BerylThemeProperty::Border,
            ))
            .is_none(),
        "single-color role must not expose border"
    );
    assert!(
        model
            .row(&theme_property_source_field_id(
                BerylThemeRole::MarkdownThematicBreak,
                BerylThemeProperty::Foreground,
            ))
            .is_none(),
        "single-color role must not expose foreground"
    );
}

#[test]
fn settings_theme_editor_no_property_roles_have_no_property_rows() {
    let mut state = settings_state(AppearanceSettings::default());

    state.select_page(SettingsPageId::from("themes.editor"));
    state.select_theme_editor_role(SettingsPageSplitItemId::from(
        BerylThemeRole::PopupRowNormal.id(),
    ));

    let model = state.model();
    assert_eq!(
        built_in_theme_supported_properties(BerylThemeRole::PopupRowNormal),
        &[]
    );
    assert_eq!(
        model.selected_rows().len(),
        1,
        "no-property roles should expose only the Save As name row"
    );
    assert!(
        model
            .row(&theme_property_source_field_id(
                BerylThemeRole::PopupRowNormal,
                BerylThemeProperty::Background,
            ))
            .is_none()
    );
}

#[test]
fn settings_theme_editor_role_previews_update_from_draft_values() {
    let mut state = settings_state(AppearanceSettings::default());
    let field_id = theme_property_field_id(
        BerylThemeRole::CodePanelBody,
        BerylThemeProperty::Foreground,
    );

    state.select_page(SettingsPageId::from("themes.editor"));
    state.select_theme_editor_role(SettingsPageSplitItemId::from(
        BerylThemeRole::CodePanelBody.id(),
    ));
    state.set_field_value(
        &theme_property_source_field_id(
            BerylThemeRole::CodePanelBody,
            BerylThemeProperty::Foreground,
        ),
        "value".to_string(),
    );
    state.set_field_value(&field_id, "#123456".to_string());

    let model = state.model();
    let split = model
        .selected_page()
        .local_split()
        .expect("theme editor should carry a role split");
    let code_item = split
        .items()
        .iter()
        .find(|item| item.item_id().as_str() == BerylThemeRole::CodePanelBody.id())
        .expect("code-panel body role should be listed");
    assert_eq!(
        code_item
            .preview_style()
            .and_then(|style| style.foreground())
            .map(|color| color.to_hex()),
        Some("#123456".to_string())
    );
    let source_row = model
        .row(&theme_property_source_field_id(
            BerylThemeRole::CodePanelBody,
            BerylThemeProperty::Foreground,
        ))
        .expect("foreground property row should exist");
    assert!(source_row.is_modified());
    assert!(
        source_row
            .detail_field()
            .is_some_and(SettingsRowDetailField::is_modified)
    );
}

#[test]
fn settings_theme_editor_role_previews_show_single_color_roles() {
    let mut state = settings_state(AppearanceSettings::default());
    let field_id = theme_property_field_id(
        BerylThemeRole::MarkdownThematicBreak,
        BerylThemeProperty::Color,
    );

    state.select_page(SettingsPageId::from("themes.editor"));
    state.select_theme_editor_role(SettingsPageSplitItemId::from(
        BerylThemeRole::MarkdownThematicBreak.id(),
    ));
    state.set_field_value(
        &theme_property_source_field_id(
            BerylThemeRole::MarkdownThematicBreak,
            BerylThemeProperty::Color,
        ),
        "value".to_string(),
    );
    state.set_field_value(&field_id, "#abcdef".to_string());

    let model = state.model();
    let split = model
        .selected_page()
        .local_split()
        .expect("theme editor should carry a role split");
    let item = split
        .items()
        .iter()
        .find(|item| item.item_id().as_str() == BerylThemeRole::MarkdownThematicBreak.id())
        .expect("thematic-break role should be listed");
    assert_eq!(
        item.preview_style()
            .and_then(|style| style.border())
            .map(|color| color.to_hex()),
        Some("#abcdef".to_string())
    );
}

#[test]
fn settings_theme_editor_property_source_changes_roundtrip_without_concretizing() {
    let (mut state, _shared, _preferences, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let source_field_id = theme_property_source_field_id(
        BerylThemeRole::MarkdownInlineCode,
        BerylThemeProperty::TextBackground,
    );
    let value_field_id = theme_property_field_id(
        BerylThemeRole::MarkdownInlineCode,
        BerylThemeProperty::TextBackground,
    );

    state.select_page(SettingsPageId::from("themes.editor"));
    state.select_theme_editor_role(SettingsPageSplitItemId::from(
        BerylThemeRole::MarkdownInlineCode.id(),
    ));
    let model = state.model();
    let source = model
        .row(&source_field_id)
        .expect("inline-code text-background source row should exist");
    assert_eq!(source.kind(), SettingsFieldKind::Choice);
    assert_eq!(source.value(), "ambient_parent");
    assert_eq!(source.subtext(), None);
    assert!(
        model.row(&value_field_id).is_none(),
        "ambient source should not expose a concrete value editor"
    );

    state.set_field_value(&source_field_id, "static_parent".to_string());
    assert_eq!(
        state.handle_row_action(
            &SettingsFieldId::from("themes.active"),
            &SettingsRowActionId::from("save"),
        ),
        Some(settings::SettingsRowActionOutcome::ActiveThemeChanged)
    );

    let snapshot = ThemeRepositoryStore::new(&root).load_or_default().unwrap();
    let role = snapshot
        .active_definition()
        .roles()
        .iter()
        .find(|role| role.role_id().as_str() == BerylThemeRole::MarkdownInlineCode.id())
        .expect("inline-code role should persist");
    assert_eq!(
        role.properties()
            .get(&StylePropertyId::from(BerylThemeProperty::Background.id())),
        None
    );
    assert_eq!(
        role.properties().get(&StylePropertyId::from(
            BerylThemeProperty::TextBackground.id()
        )),
        Some(&StylePropertySource::StaticParent)
    );
    cleanup_temp_dir(root);
}

#[test]
fn settings_theme_editor_save_preserves_compact_document_omissions() {
    let (mut state, _shared, _preferences, root) =
        settings_state_with_compact_theme_document(COMPACT_THEME_DOCUMENT);
    let field_id =
        theme_property_field_id(BerylThemeRole::AppWindow, BerylThemeProperty::Foreground);

    state.select_page(SettingsPageId::from("themes.editor"));
    state.set_field_value(&field_id, "#445566".to_string());

    assert_eq!(
        state.handle_row_action(
            &SettingsFieldId::from("themes.active"),
            &SettingsRowActionId::from("save"),
        ),
        Some(settings::SettingsRowActionOutcome::ActiveThemeChanged)
    );

    let store = ThemeRepositoryStore::new(&root);
    let text =
        fs::read_to_string(store.theme_document_path(&InstalledThemeId::from("compact"))).unwrap();
    let document = ThemeDocument::from_toml_str(&text).unwrap();

    assert_compact_theme_sources(document.definition(), "#445566");
    assert!(
        !role_record_text(&text, BerylThemeRole::AppWindow.id()).contains("background ="),
        "saving an unrelated property must not serialize omitted app-window background"
    );
    cleanup_temp_dir(root);
}

#[test]
fn settings_theme_editor_save_as_preserves_compact_document_omissions() {
    let (mut state, _shared, _preferences, root) =
        settings_state_with_compact_theme_document(COMPACT_THEME_DOCUMENT);
    let field_id =
        theme_property_field_id(BerylThemeRole::AppWindow, BerylThemeProperty::Foreground);

    state.select_page(SettingsPageId::from("themes.editor"));
    state.set_field_value(&field_id, "#556677".to_string());
    state.set_field_value(
        &SettingsFieldId::from("themes.save_as_name"),
        "Compact Copy".to_string(),
    );

    assert_eq!(
        state.handle_row_action(
            &SettingsFieldId::from("themes.active"),
            &SettingsRowActionId::from("save_as"),
        ),
        Some(settings::SettingsRowActionOutcome::ActiveThemeChanged)
    );

    let store = ThemeRepositoryStore::new(&root);
    let snapshot = store.load_or_default().unwrap();
    let text = fs::read_to_string(store.theme_document_path(snapshot.active_theme_id())).unwrap();
    let document = ThemeDocument::from_toml_str(&text).unwrap();

    assert_eq!(snapshot.active_theme_id().as_str(), "compact-copy");
    assert_compact_theme_sources(document.definition(), "#556677");
    assert!(
        !role_record_text(&text, BerylThemeRole::AppWindow.id()).contains("background ="),
        "Save As must not expand omitted properties into explicit fallback sources"
    );
    cleanup_temp_dir(root);
}

#[test]
fn settings_theme_editor_save_omits_stale_unsupported_loaded_properties() {
    let (mut state, _shared, _preferences, root) =
        settings_state_with_compact_theme_document(COMPACT_THEME_WITH_STALE_UNSUPPORTED_DOCUMENT);
    let field_id = theme_property_field_id(
        BerylThemeRole::CodePanelBody,
        BerylThemeProperty::Foreground,
    );

    state.select_page(SettingsPageId::from("themes.editor"));
    state.select_theme_editor_role(SettingsPageSplitItemId::from(
        BerylThemeRole::CodePanelBody.id(),
    ));
    state.set_field_value(&field_id, "#778899".to_string());

    assert_eq!(
        state.handle_row_action(
            &SettingsFieldId::from("themes.active"),
            &SettingsRowActionId::from("save"),
        ),
        Some(settings::SettingsRowActionOutcome::ActiveThemeChanged)
    );

    let store = ThemeRepositoryStore::new(&root);
    let text =
        fs::read_to_string(store.theme_document_path(&InstalledThemeId::from("compact"))).unwrap();
    let document = ThemeDocument::from_toml_str(&text).unwrap();

    assert_eq!(
        theme_source(
            document.definition(),
            BerylThemeRole::CodePanelBody,
            BerylThemeProperty::Foreground,
        ),
        Some(&StylePropertySource::Concrete(StylePropertyValue::color(
            "#778899"
        )))
    );
    assert!(
        !text.contains("border ="),
        "editor save must not reserialize stale unsupported border properties ignored on load"
    );
    assert!(
        !role_record_text(&text, BerylThemeRole::MarkdownInlineCode.id())
            .contains("\nbackground ="),
        "editor save must not reserialize stale unsupported inline-code background"
    );
    cleanup_temp_dir(root);
}

#[test]
fn settings_theme_editor_selected_fallback_on_omitted_property_becomes_explicit() {
    let (mut state, _shared, _preferences, root) =
        settings_state_with_compact_theme_document(COMPACT_THEME_DOCUMENT);
    let source_field_id =
        theme_property_source_field_id(BerylThemeRole::AppWindow, BerylThemeProperty::Background);

    state.select_page(SettingsPageId::from("themes.editor"));
    let model = state.model();
    assert_eq!(
        model
            .row(&source_field_id)
            .expect("background source row should exist")
            .value(),
        "fallback",
        "omitted properties still display the fallback source choice"
    );

    state.set_field_value(&source_field_id, "fallback".to_string());
    assert_eq!(
        state.handle_row_action(
            &SettingsFieldId::from("themes.active"),
            &SettingsRowActionId::from("save"),
        ),
        Some(settings::SettingsRowActionOutcome::ActiveThemeChanged)
    );

    let store = ThemeRepositoryStore::new(&root);
    let text =
        fs::read_to_string(store.theme_document_path(&InstalledThemeId::from("compact"))).unwrap();
    let document = ThemeDocument::from_toml_str(&text).unwrap();

    assert_eq!(
        theme_source(
            document.definition(),
            BerylThemeRole::AppWindow,
            BerylThemeProperty::Background,
        ),
        Some(&StylePropertySource::Fallback)
    );
    assert!(
        role_record_text(&text, BerylThemeRole::AppWindow.id())
            .contains("background = \"fallback\"")
    );
    cleanup_temp_dir(root);
}

#[test]
fn settings_theme_editor_concrete_source_uses_typed_value_editor() {
    let (mut state, shared, _preferences, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let source_field_id = theme_property_source_field_id(
        BerylThemeRole::MarkdownInlineCode,
        BerylThemeProperty::TextBackground,
    );
    let value_field_id = theme_property_field_id(
        BerylThemeRole::MarkdownInlineCode,
        BerylThemeProperty::TextBackground,
    );

    state.select_page(SettingsPageId::from("themes.editor"));
    state.select_theme_editor_role(SettingsPageSplitItemId::from(
        BerylThemeRole::MarkdownInlineCode.id(),
    ));
    state.set_field_value(&source_field_id, "value".to_string());
    let model = state.model();
    let value = model
        .row(&source_field_id)
        .and_then(|row| row.detail_field())
        .expect("concrete source should expose a nested concrete value editor");
    assert_eq!(value.kind(), SettingsFieldKind::Color);
    state.set_field_value(&value_field_id, "#445566".to_string());

    assert_eq!(
        state.handle_row_action(
            &SettingsFieldId::from("themes.active"),
            &SettingsRowActionId::from("save"),
        ),
        Some(settings::SettingsRowActionOutcome::ActiveThemeChanged)
    );
    assert_eq!(
        shared
            .lock()
            .unwrap()
            .resolve_property(
                BerylThemeRole::MarkdownInlineCode.id(),
                BerylThemeProperty::TextBackground.id(),
                &ThemeResolutionContext::new()
            )
            .unwrap(),
        StylePropertyValue::color("#445566")
    );

    let snapshot = ThemeRepositoryStore::new(&root).load_or_default().unwrap();
    let role = snapshot
        .active_definition()
        .roles()
        .iter()
        .find(|role| role.role_id().as_str() == BerylThemeRole::MarkdownInlineCode.id())
        .expect("inline-code role should persist");
    assert_eq!(
        role.properties().get(&StylePropertyId::from(
            BerylThemeProperty::TextBackground.id()
        )),
        Some(&StylePropertySource::Concrete(StylePropertyValue::color(
            "#445566"
        )))
    );
    cleanup_temp_dir(root);
}

#[test]
fn settings_theme_editor_static_parent_is_schema_metadata_not_text_field() {
    let mut state = settings_state(AppearanceSettings::default());

    state.select_page(SettingsPageId::from("themes.editor"));
    state.select_theme_editor_role(SettingsPageSplitItemId::from(
        BerylThemeRole::CodePanelBody.id(),
    ));
    let model = state.model();
    let editor = model.selected_page();
    let split = editor
        .local_split()
        .expect("theme editor should carry a role split");
    let item = split
        .items()
        .iter()
        .find(|item| item.item_id().as_str() == BerylThemeRole::CodePanelBody.id())
        .expect("code panel body role should exist");

    assert_eq!(item.subtext(), Some("static parent: code_panel.container"));
    assert!(
        model
            .selected_rows()
            .iter()
            .all(|row| row.label() != "Static parent"),
        "Theme Editor must not expose free-form static-parent editing"
    );
}

#[test]
fn settings_model_includes_notifications_sound_picker_row() {
    let state = settings_state(AppearanceSettings::default());
    let model = state.model();
    let section = model
        .sections()
        .iter()
        .find(|section| section.section_id().as_str() == "notifications")
        .expect("notifications section should exist");

    assert_eq!(section.label(), "Notifications");
    assert_eq!(section.rows().len(), 1);

    let field_id = state.notification_end_turn_sound_field_id();
    let row = model
        .row(&field_id)
        .expect("end-turn sound row should exist");
    assert_eq!(row.label(), "End-turn sound");
    assert_eq!(row.kind(), SettingsFieldKind::Text);
    assert_eq!(row.value(), "");
    assert_eq!(row.actions().len(), 2);
    assert_eq!(
        row.actions()[0].action_id(),
        &SettingsRowActionId::from("choose")
    );
    assert_eq!(row.actions()[0].label(), "Choose...");
    assert_eq!(
        row.actions()[1].action_id(),
        &SettingsRowActionId::from("clear")
    );
    assert_eq!(row.actions()[1].label(), "Clear");
}

#[test]
fn settings_model_includes_agent_developer_instructions_row() {
    let state = settings_state(AppearanceSettings::default());
    let model = state.model();
    let section = model
        .sections()
        .iter()
        .find(|section| section.section_id().as_str() == "agent")
        .expect("agent section should exist");

    assert_eq!(section.label(), "Agent");
    assert_eq!(section.rows().len(), 1);

    let field_id = state.developer_instructions_field_id();
    let row = model
        .row(&field_id)
        .expect("developer instructions row should exist");
    assert_eq!(row.label(), "Developer Instructions");
    assert_eq!(
        row.subtext(),
        Some("Sent as developer instructions with every user message.")
    );
    assert_eq!(row.kind(), SettingsFieldKind::MultilineText);
    assert_eq!(row.value(), "");
    assert!(row.actions().is_empty());
}

#[test]
fn settings_model_includes_operations_context_compaction_timeout_row() {
    let state = settings_state(AppearanceSettings::default());
    let model = state.model();
    let section = model
        .sections()
        .iter()
        .find(|section| section.section_id().as_str() == "operations")
        .expect("operations section should exist");

    assert_eq!(section.label(), "Operations");
    assert_eq!(section.rows().len(), 1);

    let field_id = context_compaction_timeout_field_id();
    let row = model
        .row(&field_id)
        .expect("context compaction timeout row should exist");
    assert_eq!(row.label(), "Context compaction timeout");
    assert_eq!(
        row.subtext(),
        Some("Seconds Beryl waits for backend-reported compaction completion.")
    );
    assert_eq!(row.kind(), SettingsFieldKind::Number);
    assert_eq!(row.value(), "180");
    assert!(row.actions().is_empty());
}

#[test]
fn settings_window_options_map_active_theme_to_visual_theme() {
    let mut active = AppearanceSettings::default();
    active.general_ui.background = "#101112".to_string();
    active.general_ui.foreground = "#edeff1".to_string();
    active.chrome.surfaces.panel_background = "#202122".to_string();
    active.chrome.surfaces.row_background = "#505152".to_string();
    active.chrome.surfaces.popup_background = "#606162".to_string();
    active.chrome.surfaces.border = "#303132".to_string();
    active.chrome.surfaces.muted_foreground = "#707172".to_string();
    active.chrome.input.input_background = "#808182".to_string();
    active.chrome.input.input_border = "#909192".to_string();
    active.chrome.input.input_foreground = "#a0a1a2".to_string();
    active.chrome.primary_button.font_weight = 650;
    active.chrome.primary_button.normal.background = "#404142".to_string();
    active.chrome.secondary_button.font_weight = 550;
    let mut state = settings_state(active);

    let theme = state.window_options().visual_theme().clone();

    assert_eq!(theme.window_background.to_hex(), "#101112");
    assert_eq!(theme.panel.background.to_hex(), "#202122");
    assert_eq!(theme.panel.foreground.to_hex(), "#edeff1");
    assert_eq!(theme.panel.muted_foreground.to_hex(), "#707172");
    assert_eq!(theme.row.background.to_hex(), "#505152");
    assert_eq!(theme.popup.background.to_hex(), "#606162");
    assert_eq!(theme.input.background.to_hex(), "#808182");
    assert_eq!(theme.input.border.to_hex(), "#909192");
    assert_eq!(theme.input.foreground.to_hex(), "#a0a1a2");
    assert_eq!(theme.navigation_button.font_weight, 550);
    assert_eq!(theme.primary_button.font_weight, 650);
    assert_eq!(theme.primary_button.normal.background.to_hex(), "#404142");
    assert_eq!(theme.secondary_button.font_weight, 550);
}

#[test]
fn settings_window_options_use_minimal_reusable_crate_layout_size() {
    let mut state = settings_state(AppearanceSettings::default());
    let options = state.window_options();
    let (width, height) = options.window_size();
    let (min_width, min_height) = options.min_window_size();

    assert_eq!((width, height), (800.0, 520.0));
    assert_eq!((min_width, min_height), (800.0, 520.0));
}

#[test]
fn settings_window_options_sync_skips_ordinary_theme_editor_field_edits() {
    let mut state = settings_state(AppearanceSettings::default());
    let initial = state
        .window_options_for_sync()
        .expect("first options sync should publish options");
    state.record_window_options_synced(initial);

    state.select_page(SettingsPageId::from("themes.editor"));
    state.select_theme_editor_role(SettingsPageSplitItemId::from(
        BerylThemeRole::AppWindow.id(),
    ));
    state.set_field_value(
        &theme_property_source_field_id(BerylThemeRole::AppWindow, BerylThemeProperty::Background),
        "value".to_string(),
    );
    state.set_field_value(
        &theme_property_field_id(BerylThemeRole::AppWindow, BerylThemeProperty::Background),
        "#101112".to_string(),
    );

    let model = state.model();
    let active_row = model
        .row(&SettingsFieldId::from("themes.active"))
        .expect("active theme row should exist");
    assert!(active_row.is_modified());
    assert!(
        state.window_options_for_sync().is_none(),
        "staged field edits must sync the model without resyncing unchanged window options"
    );
}

#[test]
fn settings_window_options_sync_invalidates_once_for_active_theme_preview_and_save() {
    let (mut state, shared, _preferences, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let initial = state
        .window_options_for_sync()
        .expect("first options sync should publish options");
    state.record_window_options_synced(initial.clone());

    let mut preview = AppearanceSettings::default();
    preview.general_ui.background = "#101112".to_string();
    *shared.lock().unwrap() = preview.to_active_theme_projection().unwrap();

    let preview_options = state
        .window_options_for_sync()
        .expect("theme preview should publish changed visual options");
    assert_ne!(preview_options, initial);
    state.record_window_options_synced(preview_options.clone());
    assert!(
        state.window_options_for_sync().is_none(),
        "unchanged preview options should not publish twice"
    );

    *shared.lock().unwrap() = state
        .theme_repository_snapshot()
        .active_projection()
        .clone();
    let restored_options = state
        .window_options_for_sync()
        .expect("stopping preview should restore visual options once");
    assert_eq!(restored_options, initial);
    state.record_window_options_synced(restored_options);
    assert!(
        state.window_options_for_sync().is_none(),
        "restored options should not publish twice"
    );

    state.select_page(SettingsPageId::from("themes.editor"));
    state.set_field_value(
        &theme_property_source_field_id(BerylThemeRole::AppWindow, BerylThemeProperty::Background),
        "value".to_string(),
    );
    state.set_field_value(
        &theme_property_field_id(BerylThemeRole::AppWindow, BerylThemeProperty::Background),
        "#202122".to_string(),
    );
    assert_eq!(
        state.handle_row_action(
            &SettingsFieldId::from("themes.active"),
            &SettingsRowActionId::from("save"),
        ),
        Some(settings::SettingsRowActionOutcome::ActiveThemeChanged)
    );
    let saved_options = state
        .window_options_for_sync()
        .expect("saving active theme changes should publish options once");
    assert_ne!(saved_options, initial);
    state.record_window_options_synced(saved_options);
    assert!(
        state.window_options_for_sync().is_none(),
        "saved options should not publish twice"
    );
    cleanup_temp_dir(root);
}

#[test]
fn settings_model_exposes_clipping_sensitive_controls() {
    let mut state = settings_state(AppearanceSettings::default());

    state.select_page(SettingsPageId::from("themes.editor"));
    let model = state.model();
    let page = model.selected_page();
    assert_eq!(page.page_id().as_str(), "themes.editor");
    assert_eq!(page.actions().len(), 2);
    assert_eq!(
        page.actions()[1].action_id(),
        &SettingsPageActionId::from("save_as")
    );
    assert_eq!(page.actions()[1].label(), "Save As");
    assert!(
        model
            .row(&SettingsFieldId::from("themes.save_as_name"))
            .is_some(),
        "Theme Editor should expose the Save As name row"
    );

    let notification_row = model
        .row(&state.notification_end_turn_sound_field_id())
        .expect("notification sound row should exist");
    assert_eq!(notification_row.actions()[1].label(), "Clear");

    let developer_row = model
        .row(&state.developer_instructions_field_id())
        .expect("developer instructions row should exist");
    assert_eq!(developer_row.kind(), SettingsFieldKind::MultilineText);
}

#[test]
fn initial_active_theme_uses_built_in_theme_and_ignores_legacy_theme_toml() {
    let root = unique_temp_dir();
    let legacy_store = AppearanceSettingsStore::new(&root);
    fs::write(legacy_store.theme_path(), b"legacy theme should remain").unwrap();

    let active = settings::load_initial_theme_repository_snapshot(None)
        .active_projection()
        .clone();
    let expected = ActiveThemeProjection::built_in();

    assert_eq!(
        active
            .default_style(beryl_app::BerylThemeRole::AppWindow.id())
            .unwrap(),
        expected
            .default_style(beryl_app::BerylThemeRole::AppWindow.id())
            .unwrap()
    );
    assert_eq!(
        fs::read(legacy_store.theme_path()).unwrap(),
        b"legacy theme should remain"
    );
    cleanup_temp_dir(root);
}

#[test]
fn settings_theme_save_stages_color_changes_and_normalizes_on_save() {
    let mut active = AppearanceSettings::default();
    active.code.foreground = "#112233".to_string();
    let (mut state, shared, _notifications, root) = settings_state_with_temp_store(active);
    let field_id = theme_property_field_id(
        BerylThemeRole::CodePanelBody,
        BerylThemeProperty::Foreground,
    );
    let commentary_field_id = theme_property_field_id(
        BerylThemeRole::TranscriptAssistantCommentary,
        BerylThemeProperty::Foreground,
    );
    let thread_strip_field_id = theme_property_field_id(
        BerylThemeRole::MainThreadStrip,
        BerylThemeProperty::Background,
    );
    let primary_button_weight_field_id = theme_property_field_id(
        BerylThemeRole::ButtonPrimaryNormal,
        BerylThemeProperty::FontWeight,
    );

    state.select_theme_editor_role(SettingsPageSplitItemId::from(
        BerylThemeRole::CodePanelBody.id(),
    ));
    state.set_field_value(
        &theme_property_source_field_id(
            BerylThemeRole::CodePanelBody,
            BerylThemeProperty::Foreground,
        ),
        "value".to_string(),
    );
    state.set_field_value(&field_id, "#AABBCC".to_string());
    state.set_field_value(
        &theme_property_source_field_id(
            BerylThemeRole::TranscriptAssistantCommentary,
            BerylThemeProperty::Foreground,
        ),
        "value".to_string(),
    );
    state.set_field_value(&commentary_field_id, "#334455".to_string());
    state.set_field_value(
        &theme_property_source_field_id(
            BerylThemeRole::MainThreadStrip,
            BerylThemeProperty::Background,
        ),
        "value".to_string(),
    );
    state.set_field_value(&thread_strip_field_id, "#010203".to_string());
    state.set_field_value(
        &theme_property_source_field_id(
            BerylThemeRole::ButtonPrimaryNormal,
            BerylThemeProperty::FontWeight,
        ),
        "value".to_string(),
    );
    state.set_field_value(&primary_button_weight_field_id, "650".to_string());
    assert_eq!(
        active_settings(&shared).code.foreground,
        "#112233",
        "field edits must not live-preview into active settings"
    );
    assert_eq!(
        theme_property_detail_field(
            &state.model(),
            BerylThemeRole::CodePanelBody,
            BerylThemeProperty::Foreground,
        )
        .map(|field| field.value()),
        Some("#AABBCC")
    );
    let model = state.model();
    let active_row = model
        .row(&SettingsFieldId::from("themes.active"))
        .expect("active theme row should exist");
    assert!(active_row.is_modified());
    assert_eq!(
        active_row.actions()[0].action_id(),
        &SettingsRowActionId::from("save")
    );
    assert!(active_row.actions()[0].is_enabled());
    assert_eq!(
        active_row.actions()[1].action_id(),
        &SettingsRowActionId::from("save_as")
    );
    assert!(active_row.actions()[1].is_enabled());

    assert_eq!(
        state.handle_row_action(
            &SettingsFieldId::from("themes.active"),
            &SettingsRowActionId::from("save"),
        ),
        Some(settings::SettingsRowActionOutcome::ActiveThemeChanged)
    );
    assert_eq!(active_settings(&shared).code.foreground, "#aabbcc");
    assert_eq!(
        active_settings(&shared).transcript_commentary.foreground,
        "#334455"
    );
    assert_eq!(
        active_settings(&shared)
            .chrome
            .conversation_thread_strip_background,
        "#010203"
    );
    assert_eq!(
        active_settings(&shared).chrome.primary_button.font_weight,
        650
    );

    assert!(!AppearanceSettingsStore::new(&root).theme_path().exists());
    assert!(ThemeRepositoryStore::new(&root).manifest_path().exists());
    cleanup_temp_dir(root);
}

#[test]
fn settings_apply_persists_notification_preferences_separately_from_theme() {
    let (mut state, _appearance, notifications, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let sound_path = root.join("turn-done.wav");

    state.set_notification_end_turn_sound_path(sound_path.display().to_string());
    assert_eq!(
        notifications
            .lock()
            .unwrap()
            .notifications
            .end_turn_sound_path,
        None,
        "notification edits must not live-preview into active preferences"
    );

    assert!(state.apply());
    assert_eq!(
        notifications
            .lock()
            .unwrap()
            .notifications
            .end_turn_sound_path
            .as_deref(),
        Some(sound_path.as_path())
    );
    wait_for_save(&mut state);

    let loaded_preferences = GuiPreferencesStore::new(&root).load_or_default().unwrap();
    assert_eq!(
        loaded_preferences
            .notifications
            .end_turn_sound_path
            .as_deref(),
        Some(sound_path.as_path())
    );
    assert!(!AppearanceSettingsStore::new(&root).theme_path().exists());
    assert!(GuiPreferencesStore::new(&root).preferences_path().exists());
    cleanup_temp_dir(root);
}

#[test]
fn settings_apply_persists_operation_preferences() {
    let (mut state, _appearance, preferences, root) =
        settings_state_with_temp_store(AppearanceSettings::default());

    state.set_field_value(&context_compaction_timeout_field_id(), "240".to_string());
    assert_eq!(
        preferences
            .lock()
            .unwrap()
            .operations
            .context_compaction_timeout_seconds,
        180,
        "operation edits must not live-preview into active preferences"
    );

    assert!(state.apply());
    assert_eq!(
        preferences
            .lock()
            .unwrap()
            .operations
            .context_compaction_timeout_seconds,
        240
    );
    wait_for_save(&mut state);

    let loaded_preferences = GuiPreferencesStore::new(&root).load_or_default().unwrap();
    assert_eq!(
        loaded_preferences
            .operations
            .context_compaction_timeout_seconds,
        240
    );
    cleanup_temp_dir(root);
}

#[test]
fn settings_notification_row_actions_choose_and_clear() {
    let (mut state, _appearance, _notifications, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let field_id = state.notification_end_turn_sound_field_id();
    let sound_path = root.join("turn-done.wav");

    assert_eq!(
        state.handle_row_action(&field_id, &SettingsRowActionId::from("choose")),
        Some(settings::SettingsRowActionOutcome::PromptForEndTurnSoundPath)
    );

    state.set_notification_end_turn_sound_path(sound_path.display().to_string());
    let sound_path_text = sound_path.display().to_string();
    assert_eq!(
        state.model().row(&field_id).map(|row| row.value()),
        Some(sound_path_text.as_str())
    );

    assert_eq!(
        state.handle_row_action(&field_id, &SettingsRowActionId::from("clear")),
        Some(settings::SettingsRowActionOutcome::Updated)
    );
    assert_eq!(state.notification_end_turn_sound_path_value(), "");
    assert_eq!(
        state.handle_row_action(&field_id, &SettingsRowActionId::from("missing")),
        None
    );
    cleanup_temp_dir(root);
}

#[test]
fn settings_stage_notification_picker_path_validates_wav_extension() {
    let (mut state, _appearance, _notifications, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let field_id = state.notification_end_turn_sound_field_id();
    let sound_path = root.join("turn-done.WAV");
    let text_path = root.join("turn-done.txt");

    state.stage_notification_end_turn_sound_path_from_picker(sound_path.clone());
    assert_eq!(
        state.notification_end_turn_sound_path_value(),
        sound_path.display().to_string()
    );
    assert!(state.field_error(&field_id).is_none());

    state.stage_notification_end_turn_sound_path_from_picker(text_path.clone());
    assert_eq!(
        state.notification_end_turn_sound_path_value(),
        text_path.display().to_string()
    );
    assert!(
        state
            .field_error(&field_id)
            .is_some_and(|error| error.contains(".wav"))
    );
    assert!(!state.apply());
    cleanup_temp_dir(root);
}

#[test]
fn settings_apply_persists_empty_notification_path_as_disabled() {
    let (mut state, _appearance, notifications, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let field_id = state.notification_end_turn_sound_field_id();
    let sound_path = root.join("turn-done.wav");

    state.set_notification_end_turn_sound_path(sound_path.display().to_string());
    assert_eq!(
        state.handle_row_action(&field_id, &SettingsRowActionId::from("clear")),
        Some(settings::SettingsRowActionOutcome::Updated)
    );

    assert!(state.apply());
    assert_eq!(
        notifications
            .lock()
            .unwrap()
            .notifications
            .end_turn_sound_path,
        None
    );
    wait_for_save(&mut state);

    let loaded_preferences = GuiPreferencesStore::new(&root).load_or_default().unwrap();
    assert_eq!(loaded_preferences.notifications.end_turn_sound_path, None);
    cleanup_temp_dir(root);
}

#[test]
fn settings_save_uses_injected_root_when_environment_home_differs() {
    let env_home = unique_temp_dir();
    let (mut state, _appearance, _preferences, injected_root) =
        with_environment_home(&env_home, || {
            settings_state_with_temp_store(AppearanceSettings::default())
        });

    state.set_field_value(
        &theme_property_field_id(
            BerylThemeRole::CodePanelBody,
            BerylThemeProperty::Foreground,
        ),
        "#010203".to_string(),
    );
    assert!(state.apply());
    wait_for_save(&mut state);

    assert!(
        !AppearanceSettingsStore::new(&injected_root)
            .theme_path()
            .exists()
    );
    assert!(
        GuiPreferencesStore::new(&injected_root)
            .preferences_path()
            .exists()
    );
    assert!(!env_home.join(".beryl").exists());

    cleanup_temp_dir(injected_root);
    cleanup_temp_dir(env_home);
}

#[test]
fn settings_apply_rejects_invalid_notification_path_without_mutating_active_preferences() {
    let (mut state, _appearance, notifications, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let field_id = state.notification_end_turn_sound_field_id();

    state.set_notification_end_turn_sound_path("relative/done.wav".to_string());

    assert!(!state.apply());
    assert_eq!(
        notifications
            .lock()
            .unwrap()
            .notifications
            .end_turn_sound_path,
        None
    );
    assert!(
        state
            .field_error(&field_id)
            .is_some_and(|error| error.contains("absolute"))
    );
    assert!(!GuiPreferencesStore::new(&root).preferences_path().exists());
    cleanup_temp_dir(root);
}

#[test]
fn settings_apply_rejects_invalid_operation_timeout_without_mutating_active_preferences() {
    let (mut state, _appearance, preferences, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let field_id = context_compaction_timeout_field_id();

    state.set_field_value(&field_id, "0".to_string());

    assert!(!state.apply());
    assert_eq!(
        preferences
            .lock()
            .unwrap()
            .operations
            .context_compaction_timeout_seconds,
        180
    );
    assert!(
        state
            .field_error(&field_id)
            .is_some_and(|error| error.contains("at least"))
    );
    assert!(!GuiPreferencesStore::new(&root).preferences_path().exists());
    cleanup_temp_dir(root);
}

#[test]
fn settings_theme_save_rejects_invalid_color_draft_without_mutating_active_settings() {
    let mut active = AppearanceSettings::default();
    active.emphasis.background = "#010203".to_string();
    let (mut state, shared, _notifications, root) = settings_state_with_temp_store(active);
    let field_id = theme_property_field_id(
        BerylThemeRole::MarkdownEmphasis,
        BerylThemeProperty::TextBackground,
    );

    state.select_theme_editor_role(SettingsPageSplitItemId::from(
        BerylThemeRole::MarkdownEmphasis.id(),
    ));
    state.set_field_value(
        &theme_property_source_field_id(
            BerylThemeRole::MarkdownEmphasis,
            BerylThemeProperty::TextBackground,
        ),
        "value".to_string(),
    );
    state.set_field_value(&field_id, "slate".to_string());

    assert_eq!(
        state.handle_row_action(
            &SettingsFieldId::from("themes.active"),
            &SettingsRowActionId::from("save"),
        ),
        Some(settings::SettingsRowActionOutcome::Updated)
    );
    assert_eq!(active_settings(&shared).emphasis.background, "#010203");
    assert!(
        theme_property_detail_field(
            &state.model(),
            BerylThemeRole::MarkdownEmphasis,
            BerylThemeProperty::TextBackground,
        )
        .and_then(|field| field.error())
        .is_some_and(|error| error.contains("#rrggbb"))
    );
    assert!(!AppearanceSettingsStore::new(&root).theme_path().exists());
    cleanup_temp_dir(root);
}

#[test]
fn settings_theme_modified_state_tracks_edits_apply_cancel_save_and_failed_validation() {
    let (mut state, shared, _preferences, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let active_row_id = SettingsFieldId::from("themes.active");
    let source_field_id =
        theme_property_source_field_id(BerylThemeRole::AppWindow, BerylThemeProperty::Background);
    let value_field_id =
        theme_property_field_id(BerylThemeRole::AppWindow, BerylThemeProperty::Background);

    let model = state.model();
    let active_row = model
        .row(&active_row_id)
        .expect("active theme row should exist");
    assert!(!active_row.is_modified());
    assert!(!active_row.actions()[0].is_enabled());

    state.select_page(SettingsPageId::from("themes.editor"));
    state.set_field_value(&source_field_id, "value".to_string());
    state.set_field_value(&value_field_id, "slate".to_string());
    let model = state.model();
    assert!(
        model
            .row(&active_row_id)
            .expect("active theme row should exist")
            .is_modified(),
        "field edits, including invalid concrete values, should mark the theme draft modified"
    );

    assert!(
        state.apply(),
        "preference apply should not validate theme edits"
    );
    wait_for_save(&mut state);
    assert!(
        state
            .model()
            .row(&active_row_id)
            .expect("active theme row should exist")
            .is_modified(),
        "preference apply must not clear staged theme edits"
    );

    assert_eq!(
        state.handle_row_action(&active_row_id, &SettingsRowActionId::from("save")),
        Some(settings::SettingsRowActionOutcome::Updated)
    );
    assert!(
        state
            .model()
            .row(&active_row_id)
            .expect("active theme row should exist")
            .is_modified(),
        "failed theme validation must keep the draft modified"
    );

    state.reset_draft_from_active();
    assert!(
        !state
            .model()
            .row(&active_row_id)
            .expect("active theme row should exist")
            .is_modified(),
        "cancel/reset should clear staged theme edits"
    );

    state.select_page(SettingsPageId::from("themes.editor"));
    state.set_field_value(&source_field_id, "value".to_string());
    state.set_field_value(&value_field_id, "#202122".to_string());
    assert_eq!(
        state.handle_row_action(&active_row_id, &SettingsRowActionId::from("save")),
        Some(settings::SettingsRowActionOutcome::ActiveThemeChanged)
    );
    assert_eq!(active_settings(&shared).general_ui.background, "#202122");
    assert!(
        !state
            .model()
            .row(&active_row_id)
            .expect("active theme row should exist")
            .is_modified(),
        "successful save should rebaseline the theme draft"
    );
    cleanup_temp_dir(root);
}

#[test]
fn settings_theme_save_as_and_activate_switch_installed_themes() {
    let (mut state, shared, _notifications, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let field_id = theme_property_field_id(
        BerylThemeRole::CodePanelBody,
        BerylThemeProperty::Foreground,
    );
    let save_as_name = SettingsFieldId::from("themes.save_as_name");

    state.select_page(SettingsPageId::from("themes.editor"));
    state.select_theme_editor_role(SettingsPageSplitItemId::from(
        BerylThemeRole::CodePanelBody.id(),
    ));
    state.set_field_value(
        &theme_property_source_field_id(
            BerylThemeRole::CodePanelBody,
            BerylThemeProperty::Foreground,
        ),
        "value".to_string(),
    );
    state.set_field_value(&field_id, "#223344".to_string());
    state.set_field_value(&save_as_name, "Alternate Theme".to_string());
    assert_eq!(
        state.handle_row_action(
            &SettingsFieldId::from("themes.active"),
            &SettingsRowActionId::from("save_as"),
        ),
        Some(settings::SettingsRowActionOutcome::ActiveThemeChanged)
    );
    assert_eq!(active_settings(&shared).code.foreground, "#223344");

    let model = state.model();
    let original = model
        .row(&SettingsFieldId::from("themes.installed.test-theme"))
        .expect("original installed theme row should exist");
    assert_eq!(original.label(), "Test Theme");
    assert_eq!(original.actions()[0].label(), "Activate");

    assert_eq!(
        state.handle_row_action(
            &SettingsFieldId::from("themes.installed.test-theme"),
            &SettingsRowActionId::from("activate"),
        ),
        Some(settings::SettingsRowActionOutcome::ActiveThemeChanged)
    );
    assert_eq!(active_settings(&shared).code.foreground, "#e2e8f0");
    assert!(
        !state
            .model()
            .row(&SettingsFieldId::from("themes.active"))
            .expect("active theme row should exist")
            .is_modified(),
        "activating another theme should leave no staged theme edits"
    );
    assert_eq!(
        ThemeRepositoryStore::new(&root)
            .load_or_default()
            .unwrap()
            .active_theme_id()
            .as_str(),
        "test-theme"
    );
    cleanup_temp_dir(root);
}

#[test]
fn settings_reset_discards_unapplied_draft_and_preserves_selected_section() {
    let (mut state, _shared, _notifications, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let field_id = theme_property_field_id(
        BerylThemeRole::CodePanelBody,
        BerylThemeProperty::FontFamily,
    );

    state.select_page(SettingsPageId::from("themes.editor"));
    state.select_theme_editor_role(SettingsPageSplitItemId::from(
        BerylThemeRole::CodePanelBody.id(),
    ));
    state.set_field_value(&field_id, "JetBrains Mono".to_string());
    state.set_notification_end_turn_sound_path(root.join("done.wav").display().to_string());
    state.set_developer_instructions("Use a staged draft.".to_string());
    let context_timeout_field_id = context_compaction_timeout_field_id();
    state.set_field_value(&context_timeout_field_id, "240".to_string());
    state.reset_draft_from_active();

    let model = state.model();
    assert_eq!(model.selected_section_id().as_str(), "themes");
    assert_eq!(model.selected_page_id().as_str(), "themes.editor");
    assert_eq!(
        theme_property_detail_field(
            &model,
            BerylThemeRole::CodePanelBody,
            BerylThemeProperty::FontFamily,
        )
        .map(|field| field.value()),
        Some("Consolas")
    );
    assert_eq!(state.notification_end_turn_sound_path_value(), "");
    assert_eq!(state.developer_instructions_value(), "");
    assert_eq!(
        model.row(&context_timeout_field_id).map(|row| row.value()),
        Some("180")
    );
    cleanup_temp_dir(root);
}

fn settings_state(settings_value: AppearanceSettings) -> settings::SettingsState {
    settings_state_with_temp_store(settings_value).0
}

const COMPACT_THEME_DOCUMENT: &str = r##"
schema = 1
id = "compact"
name = "Compact Theme"

[[role]]
id = "app.window"
foreground = { value = "#112233" }

[[role]]
id = "code_panel.body"
font_family = "fallback"

[[role]]
id = "markdown.inline_code"
foreground = "static_parent"
text_background = "ambient_parent"
"##;

const COMPACT_THEME_WITH_STALE_UNSUPPORTED_DOCUMENT: &str = r##"
schema = 1
id = "compact"
name = "Compact Theme"

[[role]]
id = "app.window"
foreground = { value = "#112233" }

[[role]]
id = "code_panel.body"
border = { value = "#334455" }

[[role]]
id = "markdown.inline_code"
background = "ambient_parent"
text_background = "ambient_parent"

[[role]]
id = "markdown.thematic_break"
border = { value = "#556677" }
"##;

fn settings_state_with_temp_store(
    settings_value: AppearanceSettings,
) -> (
    settings::SettingsState,
    Arc<Mutex<ActiveThemeProjection>>,
    Arc<Mutex<GuiPreferences>>,
    tempdir_support::TestTempDir,
) {
    let root = unique_temp_dir();
    let theme_store = ThemeRepositoryStore::new(&root);
    let theme_snapshot = theme_store
        .save_as_theme("Test Theme", settings_value.to_theme_definition().unwrap())
        .unwrap();
    let shared_theme = Arc::new(Mutex::new(theme_snapshot.active_projection().clone()));
    let shared_preferences = Arc::new(Mutex::new(GuiPreferences::default()));
    let state = settings::SettingsState::new_with_theme_repository(
        shared_theme.clone(),
        shared_preferences.clone(),
        GuiPreferencesStore::new(&root),
        theme_store,
        theme_snapshot,
    );
    (state, shared_theme, shared_preferences, root)
}

fn settings_state_with_compact_theme_document(
    document: &str,
) -> (
    settings::SettingsState,
    Arc<Mutex<ActiveThemeProjection>>,
    Arc<Mutex<GuiPreferences>>,
    tempdir_support::TestTempDir,
) {
    let root = unique_temp_dir();
    let theme_store = ThemeRepositoryStore::new(&root);
    fs::create_dir_all(theme_store.theme_documents_dir()).unwrap();
    fs::write(
        theme_store.manifest_path(),
        r#"schema = 1
active_theme_id = "compact"

[[theme]]
id = "compact"
name = "Compact Theme"
file = "compact.toml"
"#,
    )
    .unwrap();
    fs::write(
        theme_store.theme_document_path(&InstalledThemeId::from("compact")),
        document,
    )
    .unwrap();

    let theme_snapshot = theme_store.load_or_default().unwrap();
    let shared_theme = Arc::new(Mutex::new(theme_snapshot.active_projection().clone()));
    let shared_preferences = Arc::new(Mutex::new(GuiPreferences::default()));
    let state = settings::SettingsState::new_with_theme_repository(
        shared_theme.clone(),
        shared_preferences.clone(),
        GuiPreferencesStore::new(&root),
        theme_store,
        theme_snapshot,
    );
    (state, shared_theme, shared_preferences, root)
}

fn assert_compact_theme_sources(definition: &ThemeDefinition, foreground: &str) {
    assert_eq!(
        theme_source(
            definition,
            BerylThemeRole::AppWindow,
            BerylThemeProperty::Foreground,
        ),
        Some(&StylePropertySource::Concrete(StylePropertyValue::color(
            foreground
        )))
    );
    assert_eq!(
        theme_source(
            definition,
            BerylThemeRole::AppWindow,
            BerylThemeProperty::Background,
        ),
        None
    );
    assert_eq!(
        theme_source(
            definition,
            BerylThemeRole::CodePanelBody,
            BerylThemeProperty::FontFamily,
        ),
        Some(&StylePropertySource::Fallback)
    );
    assert_eq!(
        theme_source(
            definition,
            BerylThemeRole::MarkdownInlineCode,
            BerylThemeProperty::Foreground,
        ),
        Some(&StylePropertySource::StaticParent)
    );
    assert_eq!(
        theme_source(
            definition,
            BerylThemeRole::MarkdownInlineCode,
            BerylThemeProperty::TextBackground,
        ),
        Some(&StylePropertySource::AmbientParent)
    );
    assert_eq!(
        theme_source(
            definition,
            BerylThemeRole::CodePanelBody,
            BerylThemeProperty::Background,
        ),
        None,
    );
}

fn theme_source(
    definition: &ThemeDefinition,
    role: BerylThemeRole,
    property: BerylThemeProperty,
) -> Option<&StylePropertySource> {
    let property_id = StylePropertyId::from(property.id());
    theme_role(definition, role).properties().get(&property_id)
}

fn theme_role(definition: &ThemeDefinition, role: BerylThemeRole) -> &ThemeRoleDefinition {
    definition
        .roles()
        .iter()
        .find(|definition| definition.role_id().as_str() == role.id())
        .expect("theme role should exist")
}

fn role_record_text<'a>(document: &'a str, role_id: &str) -> &'a str {
    let role_id_line = format!("id = \"{role_id}\"");
    document
        .split("[[role]]")
        .skip(1)
        .find(|section| section.contains(&role_id_line))
        .expect("theme document role should be present")
}

fn wait_for_save(state: &mut settings::SettingsState) {
    for _ in 0..100 {
        match state.poll_save() {
            settings::SettingsSavePoll::Saved => return,
            settings::SettingsSavePoll::Pending => thread::sleep(Duration::from_millis(10)),
            settings::SettingsSavePoll::Idle => panic!("settings save should be pending"),
            settings::SettingsSavePoll::Failed(error) => panic!("settings save failed: {error}"),
        }
    }

    panic!("timed out waiting for settings save");
}

fn active_settings(shared: &Arc<Mutex<ActiveThemeProjection>>) -> AppearanceSettings {
    AppearanceSettings::from_active_theme(&shared.lock().unwrap())
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-settings-window-test-")
}

fn cleanup_temp_dir(root: tempdir_support::TestTempDir) {
    root.close().unwrap();
}

fn context_compaction_timeout_field_id() -> SettingsFieldId {
    SettingsFieldId::from("operations.context_compaction_timeout_seconds")
}

fn theme_property_field_id(role: BerylThemeRole, property: BerylThemeProperty) -> SettingsFieldId {
    SettingsFieldId::from(format!(
        "themes.editor.role.{}.{}",
        role.id(),
        property.id()
    ))
}

fn theme_property_source_field_id(
    role: BerylThemeRole,
    property: BerylThemeProperty,
) -> SettingsFieldId {
    SettingsFieldId::from(format!(
        "themes.editor.role.{}.{}.source",
        role.id(),
        property.id()
    ))
}

fn theme_property_detail_field<'a>(
    model: &'a SettingsWindowModel,
    role: BerylThemeRole,
    property: BerylThemeProperty,
) -> Option<&'a SettingsRowDetailField> {
    model
        .row(&theme_property_source_field_id(role, property))
        .and_then(|row| row.detail_field())
}

fn with_environment_home<T>(home: &Path, action: impl FnOnce() -> T) -> T {
    let userprofile = env::var_os("USERPROFILE");
    let home_var = env::var_os("HOME");
    unsafe {
        env::set_var("USERPROFILE", home);
        env::set_var("HOME", home);
    }

    let result = panic::catch_unwind(AssertUnwindSafe(action));

    restore_env_var("USERPROFILE", userprofile);
    restore_env_var("HOME", home_var);

    match result {
        Ok(value) => value,
        Err(payload) => panic::resume_unwind(payload),
    }
}

fn restore_env_var(key: &str, value: Option<OsString>) {
    unsafe {
        if let Some(value) = value {
            env::set_var(key, value);
        } else {
            env::remove_var(key);
        }
    }
}
