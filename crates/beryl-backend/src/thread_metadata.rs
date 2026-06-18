use serde::Serialize;

use crate::{ThreadSessionMetadata, ThreadSessionResponse, ThreadSummary};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadReadMetadata {
    pub thread: ThreadSummary,
    pub session_metadata: ThreadSessionMetadata,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadResumeMetadataParams<'a> {
    pub thread_id: &'a str,
    pub exclude_turns: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadReadMetadataParams<'a> {
    pub thread_id: &'a str,
    #[serde(rename = "includeTurns")]
    pub include_transcript_turns: bool,
}

impl ThreadReadMetadata {
    pub(crate) fn from_session_response(response: ThreadSessionResponse) -> Self {
        Self {
            thread: response.thread.summary(),
            session_metadata: response.metadata(),
        }
    }
}

impl<'a> ThreadResumeMetadataParams<'a> {
    pub(crate) fn new(thread_id: &'a str) -> Self {
        Self {
            thread_id,
            exclude_turns: true,
        }
    }
}

impl<'a> ThreadReadMetadataParams<'a> {
    pub(crate) fn new(thread_id: &'a str) -> Self {
        Self {
            thread_id,
            include_transcript_turns: false,
        }
    }
}
