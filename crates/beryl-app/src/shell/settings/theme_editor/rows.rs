use std::collections::HashMap;

use gpui_settings_window::{
    SettingsChoiceOption, SettingsFieldId, SettingsFieldKind, SettingsPageSplit,
    SettingsPageSplitItem, SettingsRow, SettingsRowDetailField,
};

use crate::{
    ActiveThemeProjection, BerylThemeRole, StylePropertyId, StylePropertyKind, StylePropertySource,
    StyleRoleId,
};

use super::{
    ThemeEditorDraft, ThemeEditorPageModel,
    field_ids::{property_field_id, property_source_field_id, validated_role_id},
    helpers::{
        PropertySourceChoice, field_kind, preview_style, projection_from_definition,
        property_label, property_value_text, role_schema, source_choices, style_value_text,
    },
};

impl ThemeEditorDraft {
    pub(crate) fn page_model(
        &self,
        selected_role_id: &StyleRoleId,
        errors: &HashMap<SettingsFieldId, String>,
    ) -> ThemeEditorPageModel {
        let selected_role_id = validated_role_id(selected_role_id.clone());
        let preview_definition = self.preview_definition();
        let preview_projection = projection_from_definition(&preview_definition)
            .unwrap_or_else(ActiveThemeProjection::built_in);

        ThemeEditorPageModel {
            split: self.role_split(&selected_role_id, &preview_projection),
            rows: self.property_rows(&selected_role_id, &preview_projection, errors),
        }
    }

    fn role_split(
        &self,
        selected_role_id: &StyleRoleId,
        projection: &ActiveThemeProjection,
    ) -> SettingsPageSplit {
        BerylThemeRole::ALL
            .iter()
            .copied()
            .fold(SettingsPageSplit::new(), |split, role| {
                let role_id = StyleRoleId::from(role.id());
                split.with_item(
                    SettingsPageSplitItem::new(role.id(), role.id())
                        .with_subtext(self.role_static_parent_subtext(&role_id))
                        .with_selected(&role_id == selected_role_id)
                        .with_preview_style(preview_style(projection, &role_id)),
                )
            })
    }

    fn role_static_parent_subtext(&self, role_id: &StyleRoleId) -> String {
        let parent = self.static_parent_text(role_id);
        if parent.is_empty() {
            "static parent: none".to_string()
        } else {
            format!("static parent: {parent}")
        }
    }

    fn property_rows(
        &self,
        selected_role_id: &StyleRoleId,
        projection: &ActiveThemeProjection,
        errors: &HashMap<SettingsFieldId, String>,
    ) -> Vec<SettingsRow> {
        let Some(role_schema) = role_schema(selected_role_id) else {
            return Vec::new();
        };

        let mut rows = Vec::new();
        for (property_id, property_schema) in role_schema.properties() {
            rows.push(self.property_row(
                selected_role_id,
                property_id,
                property_schema.kind(),
                projection,
                errors,
            ));
        }
        rows
    }

    fn property_row(
        &self,
        role_id: &StyleRoleId,
        property_id: &StylePropertyId,
        kind: StylePropertyKind,
        projection: &ActiveThemeProjection,
        errors: &HashMap<SettingsFieldId, String>,
    ) -> SettingsRow {
        let field_id = property_source_field_id(role_id, property_id);
        let source = self.property_source_choice(role_id, property_id);
        let row = source_choices().into_iter().fold(
            SettingsRow::new(
                field_id.clone(),
                property_label(property_id),
                source.as_str(),
                SettingsFieldKind::Choice,
            ),
            |row, choice| {
                row.with_choice(SettingsChoiceOption::new(choice.as_str(), choice.label()))
            },
        );
        let value_field_id = property_field_id(role_id, property_id);
        let row = if source == PropertySourceChoice::Value {
            let detail = SettingsRowDetailField::new(
                value_field_id.clone(),
                self.concrete_value_text(role_id, property_id, kind, projection),
                field_kind(kind),
            );
            let detail = if self.values.contains_key(&value_field_id) {
                detail.with_modified(true)
            } else {
                detail
            };
            let detail = match errors.get(&value_field_id) {
                Some(error) => detail.with_error(error.clone()),
                None => detail,
            };
            row.with_detail_field(detail)
        } else {
            row
        };
        let row =
            if self.values.contains_key(&field_id) || self.values.contains_key(&value_field_id) {
                row.with_modified(true)
            } else {
                row
            };
        match errors.get(&field_id) {
            Some(error) => row.with_error(error.clone()),
            None => row,
        }
    }

    fn concrete_value_text(
        &self,
        role_id: &StyleRoleId,
        property_id: &StylePropertyId,
        kind: StylePropertyKind,
        projection: &ActiveThemeProjection,
    ) -> String {
        let field_id = property_field_id(role_id, property_id);
        self.values.get(&field_id).cloned().unwrap_or_else(|| {
            match self.definition_source(role_id, property_id) {
                Some(StylePropertySource::Concrete(value)) => style_value_text(&value),
                _ => property_value_text(projection, role_id, property_id, kind),
            }
        })
    }

    fn property_source_choice(
        &self,
        role_id: &StyleRoleId,
        property_id: &StylePropertyId,
    ) -> PropertySourceChoice {
        let field_id = property_source_field_id(role_id, property_id);
        self.values
            .get(&field_id)
            .and_then(|value| PropertySourceChoice::parse(value.trim()))
            .unwrap_or_else(|| {
                PropertySourceChoice::from_source(
                    self.definition_source(role_id, property_id).as_ref(),
                )
            })
    }

    fn static_parent_text(&self, role_id: &StyleRoleId) -> String {
        self.definition_role(role_id)
            .and_then(|role| role.static_parent().cloned())
            .or_else(|| role_schema(role_id).and_then(|schema| schema.static_parent().cloned()))
            .map(|parent| parent.to_string())
            .unwrap_or_default()
    }
}
