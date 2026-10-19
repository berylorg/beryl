use std::collections::HashMap;

use gpui_settings_window::{
    SettingsBreadcrumbSegment, SettingsFieldId, SettingsPage, SettingsPageAction,
    SettingsPageActionId, SettingsPageActionPriority, SettingsPageCustomBody, SettingsPageId,
    SettingsRow, SettingsRowAction, SettingsRowActionId, SettingsSection, SettingsSectionId,
};

use crate::{InstalledThemeId, ThemeRepositorySnapshot};

use super::theme_editor::ThemeEditorPageModel;

pub(super) const SECTION_ID: &str = "themes";
pub(super) const EDITOR_PAGE_ID: &str = "themes.editor";
const ACTIVATE_ACTION_ID: &str = "activate";
const SAVE_ACTION_ID: &str = "save";
const SAVE_AS_ACTION_ID: &str = "save_as";
const SAVE_AS_NAME_FIELD_ID: &str = "themes.save_as_name";
const INSTALLED_ROW_PREFIX: &str = "themes.installed.";
const EDITOR_ROLE_NAVIGATOR_BODY_ID: &str = "themes.editor.role_navigator";
const EDITOR_ROLE_NAVIGATOR_BODY_HEIGHT: u16 = 156;

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
        .with_root_page(root_page(snapshot))
        .with_page(editor_page(
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

fn root_page(snapshot: &ThemeRepositorySnapshot) -> SettingsPage {
    let mut page = SettingsPage::new(root_page_id(), "Themes");
    for theme in snapshot.themes() {
        page = page.with_row(
            SettingsRow::action_only(
                format!("{INSTALLED_ROW_PREFIX}{}", theme.id().as_str()),
                theme.name(),
                SettingsRowAction::new(ACTIVATE_ACTION_ID, "Activate"),
            )
            .with_subtext(theme.id().as_str()),
        );
    }
    page
}

fn editor_page(
    editor_model: Option<ThemeEditorPageModel>,
    errors: &HashMap<SettingsFieldId, String>,
    staged_changes: bool,
    save_as_name: &str,
) -> SettingsPage {
    let save_action = SettingsPageAction::new(SAVE_ACTION_ID, "Save")
        .with_priority(SettingsPageActionPriority::Primary)
        .disabled_with_reason("Theme selection awaits the typed Beryl settings service.");
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
            .with_stacked_custom_body(SettingsPageCustomBody::new(
                EDITOR_ROLE_NAVIGATOR_BODY_ID,
                EDITOR_ROLE_NAVIGATOR_BODY_HEIGHT,
            ))
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
