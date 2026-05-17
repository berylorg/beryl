use std::{collections::BTreeMap, fmt};

pub const MAX_THEME_FONT_FAMILY_BYTES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleRoleId(String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StylePropertyId(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StylePropertyKind {
    Color,
    FontFamily,
    LogicalPixels,
    FontWeight,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StylePropertyValue {
    Color(String),
    FontFamily(String),
    LogicalPixels(f32),
    FontWeight(u16),
}

#[derive(Clone, Debug, PartialEq)]
pub enum StylePropertySource {
    Concrete(StylePropertyValue),
    StaticParent,
    AmbientParent,
    Fallback,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemePropertySchema {
    pub(super) kind: StylePropertyKind,
    pub(super) fallback: StylePropertyValue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeRoleSchema {
    pub(super) role_id: StyleRoleId,
    pub(super) static_parent: Option<StyleRoleId>,
    pub(super) properties: BTreeMap<StylePropertyId, ThemePropertySchema>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeSchema {
    pub(super) roles: Vec<ThemeRoleSchema>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeRoleDefinition {
    pub(super) role_id: StyleRoleId,
    pub(super) static_parent: Option<StyleRoleId>,
    pub(super) properties: BTreeMap<StylePropertyId, StylePropertySource>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeDefinition {
    pub(super) roles: Vec<ThemeRoleDefinition>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedStyle {
    pub(super) properties: BTreeMap<StylePropertyId, StylePropertyValue>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeResolutionContext {
    pub(super) ambient_parent: Option<ResolvedStyle>,
}

impl StyleRoleId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for StyleRoleId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for StyleRoleId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for StyleRoleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl StylePropertyId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for StylePropertyId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for StylePropertyId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for StylePropertyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl StylePropertyValue {
    pub fn color(value: impl Into<String>) -> Self {
        Self::Color(value.into())
    }

    pub fn font_family(value: impl Into<String>) -> Self {
        Self::FontFamily(value.into())
    }

    pub fn logical_pixels(value: f32) -> Self {
        Self::LogicalPixels(value)
    }

    pub fn font_weight(value: u16) -> Self {
        Self::FontWeight(value)
    }

    pub fn kind(&self) -> StylePropertyKind {
        match self {
            Self::Color(_) => StylePropertyKind::Color,
            Self::FontFamily(_) => StylePropertyKind::FontFamily,
            Self::LogicalPixels(_) => StylePropertyKind::LogicalPixels,
            Self::FontWeight(_) => StylePropertyKind::FontWeight,
        }
    }

    pub(super) fn normalized_for_kind(&self, kind: StylePropertyKind) -> Option<Self> {
        if self.kind() != kind {
            return None;
        }

        match self {
            Self::Color(value) => normalize_hex_color(value).map(Self::Color),
            Self::FontFamily(value) => {
                let trimmed = value.trim();
                (!trimmed.is_empty() && trimmed.len() <= MAX_THEME_FONT_FAMILY_BYTES)
                    .then(|| Self::FontFamily(trimmed.to_string()))
            }
            Self::LogicalPixels(value) => {
                (value.is_finite() && *value >= 0.0).then_some(Self::LogicalPixels(*value))
            }
            Self::FontWeight(value) => (100..=900)
                .contains(value)
                .then_some(Self::FontWeight(*value)),
        }
    }
}

impl ThemePropertySchema {
    pub fn new(kind: StylePropertyKind, fallback: StylePropertyValue) -> Self {
        Self { kind, fallback }
    }

    pub fn kind(&self) -> StylePropertyKind {
        self.kind
    }

    pub fn fallback(&self) -> &StylePropertyValue {
        &self.fallback
    }

    pub(super) fn normalized(&self) -> Option<Self> {
        Some(Self {
            kind: self.kind,
            fallback: self.fallback.normalized_for_kind(self.kind)?,
        })
    }
}

impl ThemeRoleSchema {
    pub fn new(role_id: impl Into<StyleRoleId>) -> Self {
        Self {
            role_id: role_id.into(),
            static_parent: None,
            properties: BTreeMap::new(),
        }
    }

    pub fn with_static_parent(mut self, static_parent: impl Into<StyleRoleId>) -> Self {
        self.static_parent = Some(static_parent.into());
        self
    }

    pub fn with_property(
        mut self,
        property_id: impl Into<StylePropertyId>,
        property: ThemePropertySchema,
    ) -> Self {
        self.properties.insert(property_id.into(), property);
        self
    }

    pub fn role_id(&self) -> &StyleRoleId {
        &self.role_id
    }

    pub fn static_parent(&self) -> Option<&StyleRoleId> {
        self.static_parent.as_ref()
    }

    pub fn properties(&self) -> &BTreeMap<StylePropertyId, ThemePropertySchema> {
        &self.properties
    }
}

impl ThemeSchema {
    pub fn new(roles: Vec<ThemeRoleSchema>) -> Self {
        Self { roles }
    }

    pub fn roles(&self) -> &[ThemeRoleSchema] {
        &self.roles
    }
}

impl ThemeRoleDefinition {
    pub fn new(role_id: impl Into<StyleRoleId>) -> Self {
        Self {
            role_id: role_id.into(),
            static_parent: None,
            properties: BTreeMap::new(),
        }
    }

    pub fn with_static_parent(mut self, static_parent: impl Into<StyleRoleId>) -> Self {
        self.static_parent = Some(static_parent.into());
        self
    }

    pub fn with_property(
        mut self,
        property_id: impl Into<StylePropertyId>,
        source: StylePropertySource,
    ) -> Self {
        self.properties.insert(property_id.into(), source);
        self
    }

    pub fn role_id(&self) -> &StyleRoleId {
        &self.role_id
    }

    pub fn static_parent(&self) -> Option<&StyleRoleId> {
        self.static_parent.as_ref()
    }

    pub fn properties(&self) -> &BTreeMap<StylePropertyId, StylePropertySource> {
        &self.properties
    }
}

impl ThemeDefinition {
    pub fn new(roles: Vec<ThemeRoleDefinition>) -> Self {
        Self { roles }
    }

    pub fn empty() -> Self {
        Self { roles: Vec::new() }
    }

    pub fn roles(&self) -> &[ThemeRoleDefinition] {
        &self.roles
    }
}

impl ResolvedStyle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_property(
        mut self,
        property_id: impl Into<StylePropertyId>,
        value: StylePropertyValue,
    ) -> Self {
        self.properties.insert(property_id.into(), value);
        self
    }

    pub fn property(&self, property_id: &StylePropertyId) -> Option<&StylePropertyValue> {
        self.properties.get(property_id)
    }

    pub fn properties(&self) -> &BTreeMap<StylePropertyId, StylePropertyValue> {
        &self.properties
    }
}

impl ThemeResolutionContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_ambient_parent(mut self, ambient_parent: ResolvedStyle) -> Self {
        self.ambient_parent = Some(ambient_parent);
        self
    }

    pub fn ambient_parent(&self) -> Option<&ResolvedStyle> {
        self.ambient_parent.as_ref()
    }
}

pub(super) fn normalize_hex_color(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let hex = trimmed.strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("#{}", hex.to_ascii_lowercase()))
}
