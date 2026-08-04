use beryl_model::AssetId;

use super::ProviderItemValidationError;

mod locator;

pub(crate) use locator::ProviderImageLocatorValidatorV1;

/// Maximum admitted nesting of provider list and object containers.
pub const PROVIDER_STRUCTURED_VALUE_MAX_DEPTH: usize = 128;

/// Exact finite IEEE-754 value retained by a normalized provider number.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProviderFiniteF64V1(u64);

impl ProviderFiniteF64V1 {
    pub fn new(value: f64) -> Result<Self, ProviderItemValidationError> {
        if value.is_finite() {
            Ok(Self(value.to_bits()))
        } else {
            Err(ProviderItemValidationError::NonFiniteNumber)
        }
    }

    pub(crate) fn from_bits(bits: u64) -> Result<Self, ProviderItemValidationError> {
        Self::new(f64::from_bits(bits))
    }

    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }

    #[must_use]
    pub fn get(self) -> f64 {
        f64::from_bits(self.0)
    }
}

/// Canonical numeric classes supplied by the normalized provider boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProviderNumberV1 {
    Signed(i64),
    Unsigned(u64),
    FiniteFloat(ProviderFiniteF64V1),
}

/// Exact prior UTF-8 byte range reused by a later provider frame.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProviderTextReferenceV1 {
    start: u64,
    end: u64,
    digest: [u8; 32],
}

impl ProviderTextReferenceV1 {
    pub fn new(
        start: u64,
        end: u64,
        digest: [u8; 32],
    ) -> Result<Self, ProviderItemValidationError> {
        if start >= end {
            return Err(ProviderItemValidationError::InvalidTextReference { start, end });
        }
        Ok(Self { start, end, digest })
    }

    #[must_use]
    pub const fn start(self) -> u64 {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> u64 {
        self.end
    }

    #[must_use]
    pub const fn digest(self) -> [u8; 32] {
        self.digest
    }

    #[must_use]
    pub const fn len(self) -> u64 {
        self.end - self.start
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        false
    }
}

/// One exact provider string, either newly appended or reused without copying.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderTextV1 {
    Inline(String),
    Reused(ProviderTextReferenceV1),
}

impl ProviderTextV1 {
    #[must_use]
    pub fn inline(value: impl Into<String>) -> Self {
        Self::Inline(value.into())
    }

    #[must_use]
    pub const fn reused(reference: ProviderTextReferenceV1) -> Self {
        Self::Reused(reference)
    }

    #[must_use]
    pub fn inline_str(&self) -> Option<&str> {
        match self {
            Self::Inline(value) => Some(value),
            Self::Reused(_) => None,
        }
    }

    #[must_use]
    pub const fn reference(&self) -> Option<ProviderTextReferenceV1> {
        match self {
            Self::Inline(_) => None,
            Self::Reused(reference) => Some(*reference),
        }
    }
}

impl From<String> for ProviderTextV1 {
    fn from(value: String) -> Self {
        Self::Inline(value)
    }
}

impl From<&str> for ProviderTextV1 {
    fn from(value: &str) -> Self {
        Self::Inline(value.to_owned())
    }
}

/// Exact non-inline dynamic-tool image locator.
///
/// The constructor examines only this typed `image_url` field. Opaque ordinary
/// provider strings are never searched for data-like content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderImageLocatorV1(String);

impl ProviderImageLocatorV1 {
    pub fn new(value: impl Into<String>) -> Result<Self, ProviderItemValidationError> {
        let value = value.into();
        ProviderImageLocatorValidatorV1::validate(value.as_bytes())?;
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn validate(&self) -> Result<(), ProviderItemValidationError> {
        ProviderImageLocatorValidatorV1::validate(self.0.as_bytes())
    }
}

/// One ordered object entry. Order is retained exactly and is not map-sorted by this codec.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderObjectEntryV1 {
    pub key: String,
    pub value: ProviderStructuredValueV1,
}

/// Closed recursive provider value algebra; it is not raw JSON storage authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderStructuredValueV1 {
    Null,
    Boolean(bool),
    Number(ProviderNumberV1),
    String(ProviderTextV1),
    List(Vec<Self>),
    Object(Vec<ProviderObjectEntryV1>),
}

impl ProviderStructuredValueV1 {
    /// Validates references and the exact 128-container-depth bound without copying leaves.
    pub fn validate(&self, prior_frontier: u64) -> Result<(), ProviderItemValidationError> {
        super::validate::validate_structured_value(self, prior_frontier, 0)
    }
}

