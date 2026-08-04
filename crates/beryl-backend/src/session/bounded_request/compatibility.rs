use std::{
    path::Path,
    time::{Duration, Instant},
};

use beryl_model::{CasThreadId, CasTurnId};
use serde::Serialize;

use super::{
    RequestCompletion,
    wire::{CompatibilityRequest, ConfigReadParams, ModelListParams},
};
use crate::{
    BackendConfigDefaults, BoundedResponseResult, CompatibilityProbe, CompatibilityProbeResult,
    CompatibilityProbeSet, JsonRpcErrorVerdict, ManagedBackendError, ManagedBackendProbeReport,
    ManagedBackendSession, ModelListOptions, ThreadBranchCapabilities, ThreadUnsubscribeStatus,
};

const NIL_ID: &str = "00000000-0000-0000-0000-000000000000";
const PROBE_TEXT: &str = "Beryl compatibility probe";

impl ManagedBackendSession {
    /// Executes the fixed eleven-probe admission sequence without retaining proportional results.
    pub fn probe_compatibility(
        &mut self,
        config_cwd: &Path,
        timeout: Duration,
    ) -> Result<ManagedBackendProbeReport, ManagedBackendError> {
        if !self.has_production_managed_launch_provenance() {
            return Err(ManagedBackendError::CompatibilityManagedLaunchProvenanceMissing);
        }
        let managed_launch_provenance = self
            .managed_launch_provenance
            .clone()
            .expect("managed-launch provenance was checked before compatibility admission");
        let initialize = self
            .initialize
            .clone()
            .ok_or(ManagedBackendError::ClientNotInitialized)?;
        let (successes, config_defaults, thread_branch_capabilities) =
            self.execute_compatibility_probe_sequence(config_cwd, timeout)?;

        Ok(ManagedBackendProbeReport::new(
            initialize,
            successes,
            thread_branch_capabilities,
            config_defaults,
            managed_launch_provenance,
        ))
    }

    /// Runs the exact probe wire sequence without creating compatibility admission authority.
    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    pub fn probe_non_authorizing_compatibility_for_lifecycle_test(
        &mut self,
        config_cwd: &Path,
        timeout: Duration,
    ) -> Result<
        (
            CompatibilityProbeSet,
            BackendConfigDefaults,
            ThreadBranchCapabilities,
        ),
        ManagedBackendError,
    > {
        self.execute_compatibility_probe_sequence(config_cwd, timeout)
    }

    fn execute_compatibility_probe_sequence(
        &mut self,
        config_cwd: &Path,
        timeout: Duration,
    ) -> Result<
        (
            CompatibilityProbeSet,
            BackendConfigDefaults,
            ThreadBranchCapabilities,
        ),
        ManagedBackendError,
    > {
        let thread_id = CasThreadId::new(NIL_ID).expect("the fixed nil thread id is bounded");
        let turn_id = CasTurnId::new(NIL_ID).expect("the fixed nil turn id is bounded");
        let deadline = Instant::now() + timeout;
        let mut successes = CompatibilityProbeSet::empty();
        let mut config_defaults = BackendConfigDefaults::default();

        for probe in CompatibilityProbe::ALL {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                self.retire_connection();
                return Err(ManagedBackendError::RequestTimeout {
                    method: probe.method().to_string(),
                    timeout,
                });
            };
            let completion = self
                .dispatch_compatibility_probe(probe, config_cwd, &thread_id, &turn_id, remaining)?;
            match completion {
                RequestCompletion::Rejection(error)
                    if error.verdict()
                        == Some(JsonRpcErrorVerdict::CompatibilityProbeRecognized { probe }) => {}
                RequestCompletion::Rejection(error) => {
                    self.retire_connection();
                    return Err(ManagedBackendError::RequestFailed {
                        method: probe.method().to_string(),
                        error: Box::new(error),
                    });
                }
                RequestCompletion::Response(result) => {
                    let BoundedResponseResult::Compatibility(result) = result else {
                        return self.fail_unexpected_response(probe.method());
                    };
                    if result.probe() != probe {
                        return self.fail_unexpected_response(probe.method());
                    }
                    match result {
                        CompatibilityProbeResult::ConfigRead(config) => {
                            config_defaults = config.into_defaults();
                            if !config_defaults.proves_spawn_agent_model_overrides() {
                                self.retire_connection();
                                return Err(
                                    ManagedBackendError::CompatibilityEffectiveConfigUnproven,
                                );
                            }
                        }
                        CompatibilityProbeResult::ModelList(page) => {
                            drop(page);
                        }
                        CompatibilityProbeResult::ThreadUnsubscribe(status) => {
                            if status != ThreadUnsubscribeStatus::NotLoaded {
                                self.retire_connection();
                                return Err(ManagedBackendError::CompatibilityUnsafeSuccess {
                                    probe,
                                });
                            }
                        }
                        CompatibilityProbeResult::UnexpectedMutatingSuccess(actual) => {
                            self.retire_connection();
                            return Err(ManagedBackendError::CompatibilityMutatingSuccess {
                                probe: actual,
                            });
                        }
                    }
                }
            }
            successes.insert(probe);
        }

