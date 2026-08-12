use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    sync::OnceLock,
};

use super::schema::{ThemeRoleSchema, ThemeSchema};
use super::{
    ResolvedAppearance, ResolvedStyle, ThemeAmbientContext, ThemeDefinition, ThemePropertyId,
    ThemePropertySource, ThemeRoleId, ThemeValue, canonical_theme_schema,
};

pub const THEME_VALIDATION_MAX_DIAGNOSTICS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeDiagnosticKind {
    UnknownRole,
    UnknownProperty,
    InvalidPropertyType,
    InvalidStaticParent,
    StaticParentCycle,
    IneligibleStaticParent,
    IneligibleAmbientParent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeDiagnostic {
    kind: ThemeDiagnosticKind,
    role: Box<str>,
    property: Option<ThemePropertyId>,
}

impl ThemeDiagnostic {
    #[must_use]
    pub const fn kind(&self) -> ThemeDiagnosticKind {
        self.kind
    }

    #[must_use]
    pub fn role(&self) -> &str {
        &self.role
    }

    #[must_use]
    pub const fn property(&self) -> Option<ThemePropertyId> {
        self.property
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeValidationDiagnostics {
    diagnostics: Box<[ThemeDiagnostic]>,
    truncated: bool,
}

impl ThemeValidationDiagnostics {
    #[must_use]
    pub fn diagnostics(&self) -> &[ThemeDiagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

impl fmt::Display for ThemeValidationDiagnostics {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "theme validation failed with {} bounded diagnostic(s)",
            self.diagnostics.len()
        )
    }
}

impl Error for ThemeValidationDiagnostics {}

#[derive(Clone, Debug)]
pub struct ThemeResolver {
    definition: ThemeDefinition,
}

impl ThemeResolver {
    pub fn new(definition: &ThemeDefinition) -> Result<Self, ThemeValidationDiagnostics> {
        validate_definition(canonical_theme_schema(), definition)?;
        Ok(Self {
            definition: definition.clone(),
        })
    }

    #[must_use]
    pub fn resolve(&self) -> ResolvedAppearance {
        resolve_complete(canonical_theme_schema(), &self.definition)
    }
}

#[must_use]
pub fn builtin_fallback_appearance() -> ResolvedAppearance {
    static FALLBACK: OnceLock<ResolvedAppearance> = OnceLock::new();
    FALLBACK
        .get_or_init(|| {
            ThemeResolver::new(&ThemeDefinition::empty())
                .expect("the canonical empty theme definition must validate")
                .resolve()
        })
        .clone()
}

struct DiagnosticBuilder {
    diagnostics: Vec<ThemeDiagnostic>,
    truncated: bool,
}

impl DiagnosticBuilder {
    fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            truncated: false,
        }
    }

    fn push(&mut self, kind: ThemeDiagnosticKind, role: &str, property: Option<ThemePropertyId>) {
        if self.diagnostics.len() == THEME_VALIDATION_MAX_DIAGNOSTICS {
            self.truncated = true;
            return;
        }
        self.diagnostics.push(ThemeDiagnostic {
            kind,
            role: role.into(),
            property,
        });
    }

    fn finish(self) -> Result<(), ThemeValidationDiagnostics> {
        if self.diagnostics.is_empty() {
            Ok(())
        } else {
            Err(ThemeValidationDiagnostics {
                diagnostics: self.diagnostics.into_boxed_slice(),
                truncated: self.truncated,
            })
        }
    }
}

fn validate_definition(
    schema: &ThemeSchema,
    definition: &ThemeDefinition,
) -> Result<(), ThemeValidationDiagnostics> {
    let mut diagnostics = DiagnosticBuilder::new();

    for role in definition.roles().values() {
        let role_name = role.role_id().as_str();
        let Some(role_schema) = schema.role(role_name) else {
            diagnostics.push(ThemeDiagnosticKind::UnknownRole, role_name, None);
            continue;
        };

        if let Some(parent) = role.static_parent() {
            if parent == role.role_id() || schema.role(parent.as_str()).is_none() {
                diagnostics.push(ThemeDiagnosticKind::InvalidStaticParent, role_name, None);
            }
        }

        for (property, source) in role.properties() {
            let Some(property_schema) = role_schema.property(*property) else {
                diagnostics.push(
                    ThemeDiagnosticKind::UnknownProperty,
                    role_name,
                    Some(*property),
                );
                continue;
            };
            match source {
                ThemePropertySource::Concrete(value) if value.kind() != property_schema.kind() => {
                    diagnostics.push(
                        ThemeDiagnosticKind::InvalidPropertyType,
                        role_name,
                        Some(*property),
                    );
                }
                ThemePropertySource::StaticParent => {
                    let parent_supports_property = effective_parent(definition, role_schema)
                        .and_then(|parent| schema.role(parent.as_str()))
                        .is_some_and(|parent| parent.property(*property).is_some());
                    if !property_schema.static_parent_eligible() || !parent_supports_property {
                        diagnostics.push(
                            ThemeDiagnosticKind::IneligibleStaticParent,
                            role_name,
                            Some(*property),
                        );
                    }
                }
                ThemePropertySource::AmbientParent
                    if !property_schema.ambient_parent_eligible() =>
                {
                    diagnostics.push(
                        ThemeDiagnosticKind::IneligibleAmbientParent,
                        role_name,
                        Some(*property),
                    );
                }
                ThemePropertySource::Concrete(_)
                | ThemePropertySource::AmbientParent
                | ThemePropertySource::Fallback => {}
            }
        }
    }

    validate_parent_cycles(schema, definition, &mut diagnostics);
    diagnostics.finish()
}

fn validate_parent_cycles(
    schema: &ThemeSchema,
    definition: &ThemeDefinition,
    diagnostics: &mut DiagnosticBuilder,
) {
    let mut reported = BTreeSet::new();
    for role in schema.roles().values() {
        let mut path = BTreeSet::new();
        let mut current = role;
        loop {
            if !path.insert(current.id().clone()) {
                if reported.insert(current.id().clone()) {
                    diagnostics.push(
                        ThemeDiagnosticKind::StaticParentCycle,
                        current.id().as_str(),
                        None,
                    );
                }
                break;
            }
            let Some(parent_id) = effective_parent(definition, current) else {
                break;
            };
            let Some(parent) = schema.role(parent_id.as_str()) else {
                break;
            };
            current = parent;
        }
    }
}

fn effective_parent<'a>(
    definition: &'a ThemeDefinition,
    role: &'a ThemeRoleSchema,
) -> Option<&'a ThemeRoleId> {
    definition
        .roles()
        .get(role.id())
        .and_then(|definition| definition.static_parent())
        .or_else(|| role.static_parent())
}

