use std::{collections::BTreeMap, error::Error, fmt};

pub const THEME_FONT_FAMILY_MAX_BYTES: usize = 256;
pub const THEME_LOGICAL_PIXELS_MAX: f32 = 4096.0;
pub const THEME_FONT_WEIGHT_MIN: u16 = 100;
pub const THEME_FONT_WEIGHT_MAX: u16 = 900;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ThemeRoleId(Box<str>);

impl ThemeRoleId {
    pub(crate) fn canonical(value: &'static str) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ThemeRoleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ThemePropertyId {
    Background,
    Border,
    Color,
    Foreground,
    TextBackground,
    FontFamily,
    FontSize,
    FontWeight,
}

impl ThemePropertyId {
    pub const ALL: [Self; 8] = [
        Self::Background,
        Self::Border,
        Self::Color,
        Self::Foreground,
        Self::TextBackground,
        Self::FontFamily,
        Self::FontSize,
        Self::FontWeight,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Border => "border",
            Self::Color => "color",
            Self::Foreground => "foreground",
            Self::TextBackground => "text_background",
            Self::FontFamily => "font_family",
            Self::FontSize => "font_size",
            Self::FontWeight => "font_weight",
        }
    }

    #[must_use]
    pub fn from_str(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|property| property.as_str() == value)
    }
}

impl fmt::Display for ThemePropertyId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ThemeValueKind {
    Color,
    FontFamily,
    LogicalPixels,
    FontWeight,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ThemeColor([u8; 3]);

impl ThemeColor {
    pub fn parse(value: &str) -> Result<Self, ThemeValueError> {
        let bytes = value.as_bytes();
        if bytes.len() != 7 || bytes[0] != b'#' {
            return Err(ThemeValueError::InvalidColor);
        }
        let mut channels = [0; 3];
        for (index, channel) in channels.iter_mut().enumerate() {
            let high = hex_digit(bytes[1 + index * 2]).ok_or(ThemeValueError::InvalidColor)?;
            let low = hex_digit(bytes[2 + index * 2]).ok_or(ThemeValueError::InvalidColor)?;
            *channel = high * 16 + low;
        }
        Ok(Self(channels))
    }

    #[must_use]
    pub const fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self([red, green, blue])
    }

    #[must_use]
    pub const fn rgb(self) -> [u8; 3] {
        self.0
    }
}

impl fmt::Display for ThemeColor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "#{:02x}{:02x}{:02x}",
            self.0[0], self.0[1], self.0[2]
        )
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ThemeFontFamily(Box<str>);

