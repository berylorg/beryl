use std::{cell::RefCell, collections::HashMap};

#[path = "theme_editor/draft.rs"]
mod draft;
#[path = "theme_editor/field_ids.rs"]
mod field_ids;
#[path = "theme_editor/helpers.rs"]
mod helpers;
#[path = "theme_editor/rows.rs"]
mod rows;

use gpui_settings_window::{
    SettingsFieldId, SettingsPageSplit, SettingsPageSplitItemId, SettingsPageSplitItemPreviewStyle,
    SettingsRow,
};

use crate::{
    ActiveThemeProjection, StyleRoleId, ThemeDefinition, ThemeResolver, built_in_theme_schema,
};

use field_ids::{property_source_field_id, role_field_id, theme_editor_field_target};

pub(super) struct ThemeEditorPageModel {
    pub(super) split: SettingsPageSplit,
    pub(super) rows: Vec<SettingsRow>,
    pub(super) diagnostics: ThemeEditorPageModelDiagnostics,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ThemeEditorPageModelDiagnostics {
    pub(super) candidate_definition_build_count: u64,
    pub(super) last_candidate_definition_build_micros: u64,
    pub(super) preview_projection_build_count: u64,
    pub(super) last_preview_projection_build_micros: u64,
    pub(super) role_preview_style_build_count: u64,
    pub(super) role_preview_row_count: usize,
    pub(super) selected_property_detail_row_count: usize,
}

#[derive(Debug, Default)]
pub(super) struct ThemeEditorPresentationCache {
    pub(super) full_invalidated: bool,
    pub(super) projection: Option<ActiveThemeProjection>,
    pub(super) preview_styles: HashMap<StyleRoleId, SettingsPageSplitItemPreviewStyle>,
}

#[derive(Debug)]
pub(super) struct ThemeEditorDraft {
    definition: ThemeDefinition,
    values: HashMap<SettingsFieldId, String>,
    presentation_cache: RefCell<ThemeEditorPresentationCache>,
}

impl Clone for ThemeEditorDraft {
    fn clone(&self) -> Self {
        Self {
            definition: self.definition.clone(),
            values: self.values.clone(),
            presentation_cache: RefCell::new(ThemeEditorPresentationCache::default()),
        }
    }
}

impl PartialEq for ThemeEditorDraft {
    fn eq(&self, other: &Self) -> bool {
        self.definition == other.definition && self.values == other.values
    }
}

impl ThemeEditorDraft {
    pub(super) fn from_definition(definition: &ThemeDefinition) -> Self {
        Self {
            definition: definition.clone(),
            values: HashMap::new(),
            presentation_cache: RefCell::new(ThemeEditorPresentationCache {
                full_invalidated: true,
                ..ThemeEditorPresentationCache::default()
            }),
        }
    }

    pub(super) fn set_field_value(&mut self, field_id: &SettingsFieldId, value: String) -> bool {
        if theme_editor_field_target(field_id).is_none() {
            return false;
        };
        self.values.insert(field_id.clone(), value);
        true
    }

    pub(super) fn to_definition(
        &self,
    ) -> Result<ThemeDefinition, HashMap<SettingsFieldId, String>> {
        let (definition, mut errors) = self.candidate_definition(false);
        if errors.is_empty()
            && let Err(diagnostics) =
                ThemeResolver::new(built_in_theme_schema(), definition.clone())
        {
            for diagnostic in diagnostics.diagnostics() {
                let field_id = diagnostic
                    .role_id()
                    .and_then(|role_id| {
                        diagnostic
                            .property_id()
                            .map(|property_id| property_source_field_id(role_id, property_id))
                            .or_else(|| Some(role_field_id(role_id)))
                    })
                    .unwrap_or_else(|| SettingsFieldId::from("themes.editor"));
                errors.insert(field_id, diagnostic.message().to_string());
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(definition)
    }
}

pub(super) fn default_role_id() -> StyleRoleId {
    field_ids::default_role_id()
}

pub(super) fn validated_role_id(role_id: StyleRoleId) -> StyleRoleId {
    field_ids::validated_role_id(role_id)
}

pub(super) fn role_id_from_split_item(item_id: &SettingsPageSplitItemId) -> Option<StyleRoleId> {
    field_ids::role_id_from_split_item(item_id)
}

pub(super) fn is_theme_editor_field_id(field_id: &SettingsFieldId) -> bool {
    field_ids::is_theme_editor_field_id(field_id)
}
