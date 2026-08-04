use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadSessionMetadata {
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub reasoning_effort: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThreadStatus {
    NotLoaded,
    Idle,
    SystemError,
    Active { active_flags: ThreadActiveFlags },
}

impl ThreadStatus {
    #[must_use]
    pub const fn active(active_flags: ThreadActiveFlags) -> Self {
        Self::Active { active_flags }
    }

    #[must_use]
    pub const fn active_flags(&self) -> Option<ThreadActiveFlags> {
        match self {
            Self::Active { active_flags } => Some(*active_flags),
            Self::NotLoaded | Self::Idle | Self::SystemError => None,
        }
    }

    #[must_use]
    pub const fn waiting_on_user_input(&self) -> bool {
        match self {
            Self::Active { active_flags } => active_flags.waiting_on_user_input(),
            Self::NotLoaded | Self::Idle | Self::SystemError => false,
        }
    }

    pub(crate) fn from_bounded_wire(
        kind: &str,
        waiting_on_approval: bool,
        waiting_on_user_input: bool,
    ) -> Option<Self> {
        let has_active_flags = waiting_on_approval || waiting_on_user_input;
        Some(match kind {
            "notLoaded" if !has_active_flags => Self::NotLoaded,
            "idle" if !has_active_flags => Self::Idle,
            "systemError" if !has_active_flags => Self::SystemError,
            "active" => Self::active(ThreadActiveFlags::new(
                waiting_on_approval,
                waiting_on_user_input,
            )),
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThreadActiveFlags(u8);

impl ThreadActiveFlags {
    const WAITING_ON_APPROVAL: u8 = 1 << 0;
    const WAITING_ON_USER_INPUT: u8 = 1 << 1;

    #[must_use]
    pub const fn new(waiting_on_approval: bool, waiting_on_user_input: bool) -> Self {
        let mut bits = 0;
        if waiting_on_approval {
            bits |= Self::WAITING_ON_APPROVAL;
        }
        if waiting_on_user_input {
            bits |= Self::WAITING_ON_USER_INPUT;
        }
        Self(bits)
    }

    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[must_use]
    pub const fn waiting_on_approval(self) -> bool {
        self.0 & Self::WAITING_ON_APPROVAL != 0
    }

    #[must_use]
    pub const fn waiting_on_user_input(self) -> bool {
        self.0 & Self::WAITING_ON_USER_INPUT != 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTokenUsage {
    pub last: TokenUsageBreakdown,
    pub total: TokenUsageBreakdown,
    #[serde(default)]
    pub model_context_window: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageBreakdown {
    #[serde(default)]
    pub cached_input_tokens: i64,
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    #[serde(default)]
    pub reasoning_output_tokens: i64,
    #[serde(default)]
    pub total_tokens: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadUnsubscribeResponse {
    pub status: ThreadUnsubscribeStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThreadUnsubscribeStatus {
    NotLoaded,
    NotSubscribed,
    Unsubscribed,
}
