use crate::ContentReference;

use super::super::ProviderTextV1;

/// Exact submitted content already owned by Syndic; user bytes are not duplicated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderSubmittedContentV1 {
    pub content: ContentReference,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderUserMessageV1 {
    pub client_id: Option<ProviderTextV1>,
    pub submitted: ProviderSubmittedContentV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderHookPromptFragmentV1 {
    pub text: ProviderTextV1,
    pub hook_run_id: ProviderTextV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderHookPromptV1 {
    pub fragments: Vec<ProviderHookPromptFragmentV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderMessagePhaseV1 {
    Commentary,
    FinalAnswer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderMemoryCitationEntryV1 {
    pub path: ProviderTextV1,
    pub line_start: u32,
    pub line_end: u32,
    pub note: ProviderTextV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderMemoryCitationV1 {
    pub entries: Vec<ProviderMemoryCitationEntryV1>,
    pub thread_ids: Vec<ProviderTextV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderAgentMessageV1 {
    pub text: ProviderTextV1,
    pub phase: Option<ProviderMessagePhaseV1>,
    pub memory_citation: Option<ProviderMemoryCitationV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderPlanV1 {
    pub text: ProviderTextV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderReasoningV1 {
    pub summary: Vec<ProviderTextV1>,
}
