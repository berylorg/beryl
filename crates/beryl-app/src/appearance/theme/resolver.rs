use std::{
    collections::{BTreeMap, BTreeSet},
    hash::{Hash, Hasher},
};

use thiserror::Error;

use super::{
    diagnostics::{ThemeDiagnostic, ThemeDiagnosticKind, ThemeValidationDiagnostics},
    model::{
        ResolvedStyle, StylePropertyId, StylePropertySource, StylePropertyValue, StyleRoleId,
        ThemeDefinition, ThemeResolutionContext, ThemeRoleDefinition, ThemeRoleSchema, ThemeSchema,
    },
};

#[derive(Clone, Debug)]
pub struct ThemeResolver {
    schema_roles: BTreeMap<StyleRoleId, ThemeRoleSchema>,
    theme_roles: BTreeMap<StyleRoleId, ThemeRoleDefinition>,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ThemeResolutionError {
    #[error("unknown theme role {role_id}")]
    UnknownRole { role_id: StyleRoleId },
    #[error("unknown theme property {property_id} on role {role_id}")]
    UnknownProperty {
        role_id: StyleRoleId,
        property_id: StylePropertyId,
    },
}

impl ThemeResolver {
    pub fn new(
        schema: ThemeSchema,
        definition: ThemeDefinition,
    ) -> Result<Self, ThemeValidationDiagnostics> {
        let mut diagnostics = ThemeValidationDiagnostics::new();
        let schema_roles = normalized_schema_roles(schema, &mut diagnostics);
        let theme_roles = normalized_theme_roles(definition, &schema_roles, &mut diagnostics);

        validate_static_parents(&schema_roles, &theme_roles, &mut diagnostics);
        validate_static_parent_cycles(&schema_roles, &theme_roles, &mut diagnostics);

        if diagnostics.is_empty() {
            Ok(Self {
                schema_roles,
                theme_roles,
            })
        } else {
            Err(diagnostics)
        }
    }

    pub fn resolve_property(
        &self,
        role_id: impl Into<StyleRoleId>,
        property_id: impl Into<StylePropertyId>,
        context: &ThemeResolutionContext,
    ) -> Result<StylePropertyValue, ThemeResolutionError> {
        let role_id = role_id.into();
        let property_id = property_id.into();
        self.require_property(&role_id, &property_id)?;

        Ok(self
            .resolve_known_property(&role_id, &property_id, context, &mut BTreeSet::new())
            .expect("known schema property must have a fallback"))
    }

    pub fn resolve_style(
        &self,
        role_id: impl Into<StyleRoleId>,
        context: &ThemeResolutionContext,
    ) -> Result<ResolvedStyle, ThemeResolutionError> {
        let role_id = role_id.into();
        let schema_role =
            self.schema_roles
                .get(&role_id)
                .ok_or_else(|| ThemeResolutionError::UnknownRole {
                    role_id: role_id.clone(),
                })?;
        let mut style = ResolvedStyle::new();

        for property_id in schema_role.properties.keys() {
            let value = self
                .resolve_known_property(&role_id, property_id, context, &mut BTreeSet::new())
                .expect("known schema property must have a fallback");
            style.properties.insert(property_id.clone(), value);
        }

        Ok(style)
    }

    pub(crate) fn hash_semantics(&self, state: &mut impl Hasher) {
        "schema_roles".hash(state);
        for (role_id, role) in &self.schema_roles {
            role_id.hash(state);
            role.role_id.hash(state);
            role.static_parent.hash(state);
            for (property_id, property) in &role.properties {
                property_id.hash(state);
                hash_property_kind(property.kind, state);
                hash_property_value(&property.fallback, state);
            }
        }

        "theme_roles".hash(state);
        for (role_id, role) in &self.theme_roles {
            role_id.hash(state);
            role.role_id.hash(state);
            role.static_parent.hash(state);
            for (property_id, source) in &role.properties {
                property_id.hash(state);
                hash_property_source(source, state);
            }
        }
    }