impl ThemeFontFamily {
    pub fn new(value: impl AsRef<str>) -> Result<Self, ThemeValueError> {
        let value = value.as_ref().trim();
        if value.is_empty()
            || value.len() > THEME_FONT_FAMILY_MAX_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(ThemeValueError::InvalidFontFamily);
        }
        Ok(Self(value.into()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ThemeLogicalPixels(u32);

impl ThemeLogicalPixels {
    pub fn new(value: f32) -> Result<Self, ThemeValueError> {
        if !value.is_finite() || !(0.0..=THEME_LOGICAL_PIXELS_MAX).contains(&value) {
            return Err(ThemeValueError::InvalidLogicalPixels);
        }
        Ok(Self(value.to_bits()))
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        f32::from_bits(self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ThemeFontWeight(u16);

impl ThemeFontWeight {
    pub fn new(value: u16) -> Result<Self, ThemeValueError> {
        if !(THEME_FONT_WEIGHT_MIN..=THEME_FONT_WEIGHT_MAX).contains(&value) {
            return Err(ThemeValueError::InvalidFontWeight);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ThemeValue {
    Color(ThemeColor),
    FontFamily(ThemeFontFamily),
    LogicalPixels(ThemeLogicalPixels),
    FontWeight(ThemeFontWeight),
}

impl ThemeValue {
    #[must_use]
    pub const fn kind(&self) -> ThemeValueKind {
        match self {
            Self::Color(_) => ThemeValueKind::Color,
            Self::FontFamily(_) => ThemeValueKind::FontFamily,
            Self::LogicalPixels(_) => ThemeValueKind::LogicalPixels,
            Self::FontWeight(_) => ThemeValueKind::FontWeight,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ThemePropertySource {
    Concrete(ThemeValue),
    StaticParent,
    AmbientParent,
    Fallback,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeRoleDefinition {
    role_id: ThemeRoleId,
    static_parent: Option<ThemeRoleId>,
    properties: BTreeMap<ThemePropertyId, ThemePropertySource>,
}

impl ThemeRoleDefinition {
    pub(crate) fn new(
        role_id: ThemeRoleId,
        static_parent: Option<ThemeRoleId>,
        properties: BTreeMap<ThemePropertyId, ThemePropertySource>,
    ) -> Self {
        Self {
            role_id,
            static_parent,
            properties,
        }
    }

    #[must_use]
    pub const fn role_id(&self) -> &ThemeRoleId {
        &self.role_id
    }

    #[must_use]
    pub const fn static_parent(&self) -> Option<&ThemeRoleId> {
        self.static_parent.as_ref()
    }

    #[must_use]
    pub const fn properties(&self) -> &BTreeMap<ThemePropertyId, ThemePropertySource> {
        &self.properties
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ThemeDefinition {
    roles: BTreeMap<ThemeRoleId, ThemeRoleDefinition>,
}

impl ThemeDefinition {
    pub(crate) fn checked(
        roles: BTreeMap<ThemeRoleId, ThemeRoleDefinition>,
    ) -> Result<Self, ThemeValueError> {
        if roles.iter().any(|(key, role)| key != role.role_id()) {
            return Err(ThemeValueError::RoleIdentityMismatch);
        }
        Ok(Self { roles })
    }

    #[must_use]
    pub const fn empty() -> Self {
        Self {
            roles: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn roles(&self) -> &BTreeMap<ThemeRoleId, ThemeRoleDefinition> {
        &self.roles
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedStyle {
    properties: BTreeMap<ThemePropertyId, ThemeValue>,
}

impl ResolvedStyle {
    pub(crate) fn complete(properties: BTreeMap<ThemePropertyId, ThemeValue>) -> Self {
        Self { properties }
    }

    #[must_use]
    pub fn property(&self, property: ThemePropertyId) -> Option<&ThemeValue> {
        self.properties.get(&property)
    }

    #[must_use]
    pub const fn properties(&self) -> &BTreeMap<ThemePropertyId, ThemeValue> {
        &self.properties
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ThemeAmbientContext {
    Window,
    Panel,
    Transcript,
    UserInput,
    CodePanel,
}

impl ThemeAmbientContext {
    pub const ALL: [Self; 5] = [
        Self::Window,
        Self::Panel,
        Self::Transcript,
        Self::UserInput,
        Self::CodePanel,
    ];
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedAppearance {
    base: BTreeMap<ThemeRoleId, ResolvedStyle>,
    ambient_variants: BTreeMap<ThemeAmbientContext, BTreeMap<ThemeRoleId, ResolvedStyle>>,
}

impl ResolvedAppearance {
    pub(crate) fn complete(
        base: BTreeMap<ThemeRoleId, ResolvedStyle>,
        ambient_variants: BTreeMap<ThemeAmbientContext, BTreeMap<ThemeRoleId, ResolvedStyle>>,
    ) -> Self {
        Self {
            base,
            ambient_variants,
        }
    }

    #[must_use]
    pub fn style(&self, role: &ThemeRoleId) -> Option<&ResolvedStyle> {
        self.base.get(role)
    }

    #[must_use]
    pub fn style_in(
        &self,
        role: &ThemeRoleId,
        ambient: ThemeAmbientContext,
    ) -> Option<&ResolvedStyle> {
        self.ambient_variants.get(&ambient)?.get(role)
    }

    #[must_use]
    pub const fn roles(&self) -> &BTreeMap<ThemeRoleId, ResolvedStyle> {
        &self.base
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemeValueError {
    InvalidColor,
    InvalidFontFamily,
    InvalidLogicalPixels,
    InvalidFontWeight,
    RoleIdentityMismatch,
}

impl fmt::Display for ThemeValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidColor => "theme color must be exactly #RRGGBB",
            Self::InvalidFontFamily => {
                "theme font family is empty, oversized, or contains controls"
            }
            Self::InvalidLogicalPixels => {
                "theme logical-pixel value is outside the finite supported range"
            }
            Self::InvalidFontWeight => "theme font weight is outside 100..=900",
            Self::RoleIdentityMismatch => "theme role map key disagrees with the role record",
        })
    }
}

impl Error for ThemeValueError {}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
