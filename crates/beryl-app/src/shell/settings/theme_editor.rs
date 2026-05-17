use std::collections::HashMap;

#[path = "theme_editor/draft.rs"]
mod draft;
#[path = "theme_editor/field_ids.rs"]
mod field_ids;
#[path = "theme_editor/helpers.rs"]
mod helpers;
#[path = "theme_editor/rows.rs"]
mod rows;

use gpui_settings_window::{
    SettingsFieldId, SettingsPageSplit, SettingsPageSplitItemId, SettingsRow,
};

use crate::{StyleRoleId, ThemeDefinition, ThemeResolver, built_in_theme_schema};

use field_ids::{property_source_field_id, role_field_id};

pub(super) struct ThemeEditorPageModel {
    pub(super) split: SettingsPageSplit,
    pub(super) rows: Vec<SettingsRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ThemeEditorDraft {
    definition: ThemeDefinition,
    values: HashMap<SettingsFieldId, String>,
}

impl ThemeEditorDraft {
    pub(super) fn from_definition(definition: &ThemeDefinition) -> Self {
        Self {
            definition: definition.clone(),
            values: HashMap::new(),
        }
    }

    pub(super) fn set_field_value(&mut self, field_id: &SettingsFieldId, value: String) -> bool {
        if !is_theme_editor_field_id(field_id) {
            return false;
        }
        self.values.insert(field_id.clone(), value);
        true
    }

    pub(super) fn is_modified_from(&self, baseline: &ThemeDefinition) -> bool {
        let (definition, errors) = self.candidate_definition(false);
        definition != *baseline || !errors.is_empty()
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
