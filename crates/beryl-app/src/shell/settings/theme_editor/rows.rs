use std::{collections::HashMap, time::Instant};

use gpui_settings_window::{
    SettingsChoiceOption, SettingsFieldId, SettingsFieldKind, SettingsPageSplit,
    SettingsPageSplitItem, SettingsPageSplitItemPreviewStyle, SettingsRow, SettingsRowDetailField,
};

use crate::{
    ActiveThemeProjection, StylePropertyId, StylePropertyKind, StylePropertySource, StyleRoleId,
};

use super::{
    ThemeEditorDraft, ThemeEditorPageModel, ThemeEditorPageModelDiagnostics,
    field_ids::{property_field_id, property_source_field_id, validated_role_id},
    helpers::{
        PropertySourceChoice, editable_theme_roles, field_kind, preview_style,
        projection_from_definition, property_label, property_value_text, role_schema,
        source_choices, style_value_text,
    },
};

impl ThemeEditorDraft {
    pub(crate) fn page_model(
        &self,
        selected_role_id: &StyleRoleId,
        errors: &HashMap<SettingsFieldId, String>,
    ) -> ThemeEditorPageModel {
        let selected_role_id = validated_role_id(selected_role_id.clone());
        let mut diagnostics = ThemeEditorPageModelDiagnostics::default();
        let mut cache = self.presentation_cache.borrow_mut();
        if cache.full_invalidated || cache.projection.is_none() {
            let projection_started = Instant::now();
            let preview_projection = projection_from_definition(&self.definition)
                .unwrap_or_else(ActiveThemeProjection::built_in);
            let projection_micros = projection_started.elapsed().as_micros();

            cache.preview_styles = editable_theme_roles()
                .map(|role| {
                    let role_id = StyleRoleId::from(role.id());
                    let preview = preview_style(&preview_projection, &role_id);
                    (role_id, preview)
                })
                .collect();
            cache.projection = Some(preview_projection);
            cache.full_invalidated = false;

            diagnostics.preview_projection_build_count = 1;
            diagnostics.last_preview_projection_build_micros =
                projection_micros.min(u128::from(u64::MAX)) as u64;
            diagnostics.role_preview_style_build_count = cache.preview_styles.len() as u64;
        }

        let preview_projection = cache
            .projection
            .as_ref()
            .expect("theme editor presentation cache is initialized before row projection");
        let split = self.role_split(&selected_role_id, &cache.preview_styles);
        let rows = self.property_rows(&selected_role_id, preview_projection, errors);
        diagnostics.role_preview_row_count = split.items().len();
        diagnostics.selected_property_detail_row_count = rows.len();

        ThemeEditorPageModel {
            diagnostics,
            split,
            rows,
        }
    }

    fn role_split(
        &self,
        selected_role_id: &StyleRoleId,
        preview_styles: &HashMap<StyleRoleId, SettingsPageSplitItemPreviewStyle>,
    ) -> SettingsPageSplit {
        editable_theme_roles().fold(SettingsPageSplit::new(), |split, role| {
            let role_id = StyleRoleId::from(role.id());
            let item = SettingsPageSplitItem::new(role.id(), role.id())
                .with_selected(&role_id == selected_role_id);
            let item = if let Some(preview) = preview_styles.get(&role_id).cloned() {
                item.with_preview_style(preview)
            } else {
                item
            };
            split.with_item(item)
        })
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
        let static_parent = self.effective_static_parent(role_id);
        let row = source_choices(static_parent.as_ref()).into_iter().fold(
            SettingsRow::new(
                field_id.clone(),
                property_label(property_id),
                source.as_str(),
                SettingsFieldKind::Choice,
            ),
            |row, (choice, label)| {
                row.with_choice(SettingsChoiceOption::new(choice.as_str(), label))
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
}