/// Beryl-owned asset identity replacing typed inline image bytes at provider ingress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderInlineImageAssetV1 {
    asset_id: AssetId,
}

impl ProviderInlineImageAssetV1 {
    #[must_use]
    pub const fn new(asset_id: AssetId) -> Self {
        Self { asset_id }
    }

    #[must_use]
    pub const fn asset_id(self) -> AssetId {
        self.asset_id
    }
}

/// Typed MCP inline-image metadata after its bytes crossed the asset boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderMcpInlineImageV1 {
    asset: ProviderInlineImageAssetV1,
    metadata: Vec<ProviderObjectEntryV1>,
}

impl ProviderMcpInlineImageV1 {
    pub fn new(
        asset: ProviderInlineImageAssetV1,
        metadata: Vec<ProviderObjectEntryV1>,
    ) -> Result<Self, ProviderItemValidationError> {
        for entry in &metadata {
            match entry.key.as_str() {
                "data" => {
                    return Err(ProviderItemValidationError::McpImageMetadataContainsBytes {
                        field: "data",
                    });
                }
                "image_url" | "imageUrl" => {
                    return Err(ProviderItemValidationError::McpImageMetadataContainsBytes {
                        field: "image URL",
                    });
                }
                _ => {}
            }
        }
        Ok(Self { asset, metadata })
    }

    #[must_use]
    pub const fn asset(&self) -> ProviderInlineImageAssetV1 {
        self.asset
    }

    #[must_use]
    pub fn metadata(&self) -> &[ProviderObjectEntryV1] {
        &self.metadata
    }
}

/// Exact MCP content branch after structurally classifying typed inline images.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderMcpContentV1(ProviderMcpContentInnerV1);

#[derive(Clone, Debug, Eq, PartialEq)]
enum ProviderMcpContentInnerV1 {
    Structured(ProviderStructuredValueV1),
    InlineImage(ProviderMcpInlineImageV1),
}

/// Borrowed closed view of one MCP content entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderMcpContentViewV1<'a> {
    Structured(&'a ProviderStructuredValueV1),
    InlineImage(&'a ProviderMcpInlineImageV1),
}

impl ProviderMcpContentV1 {
    /// Admits non-image structured content without inspecting opaque ordinary strings.
    ///
    /// Only the top-level MCP `type` discriminator is classified. A typed `image`
    /// branch must use [`Self::inline_image`] so binary text cannot become a Fjall string.
    pub fn structured(
        value: ProviderStructuredValueV1,
    ) -> Result<Self, ProviderItemValidationError> {
        reject_structured_inline_image(&value)?;
        Ok(Self(ProviderMcpContentInnerV1::Structured(value)))
    }

    #[must_use]
    pub const fn inline_image(value: ProviderMcpInlineImageV1) -> Self {
        Self(ProviderMcpContentInnerV1::InlineImage(value))
    }

    #[must_use]
    pub const fn view(&self) -> ProviderMcpContentViewV1<'_> {
        match &self.0 {
            ProviderMcpContentInnerV1::Structured(value) => {
                ProviderMcpContentViewV1::Structured(value)
            }
            ProviderMcpContentInnerV1::InlineImage(value) => {
                ProviderMcpContentViewV1::InlineImage(value)
            }
        }
    }

    pub(crate) fn validate(&self, prior_frontier: u64) -> Result<(), ProviderItemValidationError> {
        match &self.0 {
            ProviderMcpContentInnerV1::Structured(value) => {
                reject_structured_inline_image(value)?;
                value.validate(prior_frontier)
            }
            ProviderMcpContentInnerV1::InlineImage(value) => {
                for entry in value.metadata() {
                    super::validate::validate_structured_value(&entry.value, prior_frontier, 1)?;
                }
                Ok(())
            }
        }
    }
}

fn reject_structured_inline_image(
    value: &ProviderStructuredValueV1,
) -> Result<(), ProviderItemValidationError> {
    let ProviderStructuredValueV1::Object(entries) = value else {
        return Ok(());
    };
    for entry in entries {
        if entry.key != "type" {
            continue;
        }
        let ProviderStructuredValueV1::String(item_type) = &entry.value else {
            continue;
        };
        match item_type {
            ProviderTextV1::Inline(item_type) if item_type == "image" => {
                return Err(ProviderItemValidationError::McpInlineImageRequiresAsset);
            }
            ProviderTextV1::Reused(_) => {
                return Err(ProviderItemValidationError::McpContentTypeReference);
            }
            ProviderTextV1::Inline(_) => {}
        }
    }
    Ok(())
}