    fn require_property(
        &self,
        role_id: &StyleRoleId,
        property_id: &StylePropertyId,
    ) -> Result<(), ThemeResolutionError> {
        let schema_role =
            self.schema_roles
                .get(role_id)
                .ok_or_else(|| ThemeResolutionError::UnknownRole {
                    role_id: role_id.clone(),
                })?;
        if !schema_role.supports_property(property_id) {
            return Err(ThemeResolutionError::UnknownProperty {
                role_id: role_id.clone(),
                property_id: property_id.clone(),
            });
        }

        Ok(())
    }

    fn resolve_known_property(
        &self,
        role_id: &StyleRoleId,
        property_id: &StylePropertyId,
        context: &ThemeResolutionContext,
        resolving: &mut BTreeSet<(StyleRoleId, StylePropertyId)>,
    ) -> Option<StylePropertyValue> {
        if !resolving.insert((role_id.clone(), property_id.clone())) {
            return self.fallback_value(role_id, property_id);
        }

        let value = match self.theme_source(role_id, property_id) {
            Some(StylePropertySource::Concrete(value)) => Some(value.clone()),
            Some(StylePropertySource::StaticParent) => self
                .effective_static_parent(role_id)
                .and_then(|parent_id| {
                    self.resolve_known_property(parent_id, property_id, context, resolving)
                })
                .or_else(|| self.fallback_value(role_id, property_id)),
            Some(StylePropertySource::AmbientParent) => self
                .ambient_value(role_id, property_id, context)
                .or_else(|| self.fallback_value(role_id, property_id)),
            Some(StylePropertySource::Fallback) | None => self.fallback_value(role_id, property_id),
        };

        resolving.remove(&(role_id.clone(), property_id.clone()));
        value
    }

    fn theme_source(
        &self,
        role_id: &StyleRoleId,
        property_id: &StylePropertyId,
    ) -> Option<&StylePropertySource> {
        self.theme_roles
            .get(role_id)
            .and_then(|role| role.properties.get(property_id))
    }

    fn ambient_value(
        &self,
        role_id: &StyleRoleId,
        property_id: &StylePropertyId,
        context: &ThemeResolutionContext,
    ) -> Option<StylePropertyValue> {
        let kind = self
            .schema_roles
            .get(role_id)?
            .property_schema(property_id)?
            .kind;
        context
            .ambient_parent()
            .and_then(|style| style.property(property_id))
            .and_then(|value| value.normalized_for_kind(kind))
    }

    fn fallback_value(
        &self,
        role_id: &StyleRoleId,
        property_id: &StylePropertyId,
    ) -> Option<StylePropertyValue> {
        self.schema_roles
            .get(role_id)?
            .property_schema(property_id)
            .map(|property| property.fallback.clone())
    }