        debug_assert!(successes.is_complete());
        Ok((
            successes,
            config_defaults,
            ThreadBranchCapabilities::new(true, true),
        ))
    }

    fn dispatch_compatibility_probe(
        &mut self,
        probe: CompatibilityProbe,
        cwd: &Path,
        thread_id: &CasThreadId,
        turn_id: &CasTurnId,
        timeout: Duration,
    ) -> Result<RequestCompletion, ManagedBackendError> {
        match probe {
            CompatibilityProbe::ConfigRead => self.dispatch_request(
                &CompatibilityRequest::new(probe, ConfigReadParams::new(cwd)),
                timeout,
            ),
            CompatibilityProbe::ModelList => {
                let options = ModelListOptions::default();
                self.dispatch_request(
                    &CompatibilityRequest::new(probe, ModelListParams::new(&options)),
                    timeout,
                )
            }
            CompatibilityProbe::ThreadCompactStart => self.dispatch_request(
                &CompatibilityRequest::new(probe, ThreadIdParams { thread_id }),
                timeout,
            ),
            CompatibilityProbe::ThreadFork => self.dispatch_request(
                &CompatibilityRequest::new(
                    probe,
                    ThreadForkParams {
                        thread_id,
                        cwd,
                        exclude_turns: true,
                        ephemeral: false,
                    },
                ),
                timeout,
            ),
            CompatibilityProbe::ThreadInjectItems => self.dispatch_request(
                &CompatibilityRequest::new(
                    probe,
                    ThreadInjectParams {
                        thread_id,
                        items: [ProbeMessage::new()],
                    },
                ),
                timeout,
            ),
            CompatibilityProbe::ThreadResume => self.dispatch_request(
                &CompatibilityRequest::new(
                    probe,
                    ThreadResumeParams {
                        thread_id,
                        cwd,
                        exclude_turns: true,
                    },
                ),
                timeout,
            ),
            CompatibilityProbe::ThreadRollback => self.dispatch_request(
                &CompatibilityRequest::new(
                    probe,
                    ThreadRollbackParams {
                        thread_id,
                        num_turns: 1,
                    },
                ),
                timeout,
            ),
            CompatibilityProbe::ThreadUnsubscribe => self.dispatch_request(
                &CompatibilityRequest::new(probe, ThreadIdParams { thread_id }),
                timeout,
            ),
            CompatibilityProbe::TurnInterrupt => self.dispatch_request(
                &CompatibilityRequest::new(probe, TurnInterruptParams { thread_id, turn_id }),
                timeout,
            ),
            CompatibilityProbe::TurnStart => self.dispatch_request(
                &CompatibilityRequest::new(
                    probe,
                    TurnStartParams {
                        thread_id,
                        input: [ProbeInput::new()],
                    },
                ),
                timeout,
            ),
            CompatibilityProbe::TurnSteer => self.dispatch_request(
                &CompatibilityRequest::new(
                    probe,
                    TurnSteerParams {
                        thread_id,
                        expected_turn_id: turn_id,
                        input: [ProbeInput::new()],
                    },
                ),
                timeout,
            ),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadIdParams<'a> {
    thread_id: &'a CasThreadId,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadForkParams<'a> {
    thread_id: &'a CasThreadId,
    cwd: &'a Path,
    exclude_turns: bool,
    ephemeral: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadResumeParams<'a> {
    thread_id: &'a CasThreadId,
    cwd: &'a Path,
    exclude_turns: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadRollbackParams<'a> {
    thread_id: &'a CasThreadId,
    num_turns: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnInterruptParams<'a> {
    thread_id: &'a CasThreadId,
    turn_id: &'a CasTurnId,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnStartParams<'a> {
    thread_id: &'a CasThreadId,
    input: [ProbeInput; 1],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnSteerParams<'a> {
    thread_id: &'a CasThreadId,
    expected_turn_id: &'a CasTurnId,
    input: [ProbeInput; 1],
}

#[derive(Serialize)]
struct ProbeInput {
    #[serde(rename = "type")]
    kind: &'static str,
    text: &'static str,
}

impl ProbeInput {
    const fn new() -> Self {
        Self {
            kind: "text",
            text: PROBE_TEXT,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadInjectParams<'a> {
    thread_id: &'a CasThreadId,
    items: [ProbeMessage; 1],
}

#[derive(Serialize)]
struct ProbeMessage {
    #[serde(rename = "type")]
    kind: &'static str,
    role: &'static str,
    content: [ProbeMessageContent; 1],
}

impl ProbeMessage {
    const fn new() -> Self {
        Self {
            kind: "message",
            role: "user",
            content: [ProbeMessageContent {
                kind: "input_text",
                text: PROBE_TEXT,
            }],
        }
    }
}

#[derive(Serialize)]
struct ProbeMessageContent {
    #[serde(rename = "type")]
    kind: &'static str,
    text: &'static str,
}
