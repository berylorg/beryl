use serde::{Deserialize, Serialize};

use crate::{JsonRpcError, ThreadInfo};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadArchiveCapabilityProbe {
    ThreadArchive,
    ThreadUnarchive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadArchiveCapabilityReport {
    probe_results: Vec<ThreadArchiveCapabilityProbeResult>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadArchiveCapabilityProbeResult {
    probe: ThreadArchiveCapabilityProbe,
    supported: bool,
    error: Option<JsonRpcError>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadArchiveCapabilities {
    thread_archive: bool,
    thread_unarchive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadArchiveResponse {}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadUnarchiveResponse {
    pub thread: ThreadInfo,
}

pub(crate) const THREAD_ARCHIVE_CAPABILITY_PROBES: &[ThreadArchiveCapabilityProbe] = &[
    ThreadArchiveCapabilityProbe::ThreadArchive,
    ThreadArchiveCapabilityProbe::ThreadUnarchive,
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadArchiveParams<'a> {
    thread_id: &'a str,
}

impl ThreadArchiveCapabilityProbe {
    pub fn method(self) -> &'static str {
        match self {
            Self::ThreadArchive => "thread/archive",
            Self::ThreadUnarchive => "thread/unarchive",
        }
    }
}

impl ThreadArchiveCapabilityReport {
    pub(crate) fn new(probe_results: Vec<ThreadArchiveCapabilityProbeResult>) -> Self {
        Self { probe_results }
    }

    pub fn probe_results(&self) -> &[ThreadArchiveCapabilityProbeResult] {
        &self.probe_results
    }

    pub fn capabilities(&self) -> ThreadArchiveCapabilities {
        let mut capabilities = ThreadArchiveCapabilities::default();

        for result in &self.probe_results {
            match result.probe {
                ThreadArchiveCapabilityProbe::ThreadArchive => {
                    capabilities.thread_archive = result.supported;
                }
                ThreadArchiveCapabilityProbe::ThreadUnarchive => {
                    capabilities.thread_unarchive = result.supported;
                }
            }
        }

        capabilities
    }
}

impl ThreadArchiveCapabilityProbeResult {
    pub(crate) fn for_supported_probe(probe: ThreadArchiveCapabilityProbe) -> Self {
        Self {
            probe,
            supported: true,
            error: None,
        }
    }

    pub(crate) fn unsupported(probe: ThreadArchiveCapabilityProbe, error: JsonRpcError) -> Self {
        Self {
            probe,
            supported: false,
            error: Some(error),
        }
    }

    pub fn probe(&self) -> ThreadArchiveCapabilityProbe {
        self.probe
    }

    pub fn supported(&self) -> bool {
        self.supported
    }

    pub fn error(&self) -> Option<&JsonRpcError> {
        self.error.as_ref()
    }
}

impl ThreadArchiveCapabilities {
    pub fn new(thread_archive: bool, thread_unarchive: bool) -> Self {
        Self {
            thread_archive,
            thread_unarchive,
        }
    }

    pub fn thread_archive(&self) -> bool {
        self.thread_archive
    }

    pub fn thread_unarchive(&self) -> bool {
        self.thread_unarchive
    }

    pub fn thread_archiving(&self) -> bool {
        self.thread_archive && self.thread_unarchive
    }
}

impl<'a> ThreadArchiveParams<'a> {
    pub(crate) fn new(thread_id: &'a str) -> Self {
        Self { thread_id }
    }
}
