//! Canonical dynamic-tool profile and typed incremental sinks for Beryl conversations.

use beryl_backend::{
    DynamicToolArgumentControl, DynamicToolArgumentScalarKind, DynamicToolCallResponse,
    DynamicToolFunctionSpec, DynamicToolNamespaceSpec, DynamicToolSpec, ThreadStartOptions,
};
use beryl_model::{CasConversationToolProfile, DynamicToolName};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    branch_discussion_dynamic_tools::{
        BranchDiscussionResolutionRequest, BranchResolutionArgumentBuilder,
        RESOLVE_BRANCH_DISCUSSION_TOOL, branch_discussion_dynamic_tool_spec,
    },
    dynamic_tool_namespace::BERYL_DYNAMIC_TOOL_NAMESPACE,
    lifecycle_dynamic_tools::{
        LifecycleArgumentBuilder, LifecycleYieldRequest, YIELD_TOOL, lifecycle_dynamic_tool_spec,
    },
};

pub(crate) mod arguments;

#[derive(Clone, Copy)]
enum InstalledToolKind {
    LifecycleYield,
    BranchResolution,
}

struct InstalledToolDefinition {
    name: &'static str,
    spec: fn() -> DynamicToolFunctionSpec,
    kind: InstalledToolKind,
}

const INSTALLED_TOOL_DEFINITIONS: [InstalledToolDefinition; 2] = [
    InstalledToolDefinition {
        name: YIELD_TOOL,
        spec: lifecycle_dynamic_tool_spec,
        kind: InstalledToolKind::LifecycleYield,
    },
    InstalledToolDefinition {
        name: RESOLVE_BRANCH_DISCUSSION_TOOL,
        spec: branch_discussion_dynamic_tool_spec,
        kind: InstalledToolKind::BranchResolution,
    },
];

/// Compact product-schema failures retained without rejected argument content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicToolSchemaRejection {
    /// The argument root was not an object.
    RootMustBeObject,
    /// The required feature field was absent.
    MissingRequiredField,
    /// The object contained a field outside the selected feature contract.
    UnknownField,
    /// The selected feature field appeared more than once.
    DuplicateField,
    /// The required field used a non-string value shape.
    WrongValueShape,
    /// A lifecycle outcome was outside the closed enum.
    InvalidEnum,
    /// A feature string violated its nonempty constraint.
    EmptyString,
    /// A decoded feature string exceeded its product limit.
    StringTooLong,
    /// Scalar fragments were invalid UTF-8, noncontiguous, or of the wrong kind.
    InvalidScalarFragment,
    /// Structural argument controls did not form the selected schema.
    InvalidControlSequence,
}

/// One bounded typed rejection for a valid routed dynamic-tool envelope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicToolRejection {
    /// The valid envelope did not select Beryl's exact installed namespace.
    UnknownNamespace,
    /// The valid envelope selected no installed tool in the canonical registry.
    UnknownTool,
    /// The streamed argument product violated the selected feature schema.
    Schema(DynamicToolSchemaRejection),
    /// A bounded feature product could not acquire its ordinary backing.
    Unavailable(DynamicToolUnavailableRejection),
}

/// Bounded unavailable causes for a valid routed dynamic-tool product.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicToolUnavailableRejection {
    /// Fallible incremental product growth failed.
    AllocationFailed,
}

impl From<DynamicToolSchemaRejection> for DynamicToolRejection {
    fn from(rejection: DynamicToolSchemaRejection) -> Self {
        Self::Schema(rejection)
    }
}

pub(crate) enum RoutedDynamicToolRequest {
    LifecycleYield(LifecycleYieldRequest),
    BranchDiscussionResolution(BranchDiscussionResolutionRequest),
    Rejected(DynamicToolRejection),
}

pub(crate) enum InstalledArgumentBuilder {
    Lifecycle(LifecycleArgumentBuilder),
    Branch(BranchResolutionArgumentBuilder),
    Rejecting(DynamicToolRejection),
}

impl InstalledArgumentBuilder {
    pub(crate) fn select(namespace: Option<&str>, tool: &DynamicToolName) -> Self {
        if namespace != Some(BERYL_DYNAMIC_TOOL_NAMESPACE) {
            return Self::Rejecting(DynamicToolRejection::UnknownNamespace);
        }
        let Some(definition) = INSTALLED_TOOL_DEFINITIONS
            .iter()
            .find(|definition| definition.name == tool.as_str())
        else {
            return Self::Rejecting(DynamicToolRejection::UnknownTool);
        };
        match definition.kind {
            InstalledToolKind::LifecycleYield => Self::Lifecycle(LifecycleArgumentBuilder::new()),
            InstalledToolKind::BranchResolution => {
                Self::Branch(BranchResolutionArgumentBuilder::new())
            }
        }
    }