    fn effective_static_parent(&self, role_id: &StyleRoleId) -> Option<&StyleRoleId> {
        self.theme_roles
            .get(role_id)
            .and_then(|role| role.static_parent.as_ref())
            .or_else(|| {
                self.schema_roles
                    .get(role_id)
                    .and_then(|role| role.static_parent.as_ref())
            })
    }
}

fn hash_property_kind(kind: super::model::StylePropertyKind, state: &mut impl Hasher) {
    match kind {
        super::model::StylePropertyKind::Color => "color",
        super::model::StylePropertyKind::FontFamily => "font_family",
        super::model::StylePropertyKind::LogicalPixels => "logical_pixels",
        super::model::StylePropertyKind::FontWeight => "font_weight",
    }
    .hash(state);
}

fn hash_property_source(source: &StylePropertySource, state: &mut impl Hasher) {
    match source {
        StylePropertySource::Concrete(value) => {
            "concrete".hash(state);
            hash_property_value(value, state);
        }
        StylePropertySource::StaticParent => "static_parent".hash(state),
        StylePropertySource::AmbientParent => "ambient_parent".hash(state),
        StylePropertySource::Fallback => "fallback".hash(state),
    }
}

fn hash_property_value(value: &StylePropertyValue, state: &mut impl Hasher) {
    match value {
        StylePropertyValue::Color(value) => {
            "color".hash(state);
            value.hash(state);
        }
        StylePropertyValue::FontFamily(value) => {
            "font_family".hash(state);
            value.hash(state);
        }
        StylePropertyValue::LogicalPixels(value) => {
            "logical_pixels".hash(state);
            value.to_bits().hash(state);
        }
        StylePropertyValue::FontWeight(value) => {
            "font_weight".hash(state);
            value.hash(state);
        }
    }
}

fn normalized_schema_roles(
    schema: ThemeSchema,
    diagnostics: &mut ThemeValidationDiagnostics,
) -> BTreeMap<StyleRoleId, ThemeRoleSchema> {
    let mut roles = BTreeMap::new();

    for role in schema.roles {
        if roles.contains_key(&role.role_id) {
            diagnostics.push(ThemeDiagnostic::new(
                ThemeDiagnosticKind::DuplicateRole,
                Some(role.role_id.clone()),
                None,
                format!(
                    "theme schema role {} is defined more than once",
                    role.role_id
                ),
            ));
            continue;
        }

        let mut properties = BTreeMap::new();
        for (property_id, property) in role.properties {
            match property.normalized() {
                Some(normalized) => {
                    properties.insert(property_id, normalized);
                }
                None => {
                    diagnostics.push(ThemeDiagnostic::new(
                        ThemeDiagnosticKind::InvalidFallback,
                        Some(role.role_id.clone()),
                        Some(property_id.clone()),
                        format!(
                            "theme schema fallback for property {property_id} on role {} is invalid",
                            role.role_id
                        ),
                    ));
                    properties.insert(property_id, property);
                }
            }
        }

        roles.insert(
            role.role_id.clone(),
            ThemeRoleSchema {
                role_id: role.role_id,
                static_parent: role.static_parent,
                properties,
            },
        );
    }

    roles
}

fn normalized_theme_roles(
    definition: ThemeDefinition,
    schema_roles: &BTreeMap<StyleRoleId, ThemeRoleSchema>,
    diagnostics: &mut ThemeValidationDiagnostics,
) -> BTreeMap<StyleRoleId, ThemeRoleDefinition> {
    let mut roles = BTreeMap::new();

    for role in definition.roles {
        let ThemeRoleDefinition {
            role_id,
            static_parent,
            properties: raw_properties,
        } = role;

        if roles.contains_key(&role_id) {
            diagnostics.push(ThemeDiagnostic::new(
                ThemeDiagnosticKind::DuplicateRole,
                Some(role_id.clone()),
                None,
                format!("theme role {role_id} is defined more than once"),
            ));
            continue;
        }

        let Some(schema_role) = schema_roles.get(&role_id) else {
            diagnostics.push(ThemeDiagnostic::new(
                ThemeDiagnosticKind::UnknownRole,
                Some(role_id.clone()),
                None,
                format!("theme role {role_id} is not defined by the schema"),
            ));
            continue;
        };

        let mut properties = BTreeMap::new();
        for (property_id, source) in raw_properties {
            let Some(property_schema) = schema_role.property_schema(&property_id) else {
                diagnostics.push(ThemeDiagnostic::new(
                    ThemeDiagnosticKind::UnknownProperty,
                    Some(role_id.clone()),
                    Some(property_id.clone()),
                    format!("theme property {property_id} is not defined on role {role_id}"),
                ));
                continue;
            };

            match source {
                StylePropertySource::Concrete(value) => {
                    if value.kind() != property_schema.kind {
                        diagnostics.push(ThemeDiagnostic::new(
                            ThemeDiagnosticKind::InvalidPropertyType,
                            Some(role_id.clone()),
                            Some(property_id.clone()),
                            format!(
                                "theme property {property_id} on role {role_id} has the wrong value type"
                            ),
                        ));
                        continue;
                    }

                    if let Some(value) = value.normalized_for_kind(property_schema.kind) {
                        properties.insert(property_id, StylePropertySource::Concrete(value));
                    } else {
                        diagnostics.push(ThemeDiagnostic::new(
                            ThemeDiagnosticKind::InvalidPropertyValue,
                            Some(role_id.clone()),
                            Some(property_id.clone()),
                            format!(
                                "theme property {property_id} on role {role_id} has an invalid value"
                            ),
                        ));
                    }
                }
                StylePropertySource::StaticParent
                | StylePropertySource::AmbientParent
                | StylePropertySource::Fallback => {
                    properties.insert(property_id, source);
                }
            }
        }

        roles.insert(
            role_id.clone(),
            ThemeRoleDefinition {
                role_id,
                static_parent,
                properties,
            },
        );
    }

    roles
}

fn validate_static_parents(
    schema_roles: &BTreeMap<StyleRoleId, ThemeRoleSchema>,
    theme_roles: &BTreeMap<StyleRoleId, ThemeRoleDefinition>,
    diagnostics: &mut ThemeValidationDiagnostics,
) {
    for (role_id, schema_role) in schema_roles {
        if let Some(parent_id) = effective_static_parent(schema_role, theme_roles.get(role_id)) {
            if !schema_roles.contains_key(parent_id) {
                diagnostics.push(ThemeDiagnostic::new(
                    ThemeDiagnosticKind::MissingStaticParent,
                    Some(role_id.clone()),
                    None,
                    format!("theme role {role_id} references missing static parent {parent_id}"),
                ));
            }
        }

        if effective_static_parent(schema_role, theme_roles.get(role_id)).is_none() {
            let Some(theme_role) = theme_roles.get(role_id) else {
                continue;
            };
            for (property_id, source) in &theme_role.properties {
                if matches!(source, StylePropertySource::StaticParent) {
                    diagnostics.push(ThemeDiagnostic::new(
                        ThemeDiagnosticKind::MissingStaticParent,
                        Some(role_id.clone()),
                        Some(property_id.clone()),
                        format!(
                            "theme property {property_id} on role {role_id} requests static parent inheritance but no static parent is defined"
                        ),
                    ));
                }
            }
        }
    }
}

fn validate_static_parent_cycles(
    schema_roles: &BTreeMap<StyleRoleId, ThemeRoleSchema>,
    theme_roles: &BTreeMap<StyleRoleId, ThemeRoleDefinition>,
    diagnostics: &mut ThemeValidationDiagnostics,
) {
    for role_id in schema_roles.keys() {
        let mut seen = BTreeSet::new();
        let mut cursor = Some(role_id);

        while let Some(current_role_id) = cursor {
            if !seen.insert(current_role_id.clone()) {
                diagnostics.push(ThemeDiagnostic::new(
                    ThemeDiagnosticKind::StaticParentCycle,
                    Some(role_id.clone()),
                    None,
                    format!("theme role {role_id} participates in a static parent cycle"),
                ));
                break;
            }

            let Some(schema_role) = schema_roles.get(current_role_id) else {
                break;
            };
            let Some(parent_id) =
                effective_static_parent(schema_role, theme_roles.get(current_role_id))
            else {
                break;
            };
            if !schema_roles.contains_key(parent_id) {
                break;
            }
            cursor = Some(parent_id);
        }
    }
}

fn effective_static_parent<'a>(
    schema_role: &'a ThemeRoleSchema,
    theme_role: Option<&'a ThemeRoleDefinition>,
) -> Option<&'a StyleRoleId> {
    theme_role
        .and_then(|role| role.static_parent.as_ref())
        .or(schema_role.static_parent.as_ref())
}
