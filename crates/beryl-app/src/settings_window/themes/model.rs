use beryl_state::{SettingKey, ThemeName};
use gpui_settings_window::{
    SettingsBreadcrumbSegment, SettingsFieldAvailability, SettingsFieldKind, SettingsPage,
    SettingsPageAction, SettingsPageCustomBody, SettingsRow, SettingsRowAction, SettingsSection,
};

use super::*;
use crate::settings_service::SettingsDraft;

impl ThemesState {
    pub(in crate::settings_window) fn editor_source_is_current(&self) -> bool {
        !self.repository_unavailable
            && self
                .editor
                .as_ref()
                .zip(self.catalog.as_ref())
                .is_some_and(|(editor, catalog)| {
                    editor.saved_identity.is_none()
                        && editor.baseline.identity().manifest() == catalog.observation.manifest()
                })
    }
    pub(in crate::settings_window) fn section(
        &self,
        active: SettingsRow,
        draft: Option<&SettingsDraft>,
        window: u64,
        available: bool,
        appearance_failed: bool,
    ) -> SettingsSection {
        let revision = draft
            .map(|draft| draft.binding().draft_revision)
            .unwrap_or(0);
        let mut page = SettingsPage::new("themes", "Themes").with_row(active);
        if appearance_failed {
            page = page.with_row(super::super::presentation::appearance_notice());
        }
        if let Some(source) = self.split_source(window, revision) {
            page = page.with_paged_split_source(source);
        }
        if let Some(selected) = &self.selected {
            let is_active = draft.is_some_and(|draft| {
                draft.baseline().row(SettingKey::ActiveThemeId).value == selected.id().as_str()
            });
            let is_staged = draft.is_some_and(|draft| {
                draft.value(SettingKey::ActiveThemeId) == selected.id().as_str()
            });
            page = page
                .with_row(
                    SettingsRow::new(
                        "theme.selected.id",
                        "Installed theme ID",
                        selected.id().as_str(),
                        SettingsFieldKind::Text,
                    )
                    .with_value_byte_limit(Some(64))
                    .with_availability(SettingsFieldAvailability::unavailable(
                        "Installed theme identities are read-only.",
                    ))
                    .with_action(SettingsRowAction::new("theme.copy-id", "Copy ID")),
                )
                .with_row(
                    SettingsRow::new(
                        "theme.rename-name",
                        "Name",
                        &self.rename_name,
                        SettingsFieldKind::Text,
                    )
                    .with_value_byte_limit(Some(128))
                    .with_availability(
                        if available && !self.repository_unavailable {
                            SettingsFieldAvailability::Available
                        } else {
                            SettingsFieldAvailability::unavailable(
                                "This exact theme source is unavailable.",
                            )
                        },
                    ),
                );
            if !is_staged {
                let action = SettingsPageAction::new("theme.activate", "Activate");
                page = page.with_action(
                    if available
                        && self.document.as_ref().is_some_and(|document| {
                            document.identity().manifest() == selected.manifest()
                                && document.identity().theme_id() == selected.id()
                        })
                    {
                        action
                    } else {
                        action.disabled_with_reason(
                            "Load and validate this exact installed theme before activating it.",
                        )
                    },
                );
            }
            let rename = SettingsPageAction::new("theme.rename", "Rename");
            page = page.with_action(
                if available
                    && !self.repository_unavailable
                    && ThemeName::new(&self.rename_name).is_ok()
                    && self.rename_name.trim() != selected.summary().name().as_str()
                {
                    rename
                } else {
                    rename.disabled_with_reason("Enter a different valid theme name.")
                },
            );
            let delete = SettingsPageAction::new("theme.delete", "Delete");
            page = page.with_action(if !available || self.repository_unavailable {
                delete.disabled_with_reason("This exact theme source is unavailable.")
            } else if is_active || is_staged {
                delete.disabled_with_reason("The theme is referenced by active or staged Settings.")
            } else if self.document.is_none() {
                delete.disabled_with_reason("The selected document is not available yet.")
            } else {
                delete
            });
            if is_active {
                let edit = SettingsPageAction::new("theme.edit", "Edit");
                page = page.with_action(
                    if available && self.document.is_some() && !self.repository_unavailable {
                        edit
                    } else {
                        edit.disabled_with_reason("The selected document is not available yet.")
                    },
                );
            }
            if is_active
                && self.editor.as_ref().is_some_and(|editor| {
                    editor.is_dirty() && editor.baseline.identity().theme_id() == selected.id()
                })
            {
                let valid = available
                    && self.editor_source_is_current()
                    && self
                        .editor
                        .as_ref()
                        .is_some_and(|editor| editor.can_save() && editor.saved_identity.is_none());
                let save = SettingsRowAction::new("theme.save", "Save");
                let save_as = SettingsRowAction::new("theme.save-as", "Save As");
                page = page.with_row(
                    SettingsRow::action_only(
                        "theme.document.commands",
                        "Theme document",
                        if valid {
                            save
                        } else {
                            save.disabled_with_reason(
                                "Validate the complete current document first.",
                            )
                        },
                    )
                    .with_action(
                        if valid
                            && self
                                .editor
                                .as_ref()
                                .is_some_and(|editor| ThemeName::new(&editor.save_as_name).is_ok())
                        {
                            save_as
                        } else {
                            save_as.disabled_with_reason(
                                "Enter a valid Save As name in the theme editor.",
                            )
                        },
                    ),
                );
            }
        }
        if self.message.is_some() || self.catalog.is_none() {
            page = page.with_stacked_custom_body(SettingsPageCustomBody::new("theme-feedback", 64));
        }
        if self.read_failed {
            page = page.with_action(SettingsPageAction::new("theme.retry", "Retry"));
        }
        let mut editor_page = SettingsPage::new(
            "theme-editor",
            self.editor
                .as_ref()
                .and_then(|editor| {
                    self.selected
                        .as_ref()
                        .filter(|selection| selection.id() == editor.baseline.identity().theme_id())
                        .map(|selection| selection.summary().name().as_str())
                        .or_else(|| editor.baseline.document().name())
                })
                .unwrap_or("Theme Editor"),
        )
        .with_back_target("themes")
        .with_breadcrumb_segment(SettingsBreadcrumbSegment::linked("Themes", "themes"));
        if let Some(editor) = &self.editor {
            let source_current = self.editor_source_is_current();
            let unavailable = if !available || self.repository_unavailable {
                Some("This exact theme-document source is unavailable.")
            } else if !source_current {
                Some(
                    "Authenticating the original document under the current theme collection before another change.",
                )
            } else {
                None
            };
            if self.read_failed {
                editor_page = editor_page.with_row(
                    SettingsRow::action_only(
                        "theme.source.retry",
                        "Theme source refresh needs attention",
                        SettingsRowAction::new("theme.retry", "Retry"),
                    )
                    .with_subtext(self.message.unwrap_or(
                        "The exact source is unavailable. Retry to load a current source.",
                    )),
                );
            }
            if appearance_failed {
                editor_page = editor_page.with_row(super::super::presentation::appearance_notice());
            }
            editor_page = editor_page
                .with_stacked_custom_body(SettingsPageCustomBody::new("theme-role-navigator", 156));
            for row in editor.rows(unavailable) {
                editor_page = editor_page.with_row(row);
            }
            let save = SettingsPageAction::new("theme.save", "Save");
            if editor.is_dirty() {
                editor_page =
                    editor_page.with_action(if available && editor.can_save() && source_current {
                        save
                    } else {
                        save.disabled_with_reason(unavailable.unwrap_or(
                            "A changed, valid document with its exact current source is required.",
                        ))
                    });
            }
            let mut name_row = SettingsRow::new(
                "theme.save-as-name",
                "Save As name",
                &editor.save_as_name,
                SettingsFieldKind::Text,
            )
            .with_value_byte_limit(Some(128))
            .with_availability(if unavailable.is_none() {
                SettingsFieldAvailability::Available
            } else {
                SettingsFieldAvailability::unavailable(unavailable.unwrap())
            });
            if let Some((field, feedback)) = &editor.feedback {
                if field == "theme.save-as-name" {
                    name_row = name_row.with_error(*feedback);
                }
            }
            editor_page = editor_page.with_row(name_row);
            let save_as = SettingsPageAction::new("theme.save-as", "Save As");
            if editor.is_dirty() {
                editor_page = editor_page.with_action(
                    if available
                        && editor.can_save()
                        && ThemeName::new(&editor.save_as_name).is_ok()
                        && source_current
                    {
                        save_as
                    } else {
                        save_as.disabled_with_reason(unavailable.unwrap_or(
                            "Enter a valid new name and validate the complete changed document.",
                        ))
                    },
                );
            }
        }
        SettingsSection::new("themes", "Themes")
            .with_root_page(page)
            .with_page(editor_page)
    }
}
