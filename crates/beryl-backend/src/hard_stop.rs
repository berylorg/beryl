use crate::JsonRpcError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HardStopCapabilityProbe {
    CommandExecTerminate,
    ThreadBackgroundTerminalsClean,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HardStopCapabilityReport {
    probe_results: Vec<HardStopCapabilityProbeResult>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HardStopCapabilityProbeResult {
    probe: HardStopCapabilityProbe,
    supported: bool,
    error: Option<JsonRpcError>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HardStopCapabilities {
    command_exec_terminate: bool,
    thread_background_terminals_clean: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HardStopTarget {
    Turn { thread_id: String, turn_id: String },
    CommandExecution { process_id: String },
    BackgroundTerminals { thread_id: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HardStopTargetKind {
    Turn,
    CommandExecution,
    BackgroundTerminals,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HardStopTargetOutcome {
    Succeeded {
        target: HardStopTarget,
    },
    Failed {
        target: HardStopTarget,
        method: &'static str,
        message: String,
    },
}

pub(crate) const HARD_STOP_CAPABILITY_PROBES: &[HardStopCapabilityProbe] = &[
    HardStopCapabilityProbe::CommandExecTerminate,
    HardStopCapabilityProbe::ThreadBackgroundTerminalsClean,
];

impl HardStopCapabilityProbe {
    pub fn method(self) -> &'static str {
        match self {
            Self::CommandExecTerminate => "command/exec/terminate",
            Self::ThreadBackgroundTerminalsClean => "thread/backgroundTerminals/clean",
        }
    }
}

impl HardStopCapabilityReport {
    pub(crate) fn new(probe_results: Vec<HardStopCapabilityProbeResult>) -> Self {
        Self { probe_results }
    }

    pub fn probe_results(&self) -> &[HardStopCapabilityProbeResult] {
        &self.probe_results
    }

    pub fn capabilities(&self) -> HardStopCapabilities {
        let mut capabilities = HardStopCapabilities::default();

        for result in &self.probe_results {
            match result.probe {
                HardStopCapabilityProbe::CommandExecTerminate => {
                    capabilities.command_exec_terminate = result.supported;
                }
                HardStopCapabilityProbe::ThreadBackgroundTerminalsClean => {
                    capabilities.thread_background_terminals_clean = result.supported;
                }
            }
        }

        capabilities
    }
}

impl HardStopCapabilityProbeResult {
    pub(crate) fn for_supported_probe(probe: HardStopCapabilityProbe) -> Self {
        Self {
            probe,
            supported: true,
            error: None,
        }
    }

    pub(crate) fn unsupported(probe: HardStopCapabilityProbe, error: JsonRpcError) -> Self {
        Self {
            probe,
            supported: false,
            error: Some(error),
        }
    }

    pub fn probe(&self) -> HardStopCapabilityProbe {
        self.probe
    }

    pub fn supported(&self) -> bool {
        self.supported
    }

    pub fn error(&self) -> Option<&JsonRpcError> {
        self.error.as_ref()
    }
}

impl HardStopCapabilities {
    pub fn new(command_exec_terminate: bool, thread_background_terminals_clean: bool) -> Self {
        Self {
            command_exec_terminate,
            thread_background_terminals_clean,
        }
    }

    pub fn command_exec_terminate(&self) -> bool {
        self.command_exec_terminate
    }

    pub fn thread_background_terminals_clean(&self) -> bool {
        self.thread_background_terminals_clean
    }
}

impl HardStopTarget {
    pub fn turn(thread_id: impl Into<String>, turn_id: impl Into<String>) -> Self {
        Self::Turn {
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
        }
    }

    pub fn command_execution(process_id: impl Into<String>) -> Self {
        Self::CommandExecution {
            process_id: process_id.into(),
        }
    }

    pub fn background_terminals(thread_id: impl Into<String>) -> Self {
        Self::BackgroundTerminals {
            thread_id: thread_id.into(),
        }
    }

    pub fn kind(&self) -> HardStopTargetKind {
        match self {
            Self::Turn { .. } => HardStopTargetKind::Turn,
            Self::CommandExecution { .. } => HardStopTargetKind::CommandExecution,
            Self::BackgroundTerminals { .. } => HardStopTargetKind::BackgroundTerminals,
        }
    }

    pub fn method(&self) -> &'static str {
        match self {
            Self::Turn { .. } => "turn/interrupt",
            Self::CommandExecution { .. } => "command/exec/terminate",
            Self::BackgroundTerminals { .. } => "thread/backgroundTerminals/clean",
        }
    }
}

impl HardStopTargetOutcome {
    pub(crate) fn succeeded(target: HardStopTarget) -> Self {
        Self::Succeeded { target }
    }

    pub(crate) fn failed(
        target: HardStopTarget,
        method: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self::Failed {
            target,
            method,
            message: message.into(),
        }
    }

    pub fn target(&self) -> &HardStopTarget {
        match self {
            Self::Succeeded { target } | Self::Failed { target, .. } => target,
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, Self::Succeeded { .. })
    }
}
