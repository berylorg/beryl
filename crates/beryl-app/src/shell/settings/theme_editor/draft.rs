use std::collections::{BTreeSet, HashMap};

use gpui_settings_window::SettingsFieldId;

use crate::{
    ActiveThemeProjection, StylePropertyId, StylePropertyKind, StylePropertySource,
    StylePropertyValue, StyleRoleId, ThemeDefinition, ThemeRoleDefinition, ThemeRoleSchema,
    built_in_theme_schema,
};

use super::{
    ThemeEditorDraft,
    field_ids::{
        ThemeEditorFieldTarget, property_field_id, property_source_field_id,
        theme_editor_field_target,
    },
    helpers::{
        PropertySourceChoice, projection_from_definition, property_label, property_value_text,
        role_schema, style_value_text, validate_property_value,
    },
};

impl ThemeEditorDraft {
    pub(crate) fn is_modified_from(&self, baseline: &ThemeDefinition) -> bool {
        if self.definition != *baseline {
            return true;
        }

        let mut changed_properties = BTreeSet::new();
        for field_id in self.values.keys() {
            match theme_editor_field_target(field_id) {
                Some(
                    ThemeEditorFieldTarget::PropertyValue {
                        role_id,
                        property_id,
                    }
                    | ThemeEditorFieldTarget::PropertySource {
                        role_id,
                        property_id,
                    },
                ) => {
                    changed_properties.insert((role_id, property_id));
                }
                None => return true,
            }
        }

        let mut errors = HashMap::new();
        for (role_id, property_id) in changed_properties {
            let Some(kind) = role_schema(&role_id).and_then(|schema| {
                schema
                    .properties()
                    .get(&property_id)
                    .map(|property| property.kind())
            }) else {
                return true;
            };
            let candidate =
                self.candidate_property_source(&role_id, &property_id, kind, false, &mut errors);
            if !errors.is_empty() {
                return true;
            }
            if candidate != definition_source(baseline, &role_id, &property_id) {
                return true;
            }
        }

        false
    }

    pub(super) fn effective_static_parent(&self, role_id: &StyleRoleId) -> Option<StyleRoleId> {
        self.definition_role(role_id)
            .and_then(|role| role.static_parent().cloned())
            .or_else(|| role_schema(role_id).and_then(|schema| schema.static_parent().cloned()))
    }

    pub(super) fn candidate_definition(
        &self,
        ignore_invalid: bool,
    ) -> (ThemeDefinition, HashMap<SettingsFieldId, String>) {
        let mut errors = HashMap::new();
        let roles = built_in_theme_schema()
            .roles()
            .iter()
            .filter_map(|schema_role| {
                self.candidate_role_definition(schema_role, ignore_invalid, &mut errors)
            })
            .collect();
        (ThemeDefinition::new(roles), errors)
    }

    fn candidate_role_definition(
        &self,
        schema_role: &ThemeRoleSchema,
        ignore_invalid: bool,
        errors: &mut HashMap<SettingsFieldId, String>,
    ) -> Option<ThemeRoleDefinition> {
        let role_id = schema_role.role_id().clone();
        let existing_role = self.definition_role(&role_id);
        let mut next = ThemeRoleDefinition::new(role_id.clone());
        let mut has_content = false;

        if let Some(parent) = self.candidate_static_parent(&role_id) {
            next = next.with_static_parent(parent);
            has_content = true;
        }

        for (property_id, property_schema) in schema_role.properties() {
            let Some(source) = self.candidate_property_source(
                &role_id,
                property_id,
                property_schema.kind(),
                ignore_invalid,
                errors,
            ) else {
                continue;
            };
            next = next.with_property(property_id.clone(), source);
            has_content = true;
        }

        if !has_content && existing_role.is_some() {
            return Some(next);
        }

        has_content.then_some(next)
    }

    fn candidate_static_parent(&self, role_id: &StyleRoleId) -> Option<StyleRoleId> {
        self.definition_role(role_id)
            .and_then(|role| role.static_parent().cloned())
    }

    fn candidate_property_source(
        &self,
        role_id: &StyleRoleId,
        property_id: &StylePropertyId,
        property_kind: StylePropertyKind,
        ignore_invalid: bool,
        errors: &mut HashMap<SettingsFieldId, String>,
    ) -> Option<StylePropertySource> {
        let source_field_id = property_source_field_id(role_id, property_id);
        let value_field_id = property_field_id(role_id, property_id);
        let existing_source = self.definition_source(role_id, property_id);
        let value_was_edited = self.values.contains_key(&value_field_id);
        let source_choice = match self.values.get(&source_field_id) {
            Some(value) => match PropertySourceChoice::parse(value.trim()) {
                Some(choice) => choice,
                None if ignore_invalid => return existing_source,
                None => {
                    errors.insert(
                        source_field_id,
                        format!(
                            "{} source is not a supported option.",
                            property_label(property_id)
                        ),
                    );
                    return existing_source;
                }
            },
            None if value_was_edited => PropertySourceChoice::Value,
            None => return existing_source,
        };

        match source_choice {
            PropertySourceChoice::Value => self
                .candidate_concrete_property_value(
                    role_id,
                    property_id,
                    property_kind,
                    ignore_invalid,
                    errors,
                )
                .map(StylePropertySource::Concrete),
            PropertySourceChoice::StaticParent => Some(StylePropertySource::StaticParent),
            PropertySourceChoice::AmbientParent => Some(StylePropertySource::AmbientParent),
            PropertySourceChoice::Fallback => Some(StylePropertySource::Fallback),
        }
    }

    fn candidate_concrete_property_value(
        &self,
        role_id: &StyleRoleId,
        property_id: &StylePropertyId,
        property_kind: StylePropertyKind,
        ignore_invalid: bool,
        errors: &mut HashMap<SettingsFieldId, String>,
    ) -> Option<StylePropertyValue> {
        let field_id = property_field_id(role_id, property_id);
        let value = self.values.get(&field_id).cloned().unwrap_or_else(|| {
            match self.definition_source(role_id, property_id) {
                Some(StylePropertySource::Concrete(value)) => style_value_text(&value),
                _ => property_value_text(
                    &projection_from_definition(&self.definition)
                        .unwrap_or_else(ActiveThemeProjection::built_in),
                    role_id,
                    property_id,
                    property_kind,
                ),
            }
        });

        match validate_property_value(property_id, property_kind, &value) {
            Ok(value) => Some(value),
            Err(_) if ignore_invalid => {
                self.definition_source(role_id, property_id)
                    .and_then(|source| match source {
                        StylePropertySource::Concrete(value) => Some(value),
                        _ => None,
                    })
            }
            Err(error) => {
                errors.insert(field_id, error);
                None
            }
        }
    }

    pub(crate) fn definition_role(&self, role_id: &StyleRoleId) -> Option<&ThemeRoleDefinition> {
        self.definition
            .roles()
            .iter()
            .find(|role| role.role_id() == role_id)
    }

    pub(crate) fn definition_source(
        &self,
        role_id: &StyleRoleId,
        property_id: &StylePropertyId,
    ) -> Option<StylePropertySource> {
        self.definition
            .roles()
            .iter()
            .find(|role| role.role_id() == role_id)
            .and_then(|role| role.properties().get(property_id))
            .cloned()
    }
}

fn definition_source(
    definition: &ThemeDefinition,
    role_id: &StyleRoleId,
    property_id: &StylePropertyId,
) -> Option<StylePropertySource> {
    definition
        .roles()
        .iter()
        .find(|role| role.role_id() == role_id)
        .and_then(|role| role.properties().get(property_id))
        .cloned()
}
