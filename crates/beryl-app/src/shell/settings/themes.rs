use std::collections::HashMap;

use gpui_settings_window::{
    SettingsBreadcrumbSegment, SettingsFieldId, SettingsPage, SettingsPageAction,
    SettingsPageActionId, SettingsPageActionPriority, SettingsPageId, SettingsRow,
    SettingsRowAction, SettingsRowActionId, SettingsSection, SettingsSectionId,
};

use crate::{BUILT_IN_INSTALLED_THEME_ID, InstalledThemeId, ThemeRepositorySnapshot};

use super::theme_editor::ThemeEditorPageModel;

pub(super) const SECTION_ID: &str = "themes";
pub(super) const EDITOR_PAGE_ID: &str = "themes.editor";
const ACTIVATE_ACTION_ID: &str = "activate";
const SAVE_ACTION_ID: &str = "save";
const SAVE_AS_ACTION_ID: &str = "save_as";
const ACTIVE_ROW_FIELD_ID: &str = "themes.active";
const SAVE_AS_NAME_FIELD_ID: &str = "themes.save_as_name";
const INSTALLED_ROW_PREFIX: &str = "themes.installed.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ThemeRowAction {
    Activate(InstalledThemeId),
    Save,
    SaveAs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ThemePageAction {
    Save,
    SaveAs,
}

pub(super) fn section_id() -> SettingsSectionId {
    SettingsSectionId::from(SECTION_ID)
}

pub(super) fn root_page_id() -> SettingsPageId {
    SettingsPageId::from(SECTION_ID)
}

pub(super) fn editor_page_id() -> SettingsPageId {
    SettingsPageId::from(EDITOR_PAGE_ID)
}

pub(super) fn save_as_name_field_id() -> SettingsFieldId {
    SettingsFieldId::from(SAVE_AS_NAME_FIELD_ID)
}

pub(super) fn has_section_id(section_id: &SettingsSectionId) -> bool {
    section_id.as_str() == SECTION_ID
}

pub(super) fn has_page_id(page_id: &SettingsPageId) -> bool {
    matches!(page_id.as_str(), SECTION_ID | EDITOR_PAGE_ID)
}

pub(super) fn settings_section(
    snapshot: &ThemeRepositorySnapshot,
    editor_model: Option<ThemeEditorPageModel>,
    errors: &HashMap<SettingsFieldId, String>,
    staged_changes: bool,
    save_as_name: &str,
) -> SettingsSection {
    SettingsSection::new(section_id(), "Themes")
        .with_root_page(root_page(snapshot, staged_changes))
        .with_page(editor_page(
            snapshot,
            editor_model,
            errors,
            staged_changes,
            save_as_name,
        ))
}

pub(super) fn row_action(
    field_id: &SettingsFieldId,
    action_id: &SettingsRowActionId,
) -> Option<ThemeRowAction> {
    if field_id.as_str() == ACTIVE_ROW_FIELD_ID {
        return match action_id.as_str() {
            SAVE_ACTION_ID => Some(ThemeRowAction::Save),
            SAVE_AS_ACTION_ID => Some(ThemeRowAction::SaveAs),
            _ => None,
        };
    }

    if action_id.as_str() == ACTIVATE_ACTION_ID {
        let id = field_id.as_str().strip_prefix(INSTALLED_ROW_PREFIX)?;
        Some(ThemeRowAction::Activate(InstalledThemeId::new(id).ok()?))
    } else {
        None
    }
}

pub(super) fn page_action(action_id: &SettingsPageActionId) -> Option<ThemePageAction> {
    match action_id.as_str() {
        SAVE_ACTION_ID => Some(ThemePageAction::Save),
        SAVE_AS_ACTION_ID => Some(ThemePageAction::SaveAs),
        _ => None,
    }
}

