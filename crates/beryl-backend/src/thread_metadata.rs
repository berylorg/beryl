use beryl_model::CasThreadId;

use crate::{ProtocolIdentity, ThreadStatus};

pub const THREAD_AGENT_NICKNAME_MAX_BYTES: usize = 1_024;

/// Compact bounded metadata retained from one metadata-only `thread/read` response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadReadMetadata {
    thread_id: CasThreadId,
    status: ThreadStatus,
    model_provider: String,
    agent_nickname: Option<String>,
}

impl ThreadReadMetadata {
    pub(crate) fn try_new(
        thread_id: &str,
        status: ThreadStatus,
        model_provider: &str,
        agent_nickname: Option<&str>,
    ) -> Option<Self> {
        let model_provider = ProtocolIdentity::try_new(model_provider)
            .ok()?
            .as_str()
            .to_owned();
        let agent_nickname = match agent_nickname {
            Some(value) if !value.is_empty() && value.len() <= THREAD_AGENT_NICKNAME_MAX_BYTES => {
                Some(value.to_owned())
            }
            Some(_) => return None,
            None => None,
        };
        Some(Self {
            thread_id: CasThreadId::new(thread_id).ok()?,
            status,
            model_provider,
            agent_nickname,
        })
    }

    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    #[must_use]
    pub const fn status(&self) -> &ThreadStatus {
        &self.status
    }

    #[must_use]
    pub fn model_provider(&self) -> &str {
        &self.model_provider
    }

    #[must_use]
    pub fn agent_nickname(&self) -> Option<&str> {
        self.agent_nickname.as_deref()
    }
}

#[cfg(feature = "lifecycle-test-support")]
pub(crate) fn thread_read_metadata_for_lifecycle_test(
    thread_id: CasThreadId,
    status: ThreadStatus,
    model_provider: &str,
    agent_nickname: Option<&str>,
) -> ThreadReadMetadata {
    let metadata =
        ThreadReadMetadata::try_new(thread_id.as_str(), status, model_provider, agent_nickname)
            .expect("lifecycle-test thread metadata must satisfy compact bounds");
    assert_eq!(metadata.thread_id(), &thread_id);
    metadata
}
