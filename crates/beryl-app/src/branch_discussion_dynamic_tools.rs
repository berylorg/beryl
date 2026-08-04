use beryl_backend::{
    DynamicToolArgumentControl, DynamicToolArgumentScalarKind, DynamicToolCallResponse,
    DynamicToolFunctionSpec,
};
use serde_json::json;

use crate::{
    cas_projection::OrdinaryDynamicToolContext,
    conversation_tools::{
        DynamicToolRejection, DynamicToolSchemaRejection, DynamicToolUnavailableRejection,
        arguments::{SingleStringObjectBuilder, StringValueSink},
    },
};

/// Tool name used for conversational branch-discussion resolution.
pub const RESOLVE_BRANCH_DISCUSSION_TOOL: &str = "resolve_branch_discussion";

/// Maximum decoded Unicode scalar values accepted by one resolution.
pub const BRANCH_RESOLUTION_MAX_SCALARS: usize = 65_536;
/// Maximum retained UTF-8 bytes accepted by one resolution.
pub const BRANCH_RESOLUTION_MAX_UTF8_BYTES: usize = 262_144;

/// One admitted typed branch-resolution request. Durable handoff remains Checkpoint 5 work.
pub struct BranchDiscussionResolutionRequest {
    resolution: String,
}

/// Feature-owned authority for one validated branch-discussion resolution request.
pub trait BranchDiscussionResolutionRequestHandler {
    /// Handles one branch-resolution request and returns its sole bounded backend response.
    fn respond_branch_discussion_resolution(
        &mut self,
        context: OrdinaryDynamicToolContext,
        request: BranchDiscussionResolutionRequest,
    ) -> DynamicToolCallResponse;
}

impl BranchDiscussionResolutionRequest {
    /// Borrows the admitted decoded resolution.
    #[must_use]
    pub fn resolution(&self) -> &str {
        &self.resolution
    }
}

pub(crate) fn branch_discussion_dynamic_tool_spec() -> DynamicToolFunctionSpec {
    DynamicToolFunctionSpec::new(
        RESOLVE_BRANCH_DISCUSSION_TOOL,
        "Admit one resolution for the exact active branch discussion and schedule its durable handoff to the bound parent thread.",
        json!({
            "type": "object",
            "required": ["resolution"],
            "properties": {
                "resolution": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 65536,
                    "description": "The complete resolution to hand back to the discussion's bound parent thread."
                }
            },
            "additionalProperties": false
        }),
    )
    .with_defer_loading(false)
}

pub(crate) struct BranchResolutionArgumentBuilder {
    product: SingleStringObjectBuilder<BranchResolutionValueBuilder>,
}

impl BranchResolutionArgumentBuilder {
    pub(crate) fn new() -> Self {
        Self {
            product: SingleStringObjectBuilder::new(
                "resolution",
                BranchResolutionValueBuilder::new(),
            ),
        }
    }

    pub(crate) fn control(&mut self, control: DynamicToolArgumentControl) {
        self.product.control(control);
    }

    pub(crate) fn fragment(
        &mut self,
        kind: DynamicToolArgumentScalarKind,
        offset: u64,
        bytes: &[u8],
    ) {
        self.product.fragment(kind, offset, bytes);
    }

    pub(crate) fn seal(self) -> Result<BranchDiscussionResolutionRequest, DynamicToolRejection> {
        self.product.seal()
    }
}

struct BranchResolutionValueBuilder {
    scalars: usize,
    resolution: Option<String>,
}

impl BranchResolutionValueBuilder {
    fn new() -> Self {
        Self {
            scalars: 0,
            resolution: None,
        }
    }
}

impl StringValueSink for BranchResolutionValueBuilder {
    type Output = BranchDiscussionResolutionRequest;

    fn start(&mut self) -> Result<(), DynamicToolRejection> {
        self.scalars = 0;
        self.resolution = Some(String::new());
        Ok(())
    }

    fn fragment(&mut self, bytes: &[u8]) -> Result<(), DynamicToolRejection> {
        let fragment = std::str::from_utf8(bytes)
            .map_err(|_| DynamicToolSchemaRejection::InvalidScalarFragment)?;
        let resolution = self
            .resolution
            .as_mut()
            .ok_or(DynamicToolSchemaRejection::InvalidControlSequence)?;
        let next_bytes = resolution
            .len()
            .checked_add(bytes.len())
            .ok_or(DynamicToolSchemaRejection::StringTooLong)?;
        if next_bytes > BRANCH_RESOLUTION_MAX_UTF8_BYTES {
            return Err(DynamicToolSchemaRejection::StringTooLong.into());
        }
        let fragment_scalars = fragment.chars().count();
        let next_scalars = self
            .scalars
            .checked_add(fragment_scalars)
            .ok_or(DynamicToolSchemaRejection::StringTooLong)?;
        if next_scalars > BRANCH_RESOLUTION_MAX_SCALARS {
            return Err(DynamicToolSchemaRejection::StringTooLong.into());
        }
        resolution.try_reserve(bytes.len()).map_err(|_| {
            DynamicToolRejection::Unavailable(DynamicToolUnavailableRejection::AllocationFailed)
        })?;
        resolution.push_str(fragment);
        self.scalars = next_scalars;
        Ok(())
    }

    fn finish(&mut self) -> Result<Self::Output, DynamicToolRejection> {
        let resolution = self
            .resolution
            .take()
            .ok_or(DynamicToolSchemaRejection::InvalidControlSequence)?;
        if self.scalars == 0 {
            return Err(DynamicToolSchemaRejection::EmptyString.into());
        }
        Ok(BranchDiscussionResolutionRequest { resolution })
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/branch_resolution.rs"
    ));
}