fn root_page(snapshot: &ThemeRepositorySnapshot, staged_changes: bool) -> SettingsPage {
    let mut page = SettingsPage::new(root_page_id(), "Themes");
    for theme in snapshot.themes() {
        if theme.is_active() {
            let mut row =
                SettingsRow::navigation(ACTIVE_ROW_FIELD_ID, theme.name(), editor_page_id())
                    .with_subtext(active_subtext(theme.is_built_in()))
                    .with_modified(staged_changes);
            for action in active_theme_actions(theme.is_built_in(), staged_changes) {
                row = row.with_action(action);
            }
            page = page.with_row(row);
        } else {
            page = page.with_row(
                SettingsRow::action_only(
                    format!("{INSTALLED_ROW_PREFIX}{}", theme.id().as_str()),
                    theme.name(),
                    SettingsRowAction::new(ACTIVATE_ACTION_ID, "Activate"),
                )
                .with_subtext(theme.id().as_str()),
            );
        }
    }
    page
}

fn active_theme_actions(built_in: bool, staged_changes: bool) -> Vec<SettingsRowAction> {
    let save_action = if staged_changes && !built_in {
        SettingsRowAction::new(SAVE_ACTION_ID, "Save")
    } else {
        SettingsRowAction::new(SAVE_ACTION_ID, "Save").disabled_with_reason(if built_in {
            "The built-in fallback theme is read-only."
        } else {
            "No staged theme changes."
        })
    };
    let save_as_action = if staged_changes {
        SettingsRowAction::new(SAVE_AS_ACTION_ID, "Save As")
    } else {
        SettingsRowAction::new(SAVE_AS_ACTION_ID, "Save As")
            .disabled_with_reason("No staged theme changes.")
    };

    vec![save_action, save_as_action]
}

fn editor_page(
    snapshot: &ThemeRepositorySnapshot,
    editor_model: Option<ThemeEditorPageModel>,
    errors: &HashMap<SettingsFieldId, String>,
    staged_changes: bool,
    save_as_name: &str,
) -> SettingsPage {
    let active_built_in = snapshot.active_theme_id().as_str() == BUILT_IN_INSTALLED_THEME_ID;
    let save_action = if staged_changes && !active_built_in {
        SettingsPageAction::new(SAVE_ACTION_ID, "Save")
            .with_priority(SettingsPageActionPriority::Primary)
    } else {
        SettingsPageAction::new(SAVE_ACTION_ID, "Save")
            .with_priority(SettingsPageActionPriority::Primary)
            .disabled_with_reason(if active_built_in {
                "The built-in fallback theme is read-only."
            } else {
                "No staged theme changes."
            })
    };
    let save_as_action = if staged_changes {
        SettingsPageAction::new(SAVE_AS_ACTION_ID, "Save As")
    } else {
        SettingsPageAction::new(SAVE_AS_ACTION_ID, "Save As")
            .disabled_with_reason("No staged theme changes.")
    };

    let mut page = SettingsPage::new(editor_page_id(), "Theme Editor")
        .with_breadcrumb_segment(SettingsBreadcrumbSegment::linked("Themes", root_page_id()))
        .with_breadcrumb_segment(SettingsBreadcrumbSegment::new("Theme Editor"))
        .with_back_target(root_page_id())
        .with_modified(staged_changes)
        .with_action(save_action)
        .with_action(save_as_action);

    if let Some(editor_model) = editor_model {
        page = page
            .with_local_split(editor_model.split)
            .with_row(save_as_name_row(save_as_name, errors));

        for row in editor_model.rows {
            page = page.with_row(row);
        }
    }

    page
}

fn save_as_name_row(save_as_name: &str, errors: &HashMap<SettingsFieldId, String>) -> SettingsRow {
    let field_id = save_as_name_field_id();
    let row = SettingsRow::new(
        field_id.clone(),
        "Save As name",
        save_as_name,
        gpui_settings_window::SettingsFieldKind::Text,
    )
    .with_subtext("Used when Save As creates a new installed theme.");
    match errors.get(&field_id) {
        Some(error) => row.with_error(error.clone()),
        None => row,
    }
}

fn active_subtext(built_in: bool) -> &'static str {
    if built_in {
        "Active built-in fallback theme"
    } else {
        "Active installed theme"
    }
}
