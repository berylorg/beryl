#[test]
fn phase7_render_sources_do_not_use_appearance_settings_or_literal_colors() {
    for (path, source) in PHASE7_RENDER_SOURCES {
        let source_without_functional_transparency =
            source.replace("rgba(0x00000000)", "FUNCTIONAL_TRANSPARENT_TEXT");

        assert!(
            !source.contains("AppearanceSettings"),
            "{path} should not depend on flat appearance settings"
        );
        assert!(
            !source.contains("appearance."),
            "{path} should not read appearance fields directly"
        );
        assert!(
            !source_without_functional_transparency.contains("rgb("),
            "{path} should resolve visible colors through theme roles"
        );
        assert!(
            !source_without_functional_transparency.contains("rgba("),
            "{path} should not embed visible rgba colors"
        );
    }
}

#[test]
fn phase7_graph_and_checklist_sources_name_surface_roles() {
    let graph_overlay = include_str!("../src/shell/render/graph_overlay.rs");
    let graph_rows = include_str!("../src/shell/render/graph_overlay/rows.rs");
    let graph_menu_rows = include_str!("../src/shell/render/graph_link_menu_rows.rs");
    let checklist = include_str!("../src/shell/render/checklist_sidebar.rs");

    for role in [
        "BerylThemeRole::GraphOverlay",
        "BerylThemeRole::GraphColumn",
        "BerylThemeRole::GraphColumnHeader",
        "BerylThemeRole::GraphRowPending",
        "BerylThemeRole::GraphRowError",
    ] {
        assert!(
            graph_overlay.contains(role),
            "graph overlay should use {role}"
        );
    }

    for role in [
        "BerylThemeRole::GraphRowTopic",
        "BerylThemeRole::GraphRowChecklist",
        "BerylThemeRole::GraphRowChecklistItem",
        "BerylThemeRole::GraphRowThreadRef",
        "BerylThemeRole::GraphRowSoftLink",
        "BerylThemeRole::GraphRowHover",
        "BerylThemeRole::GraphRowSelected",
        "BerylThemeRole::GraphRowDisabled",
        "BerylThemeRole::GraphRowInvalid",
    ] {
        assert!(graph_rows.contains(role), "graph rows should use {role}");
    }

    assert!(
        graph_menu_rows.contains("BerylThemeRole::GraphRowError"),
        "held destructive graph action progress should resolve from graph error styling"
    );

    for role in [
        "BerylThemeRole::ChecklistSidebar",
        "BerylThemeRole::ChecklistHeader",
        "BerylThemeRole::ChecklistRow",
        "BerylThemeRole::ChecklistStatusTodo",
        "BerylThemeRole::ChecklistStatusInProgress",
        "BerylThemeRole::ChecklistStatusDone",
    ] {
        assert!(
            checklist.contains(role),
            "checklist sidebar should use {role}"
        );
    }
}

#[test]
fn phase7_selector_notice_and_status_sources_name_state_roles() {
    let conversation = include_str!("../src/shell/render/conversation.rs");
    let thread_selector = include_str!("../src/shell/render/thread_selector.rs");
    let workspace_picker = include_str!("../src/shell/render/workspace_picker.rs");

    for role in [
        "BerylThemeRole::NoticeInfo",
        "BerylThemeRole::NoticeWarning",
        "BerylThemeRole::NoticeError",
        "BerylThemeRole::NoticeSuccess",
        "BerylThemeRole::StatusValuePending",
        "BerylThemeRole::StatusValueUnavailable",
        "BerylThemeRole::StatusValueStreaming",
    ] {
        assert!(
            conversation.contains(role),
            "conversation shell should use {role}"
        );
    }

    for role in [
        "BerylThemeRole::ThreadSelectorSurface",
        "BerylThemeRole::ThreadSelectorRowSelected",
        "BerylThemeRole::ThreadSelectorRowUnavailable",
    ] {
        assert!(
            thread_selector.contains(role),
            "thread selector should use {role}"
        );
    }

    for role in [
        "BerylThemeRole::WorkspacePickerSurface",
        "BerylThemeRole::WorkspacePickerWorkspaceRow",
        "BerylThemeRole::WorkspacePickerMemberRow",
        "BerylThemeRole::WorkspacePickerRowActive",
    ] {
        assert!(
            workspace_picker.contains(role),
            "workspace picker should use {role}"
        );
    }
}

const PHASE7_RENDER_SOURCES: &[(&str, &str)] = &[
    (
        "src/shell/render/graph_overlay.rs",
        include_str!("../src/shell/render/graph_overlay.rs"),
    ),
    (
        "src/shell/render/graph_overlay/rows.rs",
        include_str!("../src/shell/render/graph_overlay/rows.rs"),
    ),
    (
        "src/shell/render/graph_link_menu.rs",
        include_str!("../src/shell/render/graph_link_menu.rs"),
    ),
    (
        "src/shell/render/graph_link_menu_rows.rs",
        include_str!("../src/shell/render/graph_link_menu_rows.rs"),
    ),
    (
        "src/shell/render/checklist_sidebar.rs",
        include_str!("../src/shell/render/checklist_sidebar.rs"),
    ),
    (
        "src/shell/render/thread_selector.rs",
        include_str!("../src/shell/render/thread_selector.rs"),
    ),
    (
        "src/shell/render/workspace_picker.rs",
        include_str!("../src/shell/render/workspace_picker.rs"),
    ),
    (
        "src/shell/render/workspace_picker_row_menu.rs",
        include_str!("../src/shell/render/workspace_picker_row_menu.rs"),
    ),
    (
        "src/shell/render/conversation.rs",
        include_str!("../src/shell/render/conversation.rs"),
    ),
];
