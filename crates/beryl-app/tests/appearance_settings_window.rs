#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

pub use beryl_app::{
    ActiveThemeProjection, AppearanceSettings, BerylThemeProperty, BerylThemeRole,
    InstalledThemeId, StylePropertyId, StylePropertyKind, StylePropertySource, StylePropertyValue,
    StyleRoleId, ThemeDefinition, ThemeRepositorySnapshot, ThemeRepositoryStore,
    ThemeResolutionContext, ThemeResolver, ThemeRoleDefinition, ThemeRoleSchema,
    built_in_theme_definition, built_in_theme_schema, built_in_theme_supported_properties,
};
use gpui_settings_window::{
    SettingsFieldId, SettingsFieldKind, SettingsPageActionId, SettingsPageBodyLayout,
    SettingsPageId, SettingsPageSplitItemPreviewStyle, SettingsRowActionId, SettingsRowDetailField,
    SettingsWindowModel, SettingsWindowOpenDisposition, open_settings_window,
};

#[allow(dead_code)]
#[path = "../src/shell/settings.rs"]
mod settings;

#[test]
fn settings_model_maps_theme_editor_navigator_and_selected_role_rows() {
    let mut state = settings_state(AppearanceSettings::default());
    let model = state.model();

    assert_eq!(model.sections().len(), 4);
    assert_eq!(model.selected_section_id().as_str(), "themes");
    assert_eq!(model.selected_page_id().as_str(), "themes");

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
    assert_eq!(editor.title(), "Theme Editor");
    let breadcrumb_labels: Vec<_> = editor
        .breadcrumb_path()
        .iter()
        .map(|segment| segment.label())
        .collect();
    assert_eq!(breadcrumb_labels.as_slice(), ["Themes", "Theme Editor"]);
    assert_eq!(
        editor.breadcrumb_path()[0].target_page_id(),
        Some(&SettingsPageId::from("themes"))
    );
    assert_eq!(editor.breadcrumb_path()[1].target_page_id(), None);
    assert!(
        editor.local_split().is_none(),
        "theme editor should not build legacy split rows while it is not selected"
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
    assert!(
        editor.local_split().is_none(),
        "theme editor should use the stacked body integration surface instead of the legacy split"
    );
    assert_eq!(editor.body_layout(), SettingsPageBodyLayout::StackedCustom);
    assert_eq!(
        editor
            .stacked_custom_body()
            .expect("theme editor should declare a custom navigator body")
            .body_id()
            .as_str(),
        "themes.editor.role_navigator"
    );
    let role_tree = state.theme_editor_role_tree_projection();
    assert_eq!(
        role_tree.selected_role_id().as_str(),
        BerylThemeRole::AppWindow.id()
    );
    assert_eq!(role_tree.rows().count(), schema_theme_role_ids().len());
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
    assert!(
        foreground_source
            .choices()
            .iter()
            .any(|choice| choice.value() == "static_parent"),
        "app.window should offer static-parent inheritance from its canonical surface parent"
    );
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

    assert!(
        theme_property_detail_field(
            &model,
            BerylThemeRole::AppWindow,
            BerylThemeProperty::FontWeight,
        )
        .is_none(),
        "app.window is a surface role and should not expose font weight"
    );

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
    state.select_theme_editor_role_id(StyleRoleId::from(BerylThemeRole::CodePanelBodyText.id()));

    let model = state.model();
    assert_eq!(model.selected_section_id().as_str(), "themes");
    assert_eq!(model.selected_page_id().as_str(), "themes.editor");
    let editor = model.selected_page();
    assert!(editor.local_split().is_none());
    assert_eq!(editor.body_layout(), SettingsPageBodyLayout::StackedCustom);
    assert_eq!(
        state.selected_theme_role_id().as_str(),
        BerylThemeRole::CodePanelBodyText.id()
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
    let foreground = theme_property_detail_field(
        &model,
        BerylThemeRole::CodePanelBodyText,
        BerylThemeProperty::Foreground,
    )
    .expect("selected code-panel text role foreground detail field should exist");
    assert_eq!(foreground.kind(), SettingsFieldKind::Color);
    assert_eq!(foreground.value(), "#e2e8f0");
    let font_source = model
        .row(&theme_property_source_field_id(
            BerylThemeRole::CodePanelBodyText,
            BerylThemeProperty::FontFamily,
        ))
        .expect("selected code-panel role font source row should exist");
    assert_eq!(font_source.value(), "value");
    assert_eq!(
        model.selected_rows().len(),
        1 + built_in_theme_supported_properties(BerylThemeRole::CodePanelBodyText).len()
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
fn settings_theme_editor_role_id_selection_reconciles_selected_path() {
    let mut state = settings_state(AppearanceSettings::default());
    let selected_roles = [
        BerylThemeRole::Root,
        BerylThemeRole::CodePanelBodyText,
        BerylThemeRole::SyntaxString,
        BerylThemeRole::PopupRowNormal,
    ];

    for role in selected_roles {
        let role_id = StyleRoleId::from(role.id());
        state.select_theme_editor_role_id(role_id.clone());
        let model = state.model();
        let projection = state.theme_editor_role_tree_projection();
        let property_count = role_property_count(role.id());

        assert_eq!(state.selected_theme_role_id(), &role_id);
        assert_eq!(projection.selected_role_id(), &role_id);
        assert_eq!(
            projection.selected_path().last(),
            Some(&role_id),
            "selected path should end at the selected role id"
        );
        assert!(
            projection
                .selected_path()
                .iter()
                .all(|path_role_id| projection.row(path_role_id).is_some()),
            "selected path must contain only real schema role ids"
        );
        assert_eq!(
            model.selected_rows().len(),
            1 + property_count,
            "detail rows should be limited to Save As plus the selected role properties"
        );
        assert!(
            model
                .row(&theme_property_source_field_id(
                    BerylThemeRole::AppWindow,
                    BerylThemeProperty::Foreground,
                ))
                .is_none()
                || role == BerylThemeRole::AppWindow,
            "unselected role property rows should not remain in the selected-role model"
        );
    }
}

#[test]
fn settings_theme_editor_selected_role_survives_rebuilds_by_role_id() {
    let mut state = settings_state(AppearanceSettings::default());
    let selected_role_id = StyleRoleId::from(BerylThemeRole::PopupRowNormal.id());

    state.select_theme_editor_role_id(selected_role_id.clone());
    let first = state.model();
    let first_projection = state.theme_editor_role_tree_projection();

    assert_eq!(state.selected_theme_role_id(), &selected_role_id);
    assert_eq!(first_projection.selected_role_id(), &selected_role_id);
    assert_eq!(first.selected_rows().len(), 1);

    let rebuilt = state.model();
    let rebuilt_projection = state.theme_editor_role_tree_projection();

    assert_eq!(state.selected_theme_role_id(), &selected_role_id);
    assert_eq!(rebuilt_projection.selected_role_id(), &selected_role_id);
    assert_eq!(
        rebuilt_projection.selected_path(),
        first_projection.selected_path()
    );
    assert_eq!(
        rebuilt.selected_rows().len(),
        first.selected_rows().len(),
        "model rebuilds should preserve no-property role selection by role id"
    );
}

#[test]
fn settings_theme_editor_stale_selected_role_recovers_to_schema_root() {
    let mut state = settings_state(AppearanceSettings::default());

    state.set_selected_theme_role_id_for_test(StyleRoleId::from("missing.schema.role"));
    state.reset_draft_from_active();

    let root_role_id = StyleRoleId::from(BerylThemeRole::Root.id());
    let model = state.model();
    let projection = state.theme_editor_role_tree_projection();

    assert_eq!(state.selected_theme_role_id(), &root_role_id);
    assert_eq!(projection.selected_role_id(), &root_role_id);
    assert_eq!(projection.selected_path(), &[root_role_id.clone()]);
    assert!(
        projection
            .row(&StyleRoleId::from("missing.schema.role"))
            .is_none()
    );
    assert!(
        model
            .row(&SettingsFieldId::from(
                "themes.editor.role.missing.schema.role.background.source"
            ))
            .is_none(),
        "stale role recovery must not create synthetic fallback property rows"
    );
    assert!(
        !model.selected_rows().is_empty(),
        "stale role recovery should keep the selected-role model present"
    );
}

#[test]
fn settings_theme_editor_exposes_only_color_for_single_color_roles() {
    let mut state = settings_state(AppearanceSettings::default());

    state.select_page(SettingsPageId::from("themes.editor"));
    select_theme_role(&mut state, BerylThemeRole::MarkdownThematicBreak);

    let model = state.model();
    assert_eq!(
        model.selected_rows().len(),
        1 + built_in_theme_supported_properties(BerylThemeRole::MarkdownThematicBreak).len()
    );
    let color_source = model
        .row(&theme_property_source_field_id(
            BerylThemeRole::MarkdownThematicBreak,
            BerylThemeProperty::Color,
        ))
        .expect("single-color role color source row should exist");
    assert_eq!(color_source.value(), "static_parent");
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
fn settings_theme_editor_no_property_schema_roles_select_with_empty_property_rows() {
    let mut state = settings_state(AppearanceSettings::default());

    state.select_page(SettingsPageId::from("themes.editor"));
    select_theme_role(&mut state, BerylThemeRole::PopupRowNormal);

    let model = state.model();
    assert_eq!(
        built_in_theme_supported_properties(BerylThemeRole::PopupRowNormal),
        &[]
    );
    assert!(
        model
            .row(&theme_property_source_field_id(
                BerylThemeRole::PopupRowNormal,
                BerylThemeProperty::Background,
            ))
            .is_none()
    );
    assert!(
        model.selected_rows().len() == 1,
        "no-property schema roles keep only the Save As row in the existing detail pane"
    );
}

#[test]
fn settings_theme_editor_property_rows_match_selected_role_supported_properties() {
    let mut state = settings_state(AppearanceSettings::default());

    state.select_page(SettingsPageId::from("themes.editor"));
    for role in BerylThemeRole::ALL {
        select_theme_role(&mut state, *role);
        let model = state.model();
        let supported = built_in_theme_supported_properties(*role);

        assert_eq!(
            model.selected_rows().len(),
            1 + supported.len(),
            "selected role {} should expose only Save As plus supported property rows",
            role.id()
        );

        for property in BerylThemeProperty::ALL {
            let source_row = model.row(&theme_property_source_field_id(*role, *property));
            assert_eq!(
                source_row.is_some(),
                supported.contains(property),
                "selected role {} property {} row presence should match schema support",
                role.id(),
                property.id()
            );

            if let Some(source_row) = source_row {
                let offers_static_parent = source_row
                    .choices()
                    .iter()
                    .any(|choice| choice.value() == "static_parent");
                let static_parent_is_valid = role.static_parent().is_some_and(|parent| {
                    built_in_theme_supported_properties(parent).contains(property)
                });
                assert_eq!(
                    offers_static_parent,
                    static_parent_is_valid,
                    "selected role {} property {} should offer static-parent only when the static parent supports that property",
                    role.id(),
                    property.id()
                );
            }
        }
    }
}

#[test]
fn settings_theme_editor_hides_invalid_static_parent_source() {
    let mut state = settings_state(AppearanceSettings::default());
    let (role, property) = invalid_static_parent_pair_for_test();
    let source_field_id = theme_property_source_field_id(role, property);

    state.select_page(SettingsPageId::from("themes.editor"));
    select_theme_role(&mut state, role);
    let source = state
        .model()
        .row(&source_field_id)
        .expect("selected role property source row should exist");

    assert!(
        !source
            .choices()
            .iter()
            .any(|choice| choice.value() == "static_parent"),
        "invalid static-parent source must not be a visible choice"
    );
}

#[test]
fn settings_theme_editor_role_navigator_projection_uses_schema_tree() {
    let state = settings_state(AppearanceSettings::default());
    let projection = state.theme_editor_role_tree_projection();
    let schema = built_in_theme_schema();
    let schema_ids = schema_theme_role_ids().into_iter().collect::<BTreeSet<_>>();
    let row_ids = projection
        .rows()
        .map(|row| row.role_id().as_str().to_string())
        .collect::<BTreeSet<_>>();

    assert_eq!(
        row_ids, schema_ids,
        "navigator projection must contain exactly built-in schema role ids"
    );
    assert_eq!(
        projection.root_role_id().as_str(),
        BerylThemeRole::Root.id()
    );
    let root_column = projection
        .columns()
        .first()
        .expect("navigator projection should expose a root column");
    assert_eq!(root_column.parent_role_id(), None);
    assert_eq!(root_column.rows().len(), 1);
    assert_eq!(
        root_column.rows()[0].role_id().as_str(),
        BerylThemeRole::Root.id()
    );

    for schema_role in schema.roles() {
        let row = projection
            .row(schema_role.role_id())
            .expect("schema role should have a navigator row");
        assert_eq!(row.static_parent_id(), schema_role.static_parent());
        assert_eq!(row.property_row_count(), schema_role.properties().len());
        assert!(!row.label().is_empty());
        let child_column = projection
            .child_column(schema_role.role_id())
            .expect("schema role should produce a child column projection");
        assert_eq!(child_column.parent_role_id(), Some(schema_role.role_id()));
        for child in child_column.rows() {
            assert_eq!(
                child.static_parent_id(),
                Some(schema_role.role_id()),
                "child columns must follow schema static-parent relationships"
            );
        }
    }

    let no_property_role = schema
        .roles()
        .iter()
        .find(|role| role.properties().is_empty())
        .expect("built-in schema should include a no-property role for this contract");
    assert_eq!(
        projection
            .row(no_property_role.role_id())
            .expect("no-property schema role should remain in navigator projection")
            .property_row_count(),
        0
    );
}

#[test]
fn settings_theme_editor_role_navigator_selecting_branch_opens_child_column() {
    let mut state = settings_state(AppearanceSettings::default());

    state.select_page(SettingsPageId::from("themes.editor"));
    let projection = state.theme_editor_role_tree_projection();
    let branch_role_id = projection
        .rows()
        .find(|row| {
            row.role_id().as_str() != BerylThemeRole::Root.id() && !row.child_role_ids().is_empty()
        })
        .expect("schema should include a non-root branching role")
        .role_id()
        .clone();

    state.select_theme_editor_role_id(branch_role_id.clone());
    let projection = state.theme_editor_role_tree_projection();

    assert_eq!(projection.selected_role_id(), &branch_role_id);
    assert!(
        projection
            .columns()
            .iter()
            .any(|column| column.parent_role_id() == Some(&branch_role_id)),
        "selecting a branching role must open its schema-child column"
    );
}

#[test]
fn settings_theme_editor_role_navigator_rendering_is_fixed_height_windowed() {
    let strategy = settings::SettingsState::theme_role_navigator_render_strategy_for_test();

    assert!(strategy.windowed);
    assert_eq!(strategy.row_height_px, 32);
    assert_eq!(strategy.overscan_rows, 3);
    assert_eq!(
        settings::SettingsState::theme_role_navigator_row_window_for_test(100, 0.0, 96.0),
        0..7,
        "the navigator renders only visible fixed-height rows plus overscan"
    );
    let scrolled =
        settings::SettingsState::theme_role_navigator_row_window_for_test(100, 720.0, 96.0);
    assert!(scrolled.start > 0);
    assert!(scrolled.end < 100);

    let (middle_range, total_height, summed_height) =
        settings::SettingsState::theme_role_navigator_row_window_height_sum_for_test(
            100, 720.0, 96.0,
        );
    assert!(middle_range.start > 0);
    assert!(middle_range.end < 100);
    assert_eq!(
        summed_height, total_height,
        "middle row windows should not add an extra trailing row gap beyond total scroll height"
    );
}

#[test]
fn settings_theme_editor_role_navigator_render_state_keeps_real_role_columns() {
    let mut state = settings_state(AppearanceSettings::default());
    let renderer = settings::SettingsState::theme_editor_role_navigator_body_renderer(|_, _| {});

    state.select_page(SettingsPageId::from("themes.editor"));
    state.select_theme_editor_role_id(StyleRoleId::from(BerylThemeRole::CodePanelBodyText.id()));
    let projection = state.theme_editor_role_tree_projection();
    renderer.update_projection(Some(projection.clone()));
    let first = renderer.diagnostics();

    assert_eq!(first.total_schema_role_count, schema_theme_role_ids().len());
    assert!(first.strategy.windowed);
    assert!(first.visible_row_count <= first.rendered_row_count);
    assert!(first.rendered_row_count <= first.total_schema_role_count);
    assert_eq!(first.column_count, projection.columns().len());
    assert_eq!(first.horizontal_scroll_surface_count, 1);
    assert_eq!(first.column_scroll_surface_count, first.column_count);
    assert!(
        first
            .column_keys
            .iter()
            .flatten()
            .all(|role_id| projection.row(role_id).is_some()),
        "navigator column scroll keys must be real schema role ids"
    );

    let rebuilt = state.theme_editor_role_tree_projection();
    renderer.update_projection(Some(rebuilt.clone()));
    let second = renderer.diagnostics();

    assert_eq!(
        second.column_keys, first.column_keys,
        "horizontal scroll and per-column vertical scroll ownership is reconciled by stable role ids across model refresh"
    );
    assert_eq!(second.column_count, rebuilt.columns().len());
}

#[test]
fn settings_theme_editor_role_navigator_renders_shared_scrollbar_chrome() {
    let navigator_source = include_str!("../src/shell/settings/theme_editor/navigator.rs");
    let chrome_source = include_str!("../src/shell/settings/theme_editor/navigator/chrome.rs");
    let scroll_state_source =
        include_str!("../src/shell/settings/theme_editor/navigator/scroll_state.rs");
    let source = format!("{navigator_source}\n{chrome_source}\n{scroll_state_source}");

    assert!(
        source.contains("render_scroll_handle_scrollbar("),
        "navigator scrollbars should use the shared gpui-scrollbar affordance"
    );
    assert!(
        source.contains("theme-role-navigator-horizontal-scrollbar"),
        "top navigator horizontal scroll surface should render scrollbar chrome"
    );
    assert!(
        source.contains("theme-role-navigator-column-scrollbar"),
        "role column vertical scroll surfaces should render scrollbar chrome"
    );
    assert!(source.contains("ScrollbarAxis::Horizontal"));
    assert!(source.contains("ScrollbarAxis::Vertical"));
    assert!(
        source.contains("ScrollHandle"),
        "navigator scrollbars should stay wired to the owning scroll handles"
    );
    assert!(
        source.contains("ScrollbarVisibilityState"),
        "navigator scrollbars should use the shared managed visibility lifecycle"
    );
    assert!(
        source.contains("record_viewport_activity"),
        "navigator scroll surfaces should report viewport activity into the shared affordance"
    );
    assert!(
        !source.contains("ScrollbarVisibilityPolicy::always_visible"),
        "navigator scrollbars should not bypass the shared fade/activity policy"
    );
}

#[gpui::test]
fn settings_theme_editor_role_selection_preserves_lower_editor_focus_scroll_and_popups(
    cx: &mut gpui::TestAppContext,
) {
    let mut state = settings_state(AppearanceSettings::default());
    let renderer = settings::SettingsState::theme_editor_role_navigator_body_renderer(|_, _| {});
    let save_as_field_id = SettingsFieldId::from("themes.save_as_name");
    let color_field_id =
        theme_property_field_id(BerylThemeRole::AppWindow, BerylThemeProperty::Foreground);

    state.select_page(SettingsPageId::from("themes.editor"));
    select_theme_role(&mut state, BerylThemeRole::AppWindow);
    renderer.update_projection(state.selected_theme_editor_role_tree_projection());
    let options = renderer.options_with_renderer(state.window_options());
    let model = state.model();
    let handle = cx.update(|cx| {
        open_settings_window(
            cx,
            model,
            options,
            SettingsWindowOpenDisposition::Visible {
                focus_requested: false,
            },
        )
        .expect("settings window should open")
    });

    handle
        .window_handle()
        .update(cx, |view, window, cx| {
            view.set_content_scroll_offset_for_test(80.0, cx);
            assert!(view.focus_field(&save_as_field_id, window, cx));
            view.open_color_picker_for_test(color_field_id.clone(), window, cx);
            assert_eq!(
                view.active_color_picker_field_for_test(cx),
                Some(color_field_id.clone())
            );

            view.set_content_scroll_offset_for_test(120.0, cx);
            assert_eq!(
                view.active_color_picker_field_for_test(cx),
                Some(color_field_id.clone()),
                "property-editor scrolling should not close a color picker whose anchor row remains selected"
            );
        })
        .expect("settings window should update");

    select_theme_role(&mut state, BerylThemeRole::CodePanelBodyText);
    renderer.update_projection(state.selected_theme_editor_role_tree_projection());
    handle
        .update_model(cx, state.model())
        .expect("model update should succeed");

    handle
        .window_handle()
        .update(cx, |view, window, cx| {
            assert_eq!(view.model().selected_page_id().as_str(), "themes.editor");
            assert_eq!(view.settings_scroll_metrics(cx).0, -120.0);
            assert_eq!(
                view.focused_field_for_test(window, cx),
                Some(save_as_field_id.clone()),
                "same-page navigator role selection should retain focus on stable lower-editor text inputs"
            );
            assert_eq!(
                view.active_color_picker_field_for_test(cx),
                None,
                "role selection should close a color picker whose field row left the selected-role editor"
            );
            assert!(
                !view.has_transient_popups(cx),
                "role selection should not leave stale lower-editor popups anchored to removed rows"
            );
        })
        .expect("settings window should update");
}

#[gpui::test]
fn settings_theme_editor_same_window_role_selection_sync_is_deferred(
    cx: &mut gpui::TestAppContext,
) {
    let mut state = settings_state(AppearanceSettings::default());
    state.select_page(SettingsPageId::from("themes.editor"));
    select_theme_role(&mut state, BerylThemeRole::AppWindow);
    let handle = cx.update(|cx| {
        open_settings_window(
            cx,
            state.model(),
            state.window_options(),
            SettingsWindowOpenDisposition::Visible {
                focus_requested: false,
            },
        )
        .expect("settings window should open")
    });

    select_theme_role(&mut state, BerylThemeRole::Root);
    let root_model = state.model();
    let root_source_field_id =
        theme_property_source_field_id(BerylThemeRole::Root, BerylThemeProperty::Background);
    let stale_source_field_id =
        theme_property_source_field_id(BerylThemeRole::AppWindow, BerylThemeProperty::Background);

    handle
        .window_handle()
        .update(cx, |_view, _window, cx| {
            assert!(
                handle.update_model(cx, root_model.clone()).is_err(),
                "a custom body click runs while the settings window is on GPUI's update stack"
            );
            cx.defer(move |cx| {
                handle
                    .update_model(cx, root_model)
                    .expect("deferred same-window model sync should succeed");
            });
        })
        .expect("settings window should update");
    cx.run_until_parked();

    handle
        .window_handle()
        .read_with(cx, |view, _| {
            assert!(
                view.model().row(&root_source_field_id).is_some(),
                "deferred role selection sync should update the lower editor rows"
            );
            assert!(
                view.model().row(&stale_source_field_id).is_none(),
                "stale selected-role rows should leave the lower editor after deferred sync"
            );
        })
        .expect("settings window should be readable");
}

#[test]
fn settings_theme_editor_role_previews_ignore_draft_values() {
    let mut state = settings_state(AppearanceSettings::default());
    let field_id = theme_property_field_id(
        BerylThemeRole::CodePanelBodyText,
        BerylThemeProperty::Foreground,
    );

    state.select_page(SettingsPageId::from("themes.editor"));
    select_theme_role(&mut state, BerylThemeRole::CodePanelBodyText);
    let original_foreground = theme_role_preview_style(&state, BerylThemeRole::CodePanelBodyText)
        .and_then(|style| style.foreground())
        .map(|color| color.to_hex());

    state.set_field_value(
        &theme_property_source_field_id(
            BerylThemeRole::CodePanelBodyText,
            BerylThemeProperty::Foreground,
        ),
        "value".to_string(),
    );
    state.set_field_value(&field_id, "#123456".to_string());

    let model = state.model();
    assert_eq!(
        theme_role_preview_style(&state, BerylThemeRole::CodePanelBodyText)
            .and_then(|style| style.foreground())
            .map(|color| color.to_hex()),
        original_foreground,
        "draft color edits must not live-preview in the role navigator model"
    );
    let source_row = model
        .row(&theme_property_source_field_id(
            BerylThemeRole::CodePanelBodyText,
            BerylThemeProperty::Foreground,
        ))
        .expect("foreground property row should exist");
    assert!(source_row.is_modified());
    assert!(
        source_row
            .detail_field()
            .is_some_and(SettingsRowDetailField::is_modified)
    );
    assert_eq!(
        source_row.detail_field().map(|field| field.value()),
        Some("#123456")
    );
}

#[test]
fn settings_theme_editor_single_color_role_previews_ignore_draft_values() {
    let mut state = settings_state(AppearanceSettings::default());
    let field_id = theme_property_field_id(
        BerylThemeRole::MarkdownThematicBreak,
        BerylThemeProperty::Color,
    );

    state.select_page(SettingsPageId::from("themes.editor"));
    select_theme_role(&mut state, BerylThemeRole::MarkdownThematicBreak);
    let original_border = theme_role_preview_style(&state, BerylThemeRole::MarkdownThematicBreak)
        .and_then(|style| style.border())
        .map(|color| color.to_hex());

    state.set_field_value(
        &theme_property_source_field_id(
            BerylThemeRole::MarkdownThematicBreak,
            BerylThemeProperty::Color,
        ),
        "value".to_string(),
    );
    state.set_field_value(&field_id, "#abcdef".to_string());

    let model = state.model();
    assert_eq!(
        theme_role_preview_style(&state, BerylThemeRole::MarkdownThematicBreak)
            .and_then(|style| style.border())
            .map(|color| color.to_hex()),
        original_border,
        "draft single-color edits must not live-preview in the role navigator model"
    );
    assert_eq!(
        theme_property_detail_field(
            &model,
            BerylThemeRole::MarkdownThematicBreak,
            BerylThemeProperty::Color,
        )
        .map(|field| field.value()),
        Some("#abcdef")
    );
}

#[test]
fn settings_theme_editor_property_source_changes_remain_typed() {
    let mut state = settings_state(AppearanceSettings::default());
    let source_field_id = theme_property_source_field_id(
        BerylThemeRole::MarkdownInlineCode,
        BerylThemeProperty::TextBackground,
    );
    let value_field_id = theme_property_field_id(
        BerylThemeRole::MarkdownInlineCode,
        BerylThemeProperty::TextBackground,
    );

    state.select_page(SettingsPageId::from("themes.editor"));
    select_theme_role(&mut state, BerylThemeRole::MarkdownInlineCode);
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

    state.set_field_value(&source_field_id, "fallback".to_string());
    let model = state.model();
    let source = model
        .row(&source_field_id)
        .expect("updated source row should remain present");
    assert_eq!(source.value(), "fallback");
    assert!(
        source.detail_field().is_none(),
        "fallback sources must not manufacture a concrete value editor"
    );
}

#[test]
fn settings_theme_editor_static_parent_source_choice_uses_parent_role_label() {
    let mut state = settings_state(AppearanceSettings::default());
    let source_field_id = theme_property_source_field_id(
        BerylThemeRole::CodePanelBodyText,
        BerylThemeProperty::Foreground,
    );

    state.select_page(SettingsPageId::from("themes.editor"));
    select_theme_role(&mut state, BerylThemeRole::CodePanelBodyText);
    let model = state.model();
    let source = model
        .row(&source_field_id)
        .expect("code panel body foreground source row should exist");
    let static_parent_choice = source
        .choices()
        .iter()
        .find(|choice| choice.value() == "static_parent")
        .expect("roles with static parents should offer static-parent inheritance");

    assert_eq!(static_parent_choice.label(), BerylThemeRole::TextCode.id());

    state.set_field_value(&source_field_id, "static_parent".to_string());
    let source = state
        .model()
        .row(&source_field_id)
        .expect("updated source row should remain present");
    assert_eq!(source.value(), "static_parent");
    assert!(source.detail_field().is_none());
}

#[test]
fn settings_theme_editor_concrete_source_uses_typed_value_editor() {
    let mut state = settings_state(AppearanceSettings::default());
    let source_field_id = theme_property_source_field_id(
        BerylThemeRole::MarkdownInlineCode,
        BerylThemeProperty::TextBackground,
    );
    let value_field_id = theme_property_field_id(
        BerylThemeRole::MarkdownInlineCode,
        BerylThemeProperty::TextBackground,
    );

    state.select_page(SettingsPageId::from("themes.editor"));
    select_theme_role(&mut state, BerylThemeRole::MarkdownInlineCode);
    state.set_field_value(&source_field_id, "value".to_string());
    let model = state.model();
    let value = model
        .row(&source_field_id)
        .and_then(|row| row.detail_field())
        .expect("concrete source should expose a nested concrete value editor");
    assert_eq!(value.kind(), SettingsFieldKind::Color);
    state.set_field_value(&value_field_id, "#445566".to_string());
    assert_eq!(
        state
            .model()
            .row(&source_field_id)
            .and_then(|row| row.detail_field())
            .map(|field| field.value()),
        Some("#445566")
    );
}

#[test]
fn settings_theme_editor_static_parent_is_not_role_list_subtext_or_text_field() {
    let mut state = settings_state(AppearanceSettings::default());

    state.select_page(SettingsPageId::from("themes.editor"));
    select_theme_role(&mut state, BerylThemeRole::CodePanelBodyText);
    let model = state.model();
    let role_tree = state.theme_editor_role_tree_projection();
    let item = role_tree
        .row(&StyleRoleId::from(BerylThemeRole::CodePanelBody.id()))
        .expect("code panel body role should exist");

    assert_eq!(
        item.static_parent_id().map(StyleRoleId::as_str),
        Some(BerylThemeRole::SurfaceInset.id())
    );
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
    select_theme_role(&mut state, BerylThemeRole::AppWindow);
    state.set_field_value(
        &theme_property_source_field_id(BerylThemeRole::AppWindow, BerylThemeProperty::Background),
        "value".to_string(),
    );
    state.set_field_value(
        &theme_property_field_id(BerylThemeRole::AppWindow, BerylThemeProperty::Background),
        "#101112".to_string(),
    );

    assert!(state.theme_draft_modified_for_external_change());
    assert!(
        state.window_options_for_sync().is_none(),
        "staged field edits must sync the model without resyncing unchanged window options"
    );
}

#[test]
fn settings_theme_editor_typing_is_draft_only_and_does_not_rebuild_previews() {
    let mut state = settings_state(AppearanceSettings::default());

    assert!(state.theme_editor_diagnostics_snapshot().is_none());

    state.select_page(SettingsPageId::from("themes.editor"));
    select_theme_role(&mut state, BerylThemeRole::Root);
    let model = state.model();
    let diagnostics = state
        .theme_editor_diagnostics_snapshot()
        .expect("theme editor model diagnostics should be available on editor page");

    assert_eq!(diagnostics.candidate_definition_build_count, 0);
    assert_eq!(diagnostics.preview_projection_build_count, 1);
    assert_eq!(
        diagnostics.role_preview_style_build_count,
        schema_theme_role_ids().len() as u64
    );
    assert_eq!(
        diagnostics.total_schema_role_count,
        schema_theme_role_ids().len()
    );
    assert!(diagnostics.navigator_column_count > 0);
    assert!(diagnostics.selected_role_path_count > 0);
    assert!(diagnostics.selected_property_detail_row_count > 0);
    assert_eq!(diagnostics.modified_state_recompute_count, 0);

    assert!(model.selected_page().local_split().is_none());
    let root_preview = theme_role_preview_style(&state, BerylThemeRole::Root)
        .expect("root role should have an active-definition preview style");

    state.set_field_value(
        &theme_property_field_id(BerylThemeRole::Root, BerylThemeProperty::FontFamily),
        "Some Slow Font Name".to_string(),
    );
    state.set_field_value(
        &theme_property_field_id(BerylThemeRole::Root, BerylThemeProperty::Background),
        "#aaaaaa".to_string(),
    );
    let diagnostics = state
        .theme_editor_diagnostics_snapshot()
        .expect("theme editor diagnostics should retain latest model metrics");

    assert_eq!(
        diagnostics.modified_state_recompute_count, 0,
        "typing theme values must not recompute exact modified state"
    );
    assert_eq!(diagnostics.candidate_definition_build_count, 0);
    assert_eq!(diagnostics.preview_projection_build_count, 1);

    let model = state.model();
    let diagnostics = state
        .theme_editor_diagnostics_snapshot()
        .expect("theme editor diagnostics should retain cached model metrics");

    assert_eq!(
        diagnostics.candidate_definition_build_count, 0,
        "typing must not build a candidate preview definition"
    );
    assert_eq!(diagnostics.preview_projection_build_count, 1);
    assert_eq!(
        diagnostics.role_preview_style_build_count,
        schema_theme_role_ids().len() as u64
    );
    assert_eq!(
        theme_property_detail_field(&model, BerylThemeRole::Root, BerylThemeProperty::FontFamily,)
            .map(|field| field.value()),
        Some("Some Slow Font Name")
    );
    assert_eq!(
        theme_property_detail_field(&model, BerylThemeRole::Root, BerylThemeProperty::Background,)
            .map(|field| field.value()),
        Some("#aaaaaa")
    );
    assert!(
        state.theme_draft_modified_for_external_change(),
        "ordinary text edits must mark the theme draft as staged"
    );
    let edited_root_preview = theme_role_preview_style(&state, BerylThemeRole::Root)
        .expect("root role should still have a preview style");
    assert_eq!(
        edited_root_preview, root_preview,
        "typing must not live-preview draft font or color changes"
    );

    state.set_field_value(
        &theme_property_field_id(BerylThemeRole::Root, BerylThemeProperty::Background),
        "#bbbbbb".to_string(),
    );
    let model = state.model();
    let diagnostics = state
        .theme_editor_diagnostics_snapshot()
        .expect("theme editor diagnostics should keep full rebuild counters stable");

    assert_eq!(diagnostics.modified_state_recompute_count, 0);
    assert_eq!(diagnostics.candidate_definition_build_count, 0);
    assert_eq!(diagnostics.preview_projection_build_count, 1);
    assert_eq!(
        theme_property_detail_field(&model, BerylThemeRole::Root, BerylThemeProperty::Background,)
            .map(|field| field.value()),
        Some("#bbbbbb")
    );
    let edited_root_preview = theme_role_preview_style(&state, BerylThemeRole::Root)
        .expect("root role should still have a preview style");
    assert_eq!(
        edited_root_preview, root_preview,
        "repeated typing must keep preview styles pinned to the active definition"
    );
}

#[test]
fn settings_window_options_sync_invalidates_once_for_active_theme_preview() {
    let active = AppearanceSettings::default()
        .to_active_theme_projection()
        .unwrap();
    let shared = Arc::new(Mutex::new(active.clone()));
    let mut state = settings::SettingsState::new_without_theme_repository(shared.clone());
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

    *shared.lock().unwrap() = active;
    let restored_options = state
        .window_options_for_sync()
        .expect("stopping preview should restore visual options once");
    assert_eq!(restored_options, initial);
    state.record_window_options_synced(restored_options);
    assert!(
        state.window_options_for_sync().is_none(),
        "restored options should not publish twice"
    );
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
fn settings_notification_row_actions_choose_and_clear() {
    let mut state = settings_state(AppearanceSettings::default());
    let field_id = state.notification_end_turn_sound_field_id();
    let sound_path = r"C:\sounds\turn-done.wav";

    assert_eq!(
        state.handle_row_action(&field_id, &SettingsRowActionId::from("choose")),
        Some(settings::SettingsRowActionOutcome::PromptForEndTurnSoundPath)
    );

    state.set_notification_end_turn_sound_path(sound_path.to_string());
    assert_eq!(
        state.model().row(&field_id).map(|row| row.value()),
        Some(sound_path)
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
}

#[test]
fn settings_theme_modified_state_tracks_edits_and_reset() {
    let mut state = settings_state(AppearanceSettings::default());
    let source_field_id =
        theme_property_source_field_id(BerylThemeRole::AppWindow, BerylThemeProperty::Background);
    let value_field_id =
        theme_property_field_id(BerylThemeRole::AppWindow, BerylThemeProperty::Background);

    assert!(!state.theme_draft_modified_for_external_change());

    state.select_page(SettingsPageId::from("themes.editor"));
    state.set_field_value(&source_field_id, "value".to_string());
    state.set_field_value(&value_field_id, "slate".to_string());
    assert!(
        state.theme_draft_modified_for_external_change(),
        "field edits, including invalid concrete values, should mark the theme draft modified"
    );

    state.reset_draft_from_active();
    assert!(
        !state.theme_draft_modified_for_external_change(),
        "cancel/reset should clear staged theme edits"
    );
}

#[test]
fn settings_theme_save_as_adds_an_installed_theme() {
    let root = unique_temp_dir();
    let theme_store = ThemeRepositoryStore::new(&root);
    let theme_snapshot = theme_store.load_or_default().unwrap();
    let active_theme = Arc::new(Mutex::new(ActiveThemeProjection::built_in()));
    let mut state = settings::SettingsState::new_with_theme_repository(
        active_theme,
        theme_store,
        theme_snapshot,
    );
    let field_id = theme_property_field_id(
        BerylThemeRole::CodePanelBodyText,
        BerylThemeProperty::Foreground,
    );
    let save_as_name = SettingsFieldId::from("themes.save_as_name");

    state.select_page(SettingsPageId::from("themes.editor"));
    select_theme_role(&mut state, BerylThemeRole::CodePanelBodyText);
    state.set_field_value(
        &theme_property_source_field_id(
            BerylThemeRole::CodePanelBodyText,
            BerylThemeProperty::Foreground,
        ),
        "value".to_string(),
    );
    state.set_field_value(&field_id, "#223344".to_string());
    state.set_field_value(&save_as_name, "Alternate Theme".to_string());
    assert_eq!(
        state.handle_page_action(&SettingsPageActionId::from("save_as")),
        Some(settings::SettingsPageActionOutcome::Updated)
    );

    let model = state.model();
    let installed = model
        .row(&SettingsFieldId::from("themes.installed.alternate-theme"))
        .expect("Save As should add an installed-theme row");
    assert_eq!(installed.label(), "Alternate Theme");
    assert_eq!(installed.actions()[0].label(), "Activate");
    cleanup_temp_dir(root);
}

#[test]
fn settings_reset_discards_unapplied_draft_and_preserves_selected_section() {
    let mut state = settings_state(AppearanceSettings::default());
    let field_id = theme_property_field_id(
        BerylThemeRole::CodePanelBodyText,
        BerylThemeProperty::FontFamily,
    );

    state.select_page(SettingsPageId::from("themes.editor"));
    select_theme_role(&mut state, BerylThemeRole::CodePanelBodyText);
    state.set_field_value(&field_id, "JetBrains Mono".to_string());
    state.reset_draft_from_active();

    let model = state.model();
    assert_eq!(model.selected_section_id().as_str(), "themes");
    assert_eq!(model.selected_page_id().as_str(), "themes.editor");
    assert_eq!(
        theme_property_detail_field(
            &model,
            BerylThemeRole::CodePanelBodyText,
            BerylThemeProperty::FontFamily,
        )
        .map(|field| field.value()),
        Some("Consolas")
    );
}

fn settings_state(settings_value: AppearanceSettings) -> settings::SettingsState {
    let active_theme = Arc::new(Mutex::new(
        settings_value.to_active_theme_projection().unwrap(),
    ));
    settings::SettingsState::new_without_theme_repository(active_theme)
}

fn select_theme_role(state: &mut settings::SettingsState, role: BerylThemeRole) {
    state.select_theme_editor_role_id(StyleRoleId::from(role.id()));
}

fn invalid_static_parent_pair_for_test() -> (BerylThemeRole, BerylThemeProperty) {
    for role in BerylThemeRole::ALL {
        let Some(parent) = role.static_parent() else {
            continue;
        };
        let parent_properties = built_in_theme_supported_properties(parent);
        for property in built_in_theme_supported_properties(*role) {
            if !parent_properties.contains(property) {
                return (*role, *property);
            }
        }
    }
    panic!("built-in theme schema should include a static-parent-invalid property pair");
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

fn theme_role_preview_style(
    state: &settings::SettingsState,
    role: BerylThemeRole,
) -> Option<SettingsPageSplitItemPreviewStyle> {
    state
        .theme_editor_role_tree_projection()
        .row(&StyleRoleId::from(role.id()))
        .and_then(|row| row.preview_style())
        .cloned()
}

fn schema_theme_role_ids() -> Vec<String> {
    built_in_theme_schema()
        .roles()
        .iter()
        .map(|role| role.role_id().as_str().to_string())
        .collect()
}

fn role_property_count(role_id: &str) -> usize {
    built_in_theme_schema()
        .roles()
        .iter()
        .find(|role| role.role_id().as_str() == role_id)
        .expect("theme role schema should exist")
        .properties()
        .len()
}