fn resolve_complete(schema: &ThemeSchema, definition: &ThemeDefinition) -> ResolvedAppearance {
    let base = resolve_styles(schema, definition, None);
    let ambient_variants = ThemeAmbientContext::ALL
        .into_iter()
        .map(|ambient| (ambient, resolve_styles(schema, definition, Some(ambient))))
        .collect();
    ResolvedAppearance::complete(base, ambient_variants)
}

fn resolve_styles(
    schema: &ThemeSchema,
    definition: &ThemeDefinition,
    ambient: Option<ThemeAmbientContext>,
) -> BTreeMap<ThemeRoleId, ResolvedStyle> {
    let mut values = BTreeMap::new();
    for role in schema.roles().values() {
        for property in role.properties().keys().copied() {
            let _ = resolve_value(schema, definition, role, property, ambient, &mut values);
        }
    }

    schema
        .roles()
        .values()
        .map(|role| {
            let properties = role
                .properties()
                .keys()
                .map(|property| {
                    let value = values
                        .get(&(role.id().clone(), *property))
                        .expect("all schema-declared properties were resolved")
                        .clone();
                    (*property, value)
                })
                .collect();
            (role.id().clone(), ResolvedStyle::complete(properties))
        })
        .collect()
}

fn resolve_value(
    schema: &ThemeSchema,
    definition: &ThemeDefinition,
    role: &ThemeRoleSchema,
    property: ThemePropertyId,
    ambient: Option<ThemeAmbientContext>,
    values: &mut BTreeMap<(ThemeRoleId, ThemePropertyId), ThemeValue>,
) -> ThemeValue {
    let key = (role.id().clone(), property);
    if let Some(value) = values.get(&key) {
        return value.clone();
    }
    let property_schema = role
        .property(property)
        .expect("resolution visits only schema-declared properties");
    let source = definition
        .roles()
        .get(role.id())
        .and_then(|role| role.properties().get(&property))
        .unwrap_or(&ThemePropertySource::Fallback);

    let value = match source {
        ThemePropertySource::Concrete(value) => value.clone(),
        ThemePropertySource::Fallback => property_schema.fallback().clone(),
        ThemePropertySource::StaticParent => effective_parent(definition, role)
            .and_then(|parent| schema.role(parent.as_str()))
            .and_then(|parent| {
                parent
                    .property(property)
                    .map(|_| resolve_value(schema, definition, parent, property, ambient, values))
            })
            .unwrap_or_else(|| property_schema.fallback().clone()),
        ThemePropertySource::AmbientParent => ambient
            .and_then(ambient_role_id)
            .and_then(|ambient_role| schema.role(ambient_role))
            .and_then(|ambient_role| {
                ambient_role.property(ThemePropertyId::Background).map(|_| {
                    resolve_value(
                        schema,
                        definition,
                        ambient_role,
                        ThemePropertyId::Background,
                        ambient,
                        values,
                    )
                })
            })
            .unwrap_or_else(|| property_schema.fallback().clone()),
    };
    values.insert(key, value.clone());
    value
}

const fn ambient_role_id(ambient: ThemeAmbientContext) -> Option<&'static str> {
    Some(match ambient {
        ThemeAmbientContext::Window => "app.window",
        ThemeAmbientContext::Panel => "panel",
        ThemeAmbientContext::Transcript => "transcript.shell",
        ThemeAmbientContext::UserInput => "input.panel",
        ThemeAmbientContext::CodePanel => "code_panel.body",
    })
}
