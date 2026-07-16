//! Canonical dynamic-tool profile for persistent Beryl conversation lineages.

use beryl_backend::{DynamicToolNamespaceSpec, DynamicToolSpec, ThreadStartOptions};
use beryl_model::CasConversationToolProfile;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    branch_discussion_dynamic_tools::branch_discussion_dynamic_tool_specs,
    dynamic_tool_namespace::BERYL_DYNAMIC_TOOL_NAMESPACE,
    lifecycle_dynamic_tools::beryl_lifecycle_dynamic_tool_specs,
};

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
        let mut functions = beryl_lifecycle_dynamic_tool_specs();
        functions.extend(branch_discussion_dynamic_tool_specs());
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
