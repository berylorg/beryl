use std::{
    collections::BTreeMap,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use super::{
    built_in::{BUILT_IN_THEME_ROLE_INVENTORY, built_in_theme_resolver},
    model::{
        ResolvedStyle, StylePropertyId, StylePropertyValue, StyleRoleId, ThemeResolutionContext,
    },
    resolver::{ThemeResolutionError, ThemeResolver},
};

#[derive(Clone, Debug)]
pub struct ActiveThemeProjection {
    resolver: ThemeResolver,
    default_styles: BTreeMap<StyleRoleId, ResolvedStyle>,
    style_revision: u64,
}

impl ActiveThemeProjection {
    pub fn built_in() -> Self {
        let resolver = built_in_theme_resolver();
        Self::from_built_in_resolver(resolver)
            .expect("built-in theme resolver must resolve every built-in theme role")
    }

    pub fn from_built_in_resolver(resolver: ThemeResolver) -> Result<Self, ThemeResolutionError> {
        let mut default_styles = BTreeMap::new();
        let context = ThemeResolutionContext::new();

        for role_id in BUILT_IN_THEME_ROLE_INVENTORY {
            default_styles.insert(
                StyleRoleId::from(*role_id),
                resolver.resolve_style(*role_id, &context)?,
            );
        }

        let style_revision = style_revision_for(&resolver, &default_styles);

        Ok(Self {
            resolver,
            default_styles,
            style_revision,
        })
    }

    pub fn resolver(&self) -> &ThemeResolver {
        &self.resolver
    }

    pub fn default_style(
        &self,
        role_id: impl Into<StyleRoleId>,
    ) -> Result<&ResolvedStyle, ThemeResolutionError> {
        let role_id = role_id.into();
        self.default_styles
            .get(&role_id)
            .ok_or(ThemeResolutionError::UnknownRole { role_id })
    }

    pub fn default_styles(&self) -> &BTreeMap<StyleRoleId, ResolvedStyle> {
        &self.default_styles
    }

    pub fn style_revision(&self) -> u64 {
        self.style_revision
    }

    pub fn resolve_style(
        &self,
        role_id: impl Into<StyleRoleId>,
        context: &ThemeResolutionContext,
    ) -> Result<ResolvedStyle, ThemeResolutionError> {
        self.resolver.resolve_style(role_id, context)
    }

    pub fn resolve_property(
        &self,
        role_id: impl Into<StyleRoleId>,
        property_id: impl Into<StylePropertyId>,
        context: &ThemeResolutionContext,
    ) -> Result<StylePropertyValue, ThemeResolutionError> {
        self.resolver
            .resolve_property(role_id, property_id, context)
    }
}

fn style_revision_for(
    resolver: &ThemeResolver,
    default_styles: &BTreeMap<StyleRoleId, ResolvedStyle>,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    resolver.hash_semantics(&mut hasher);
    for (role_id, style) in default_styles {
        role_id.as_str().hash(&mut hasher);
        for (property_id, value) in style.properties() {
            property_id.as_str().hash(&mut hasher);
            hash_style_property_value(value, &mut hasher);
        }
    }
    hasher.finish()
}

fn hash_style_property_value(value: &StylePropertyValue, hasher: &mut DefaultHasher) {
    match value {
        StylePropertyValue::Color(value) => {
            "color".hash(hasher);
            value.hash(hasher);
        }
        StylePropertyValue::FontFamily(value) => {
            "font_family".hash(hasher);
            value.hash(hasher);
        }
        StylePropertyValue::LogicalPixels(value) => {
            "logical_pixels".hash(hasher);
            value.to_bits().hash(hasher);
        }
        StylePropertyValue::FontWeight(value) => {
            "font_weight".hash(hasher);
            value.hash(hasher);
        }
    }
}
