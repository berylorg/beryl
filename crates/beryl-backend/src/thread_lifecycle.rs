use serde::{Deserialize, Serialize};

use crate::ThreadInfo;

#[derive(Deserialize)]
pub(crate) struct ThreadLifecycleEmptyResponse {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadUnarchiveResponse {
    pub thread: ThreadInfo,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadArchiveParams<'a> {
    thread_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadUnarchiveParams<'a> {
    thread_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadDeleteParams<'a> {
    thread_id: &'a str,
}

impl<'a> ThreadArchiveParams<'a> {
    pub(crate) fn new(thread_id: &'a str) -> Self {
        Self { thread_id }
    }
}

impl<'a> ThreadUnarchiveParams<'a> {
    pub(crate) fn new(thread_id: &'a str) -> Self {
        Self { thread_id }
    }
}

impl<'a> ThreadDeleteParams<'a> {
    pub(crate) fn new(thread_id: &'a str) -> Self {
        Self { thread_id }
    }
}
