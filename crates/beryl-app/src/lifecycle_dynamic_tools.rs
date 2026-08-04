use beryl_backend::{
    DynamicToolArgumentControl, DynamicToolArgumentScalarKind, DynamicToolCallResponse,
    DynamicToolFunctionSpec,
};
use serde_json::json;

use crate::{
    cas_projection::OrdinaryDynamicToolContext,
    conversation_tools::{
        DynamicToolRejection, DynamicToolSchemaRejection,
        arguments::{SingleStringObjectBuilder, StringValueSink},
    },
};

pub const YIELD_TOOL: &str = "yield";

/// Closed semantic outcomes accepted by Beryl's lifecycle-yield tool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleYieldOutcome {
    /// The current phase reached a boundary requiring review.
    PhaseNeedsReview,
    /// Progress is blocked and needs Operator input.
    BlockedNeedsOperator,
    /// The phase reached a resumable boundary and should continue.
    PhaseContinue,
    /// The complete authoritative plan has finished.
    PlanComplete,
}

/// One schema-validated lifecycle-yield request.
pub struct LifecycleYieldRequest {
    outcome: LifecycleYieldOutcome,
}

/// Feature-owned authority for one validated lifecycle-yield request.
pub trait LifecycleYieldRequestHandler {
    /// Handles one lifecycle request and returns its sole bounded backend response.
    fn respond_lifecycle_yield(
        &mut self,
        context: OrdinaryDynamicToolContext,
        request: LifecycleYieldRequest,
    ) -> DynamicToolCallResponse;
}

/// Feature dispatch result containing the response and semantic outcome.
pub struct BerylLifecycleDynamicToolDispatch {
    response: DynamicToolCallResponse,
    outcome: LifecycleYieldOutcome,
}

pub(crate) fn lifecycle_dynamic_tool_spec() -> DynamicToolFunctionSpec {
    DynamicToolFunctionSpec::new(
        YIELD_TOOL,
        "Yield control to Beryl with one semantic lifecycle outcome after the current turn reaches a natural boundary. Beryl owns all stop, notification, compaction, and resume policy.",
        json!({
            "type": "object",
            "required": ["outcome"],
            "properties": {
                "outcome": {
                    "type": "string",
                    "enum": LifecycleYieldOutcome::SUPPORTED
                }
            },
            "additionalProperties": false
        }),
    )
    .with_defer_loading(false)
}

/// Converts one accepted lifecycle request into its bounded success response and outcome.
pub fn dispatch_beryl_lifecycle_dynamic_tool_request(
    request: LifecycleYieldRequest,
) -> BerylLifecycleDynamicToolDispatch {
    let outcome = request.outcome;
    let response = DynamicToolCallResponse::success_text(format!(
        "{{\"ok\":true,\"result\":{{\"outcome\":\"{}\"}}}}",
        outcome.as_str()
    ));
    BerylLifecycleDynamicToolDispatch { response, outcome }
}

impl LifecycleYieldOutcome {
    const SUPPORTED: [&'static str; 4] = [
        "phase_needs_review",
        "blocked_needs_operator",
        "phase_continue",
        "plan_complete",
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PhaseNeedsReview => "phase_needs_review",
            Self::BlockedNeedsOperator => "blocked_needs_operator",
            Self::PhaseContinue => "phase_continue",
            Self::PlanComplete => "plan_complete",
        }
    }
}

impl LifecycleYieldRequest {
    /// Returns the validated semantic lifecycle outcome.
    #[must_use]
    pub const fn outcome(&self) -> LifecycleYieldOutcome {
        self.outcome
    }
}

impl BerylLifecycleDynamicToolDispatch {
    /// Borrows the bounded backend response.
    #[must_use]
    pub const fn response(&self) -> &DynamicToolCallResponse {
        &self.response
    }

    /// Returns the semantic outcome for Beryl lifecycle policy.
    #[must_use]
    pub const fn outcome(&self) -> LifecycleYieldOutcome {
        self.outcome
    }

    /// Consumes the dispatch into its sole backend response.
    #[must_use]
    pub fn into_response(self) -> DynamicToolCallResponse {
        self.response
    }
}

pub(crate) struct LifecycleArgumentBuilder {
    product: SingleStringObjectBuilder<LifecycleOutcomeBuilder>,
}

impl LifecycleArgumentBuilder {
    pub(crate) const fn new() -> Self {
        Self {
            product: SingleStringObjectBuilder::new("outcome", LifecycleOutcomeBuilder::new()),
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

    pub(crate) fn seal(self) -> Result<LifecycleYieldRequest, DynamicToolRejection> {
        self.product
            .seal()
            .map(|outcome| LifecycleYieldRequest { outcome })
    }
}

struct LifecycleOutcomeBuilder {
    offset: usize,
    candidates: u8,
}

impl LifecycleOutcomeBuilder {
    const fn new() -> Self {
        Self {
            offset: 0,
            candidates: 0,
        }
    }
}

impl StringValueSink for LifecycleOutcomeBuilder {
    type Output = LifecycleYieldOutcome;

    fn start(&mut self) -> Result<(), DynamicToolRejection> {
        self.offset = 0;
        self.candidates = (1 << LifecycleYieldOutcome::SUPPORTED.len()) - 1;
        Ok(())
    }

    fn fragment(&mut self, bytes: &[u8]) -> Result<(), DynamicToolRejection> {
        let end = self
            .offset
            .checked_add(bytes.len())
            .ok_or(DynamicToolSchemaRejection::InvalidEnum)?;
        for (index, candidate) in LifecycleYieldOutcome::SUPPORTED.iter().enumerate() {
            let bit = 1_u8 << index;
            if self.candidates & bit != 0
                && candidate
                    .as_bytes()
                    .get(self.offset..end)
                    .is_none_or(|expected| expected != bytes)
            {
                self.candidates &= !bit;
            }
        }
        self.offset = end;
        Ok(())
    }

    fn finish(&mut self) -> Result<Self::Output, DynamicToolRejection> {
        let outcome = LifecycleYieldOutcome::SUPPORTED
            .iter()
            .enumerate()
            .find(|(index, candidate)| {
                self.candidates & (1_u8 << index) != 0 && candidate.len() == self.offset
            })
            .map(|(index, _)| match index {
                0 => LifecycleYieldOutcome::PhaseNeedsReview,
                1 => LifecycleYieldOutcome::BlockedNeedsOperator,
                2 => LifecycleYieldOutcome::PhaseContinue,
                3 => LifecycleYieldOutcome::PlanComplete,
                _ => unreachable!("lifecycle outcome table is closed"),
            });
        outcome.ok_or_else(|| DynamicToolSchemaRejection::InvalidEnum.into())
    }
}
