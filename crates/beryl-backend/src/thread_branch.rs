use crate::JsonRpcError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadBranchCapabilityProbe {
    ThreadFork,
    ThreadRollback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadBranchCapabilityReport {
    probe_results: Vec<ThreadBranchCapabilityProbeResult>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadBranchCapabilityProbeResult {
    probe: ThreadBranchCapabilityProbe,
    supported: bool,
    error: Option<JsonRpcError>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadBranchCapabilities {
    thread_fork: bool,
    thread_rollback: bool,
}

pub(crate) const THREAD_BRANCH_CAPABILITY_PROBES: &[ThreadBranchCapabilityProbe] = &[
    ThreadBranchCapabilityProbe::ThreadFork,
    ThreadBranchCapabilityProbe::ThreadRollback,
];

impl ThreadBranchCapabilityProbe {
    pub fn method(self) -> &'static str {
        match self {
            Self::ThreadFork => "thread/fork",
            Self::ThreadRollback => "thread/rollback",
        }
    }
}

impl ThreadBranchCapabilityReport {
    pub(crate) fn new(probe_results: Vec<ThreadBranchCapabilityProbeResult>) -> Self {
        Self { probe_results }
    }

    pub fn probe_results(&self) -> &[ThreadBranchCapabilityProbeResult] {
        &self.probe_results
    }

    pub fn capabilities(&self) -> ThreadBranchCapabilities {
        let mut capabilities = ThreadBranchCapabilities::default();

        for result in &self.probe_results {
            match result.probe {
                ThreadBranchCapabilityProbe::ThreadFork => {
                    capabilities.thread_fork = result.supported;
                }
                ThreadBranchCapabilityProbe::ThreadRollback => {
                    capabilities.thread_rollback = result.supported;
                }
            }
        }

        capabilities
    }
}

impl ThreadBranchCapabilityProbeResult {
    pub(crate) fn for_supported_probe(probe: ThreadBranchCapabilityProbe) -> Self {
        Self {
            probe,
            supported: true,
            error: None,
        }
    }

    pub(crate) fn unsupported(probe: ThreadBranchCapabilityProbe, error: JsonRpcError) -> Self {
        Self {
            probe,
            supported: false,
            error: Some(error),
        }
    }

    pub fn probe(&self) -> ThreadBranchCapabilityProbe {
        self.probe
    }

    pub fn supported(&self) -> bool {
        self.supported
    }

    pub fn error(&self) -> Option<&JsonRpcError> {
        self.error.as_ref()
    }
}

impl ThreadBranchCapabilities {
    pub fn new(thread_fork: bool, thread_rollback: bool) -> Self {
        Self {
            thread_fork,
            thread_rollback,
        }
    }

    pub fn thread_fork(&self) -> bool {
        self.thread_fork
    }

    pub fn thread_rollback(&self) -> bool {
        self.thread_rollback
    }

    pub fn thread_branching(&self) -> bool {
        self.thread_fork && self.thread_rollback
    }
}
