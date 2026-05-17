use std::fmt;

pub const BUILT_IN_INSTALLED_THEME_ID: &str = "built-in";
pub(super) const BUILT_IN_INSTALLED_THEME_NAME: &str = "Beryl Built In";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstalledThemeId(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledThemeMetadata {
    id: InstalledThemeId,
    name: String,
    built_in: bool,
    active: bool,
}

impl InstalledThemeId {
    pub fn new(value: impl Into<String>) -> Result<Self, InstalledThemeIdError> {
        let value = value.into();
        if !is_valid_installed_theme_id(&value) {
            return Err(InstalledThemeIdError { value });
        }
        Ok(Self(value))
    }

    pub fn built_in() -> Self {
        Self(BUILT_IN_INSTALLED_THEME_ID.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(super) fn generated_from_name(
        name: &str,
        is_taken: impl Fn(&str) -> bool,
    ) -> InstalledThemeId {
        let mut base = String::new();
        let mut previous_dash = false;
        for ch in name.chars() {
            let next = if ch.is_ascii_alphanumeric() {
                previous_dash = false;
                Some(ch.to_ascii_lowercase())
            } else if !previous_dash {
                previous_dash = true;
                Some('-')
            } else {
                None
            };
            if let Some(ch) = next {
                base.push(ch);
            }
        }

        let base = base.trim_matches('-');
        let base = if base.is_empty() { "theme" } else { base };
        let mut candidate = base.to_string();
        let mut suffix = 2usize;
        while candidate == BUILT_IN_INSTALLED_THEME_ID || is_taken(&candidate) {
            candidate = format!("{base}-{suffix}");
            suffix = suffix.saturating_add(1);
        }
        InstalledThemeId(candidate)
    }
}

impl InstalledThemeMetadata {
    pub(super) fn new(
        id: InstalledThemeId,
        name: impl Into<String>,
        built_in: bool,
        active: bool,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            built_in,
            active,
        }
    }

    pub fn id(&self) -> &InstalledThemeId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_built_in(&self) -> bool {
        self.built_in
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl From<&str> for InstalledThemeId {
    fn from(value: &str) -> Self {
        Self::new(value).expect("installed theme id literal must be valid")
    }
}

impl From<String> for InstalledThemeId {
    fn from(value: String) -> Self {
        Self::new(value).expect("installed theme id must be valid")
    }
}

impl fmt::Display for InstalledThemeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledThemeIdError {
    value: String,
}

impl fmt::Display for InstalledThemeIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid installed theme id `{}`", self.value)
    }
}

impl std::error::Error for InstalledThemeIdError {}

pub(super) fn validate_theme_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 128 {
        return None;
    }
    Some(trimmed.to_string())
}

pub(super) fn unique_recovered_name(
    value: &str,
    existing_names: &mut Vec<String>,
) -> Option<String> {
    let base = validate_theme_name(value)?;
    let mut name = base.clone();
    let mut suffix = 2usize;
    while existing_names
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&name))
    {
        name = format!("{base} {suffix}");
        suffix = suffix.saturating_add(1);
    }
    existing_names.push(name.clone());
    Some(name)
}

fn is_valid_installed_theme_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && value
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}