    pub(crate) fn control(&mut self, control: DynamicToolArgumentControl) {
        match self {
            Self::Lifecycle(builder) => builder.control(control),
            Self::Branch(builder) => builder.control(control),
            Self::Rejecting(_) => {}
        }
    }

    pub(crate) fn fragment(
        &mut self,
        kind: DynamicToolArgumentScalarKind,
        offset: u64,
        bytes: &[u8],
    ) {
        match self {
            Self::Lifecycle(builder) => builder.fragment(kind, offset, bytes),
            Self::Branch(builder) => builder.fragment(kind, offset, bytes),
            Self::Rejecting(_) => {}
        }
    }

    pub(crate) fn seal(self) -> RoutedDynamicToolRequest {
        match self {
            Self::Lifecycle(builder) => builder.seal().map_or_else(
                RoutedDynamicToolRequest::Rejected,
                RoutedDynamicToolRequest::LifecycleYield,
            ),
            Self::Branch(builder) => builder.seal().map_or_else(
                RoutedDynamicToolRequest::Rejected,
                RoutedDynamicToolRequest::BranchDiscussionResolution,
            ),
            Self::Rejecting(rejection) => RoutedDynamicToolRequest::Rejected(rejection),
        }
    }
}

impl DynamicToolRejection {
    pub(crate) fn response(self) -> DynamicToolCallResponse {
        DynamicToolCallResponse::failure_text(format!(
            "{{\"ok\":false,\"error\":{{\"kind\":\"{}\",\"reason\":\"{}\"}}}}",
            self.kind(),
            self.reason()
        ))
    }

    const fn kind(self) -> &'static str {
        match self {
            Self::UnknownNamespace | Self::UnknownTool => "unknown_tool",
            Self::Schema(_) => "schema_rejected",
            Self::Unavailable(_) => "unavailable",
        }
    }

    const fn reason(self) -> &'static str {
        match self {
            Self::UnknownNamespace => "unknown_namespace",
            Self::UnknownTool => "unknown_tool",
            Self::Unavailable(DynamicToolUnavailableRejection::AllocationFailed) => {
                "allocation_failed"
            }
            Self::Schema(rejection) => rejection.as_str(),
        }
    }
}

impl DynamicToolSchemaRejection {
    const fn as_str(self) -> &'static str {
        match self {
            Self::RootMustBeObject => "root_must_be_object",
            Self::MissingRequiredField => "missing_required_field",
            Self::UnknownField => "unknown_field",
            Self::DuplicateField => "duplicate_field",
            Self::WrongValueShape => "wrong_value_shape",
            Self::InvalidEnum => "invalid_enum",
            Self::EmptyString => "empty_string",
            Self::StringTooLong => "string_too_long",
            Self::InvalidScalarFragment => "invalid_scalar_fragment",
            Self::InvalidControlSequence => "invalid_control_sequence",
        }
    }
}

/// One immutable ordered tool registry and its durable identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationToolRegistry {
    profile: CasConversationToolProfile,
    specs: Vec<DynamicToolSpec>,
}

/// Invalid caller input at the canonical conversation-tool boundary.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ConversationToolRegistryError {
    /// Only the app-owned registry may populate persistent conversation tools.
    #[error("persistent conversation options supplied {count} noncanonical dynamic tools")]
    CallerSuppliedDynamicTools { count: usize },
}

impl ConversationToolRegistry {
    /// Builds the exact registry installed on every persistent conversation lineage.
    #[must_use]
    pub fn canonical() -> Self {
        let functions = INSTALLED_TOOL_DEFINITIONS
            .iter()
            .map(|definition| (definition.spec)())
            .collect();
        let specs = vec![
            DynamicToolNamespaceSpec::new(
                BERYL_DYNAMIC_TOOL_NAMESPACE,
                "Beryl-owned conversation tools.",
                functions,
            )
            .into(),
        ];
        let encoded = serde_json::to_vec(&specs)
            .expect("serializing owned dynamic-tool JSON values cannot fail");
        let profile = CasConversationToolProfile::v1(Sha256::digest(encoded).into());
        Self { profile, specs }
    }

    /// Returns the exact durable identity of this ordered registry.
    #[must_use]
    pub const fn profile(&self) -> CasConversationToolProfile {
        self.profile
    }

    /// Returns the exact definitions in provider-stable registration order.
    #[must_use]
    pub fn specs(&self) -> &[DynamicToolSpec] {
        &self.specs
    }

    /// Installs this registry into one fresh persistent CAS thread request.
    pub fn install(
        &self,
        options: ThreadStartOptions,
    ) -> Result<ThreadStartOptions, ConversationToolRegistryError> {
        if !options.dynamic_tools().is_empty() {
            return Err(ConversationToolRegistryError::CallerSuppliedDynamicTools {
                count: options.dynamic_tools().len(),
            });
        }
        Ok(options.with_dynamic_tools(self.specs.clone()))
    }
}
