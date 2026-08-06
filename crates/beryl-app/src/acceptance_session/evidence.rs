use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::Serialize;
use tempfile::NamedTempFile;

use crate::{acceptance_digest::Sha256, diagnostic_child_supervisor::DiagnosticStderrSnapshot};

use super::{
    ACCEPTANCE_EVIDENCE_SCHEMA_VERSION, AcceptanceLaunchMode, AcceptanceSessionConfig,
    AcceptanceSessionError,
};

const MAX_PUBLICATION_ERROR_BYTES: usize = 4 * 1024;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceEvidence {
    pub schema_version: u32,
    pub run_identity: String,
    pub started_at_unix_millis: u64,
    pub finished_at_unix_millis: u64,
    pub duration_millis: u64,
    pub launch_mode: AcceptanceLaunchMode,
    pub fixture: AcceptanceFixtureEvidence,
    pub limits: AcceptanceLimitsEvidence,
    pub process: AcceptanceProcessEvidence,
    pub requests: Vec<AcceptanceRequestEvidence>,
    pub stderr: AcceptanceStderrEvidence,
    pub cleanup: AcceptanceCleanupEvidence,
    pub publication: AcceptancePublicationEvidence,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceFixtureEvidence {
    pub executable_path: PathBuf,
    pub executable_bytes: u64,
    pub executable_sha256: String,
    pub isolated_home: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_workspace: Option<PathBuf>,
    pub evidence_path: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceLimitsEvidence {
    pub startup_timeout_millis: u64,
    pub request_timeout_millis: u64,
    pub runtime_timeout_millis: u64,
    pub max_requests: usize,
    pub max_output_bytes: usize,
    pub cleanup_timeout_millis: u64,
    pub recovery_cleanup_timeout_millis: u64,
    pub graceful_cleanup_millis: u64,
    pub termination_phase_millis: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceProcessEvidence {
    pub diagnostic_child_pid: u32,
    pub executable_path: PathBuf,
    pub isolated_home: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_workspace: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceKnownProcessIdentityEvidence {
    pub pid: u32,
    pub executable_path: PathBuf,
    pub isolated_home: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceRequestEvidence {
    pub sequence: usize,
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_identity_range: Option<AcceptanceProtocolIdentityRangeEvidence>,
    pub command: String,
    pub params_serialized_bytes: usize,
    pub params_sha256: String,
    pub timeout_millis: u64,
    pub duration_millis: u64,
    pub outcome: String,
    pub response: Option<AcceptanceResponseEvidence>,
    pub error: Option<AcceptancePayloadEvidence>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceProtocolIdentityRangeEvidence {
    pub first_request_id: String,
    pub last_request_id: String,
    pub count: usize,
}

#[derive(Default)]
pub(super) struct ProtocolIdentityRangeBuilder {
    first_request_id: Option<String>,
    last_request_id: Option<String>,
    count: usize,
}

impl ProtocolIdentityRangeBuilder {
    pub(super) fn observe(&mut self, request_id: Option<&str>) {
        let Some(request_id) = request_id else {
            return;
        };
        if let Some(last_request_id) = self.last_request_id.as_deref() {
            let expected = last_request_id
                .parse::<u64>()
                .expect("diagnostic request identities are decimal u64")
                .saturating_add(1)
                .to_string();
            assert_eq!(
                request_id, expected,
                "diagnostic request identities are contiguous"
            );
        } else {
            self.first_request_id = Some(request_id.to_string());
        }
        self.last_request_id = Some(request_id.to_string());
        self.count += 1;
    }

    pub(super) fn last_request_id(&self) -> Option<&str> {
        self.last_request_id.as_deref()
    }

    pub(super) fn finish(self) -> Option<AcceptanceProtocolIdentityRangeEvidence> {
        Some(AcceptanceProtocolIdentityRangeEvidence {
            first_request_id: self.first_request_id?,
            last_request_id: self
                .last_request_id
                .expect("identity range with a first id has a last id"),
            count: self.count,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptancePayloadEvidence {
    pub total_bytes: usize,
    pub sha256: String,
    pub bounded_prefix: String,
    pub prefix_bytes: usize,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceResponseEvidence {
    pub serialized_bytes: usize,
    pub sha256: String,
    pub bounded_prefix: String,
    pub prefix_bytes: usize,
    pub truncated: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceStderrEvidence {
    pub total_bytes: u64,
    pub sha256: String,
    pub bounded_prefix: String,
    pub prefix_bytes: usize,
    pub truncated: bool,
    pub capture_complete: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceCleanupEvidence {
    pub exact_process_tree_termination_available: bool,
    pub attempts: Vec<AcceptanceCleanupAttemptEvidence>,
    pub final_state: String,
    pub retained_process: Option<AcceptanceKnownProcessIdentityEvidence>,
    pub external_state_swept: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceCleanupAttemptEvidence {
    pub ordinal: usize,
    pub budget_millis: u64,
    pub duration_millis: u64,
    pub phase: String,
    pub termination_method: String,
    pub error: Option<AcceptancePayloadEvidence>,
    pub known_process: AcceptanceKnownProcessIdentityEvidence,
    pub residue: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptancePublicationEvidence {
    pub outcome: String,
    pub error: Option<AcceptancePayloadEvidence>,
}

pub(super) struct PendingCleanupAttemptEvidence {
    pub ordinal: usize,
    pub budget_millis: u64,
    pub duration_millis: u64,
    pub phase: String,
    pub termination_method: String,
    pub error: Option<String>,
    pub known_process: AcceptanceKnownProcessIdentityEvidence,
    pub residue: String,
}

pub(super) struct EvidenceBuilder {
    pub evidence: AcceptanceEvidence,
    output_bytes_remaining: usize,
}

impl EvidenceBuilder {
    pub(super) fn new(
        config: &AcceptanceSessionConfig,
        executable_path: PathBuf,
        executable_bytes: u64,
        executable_sha256: String,
        pid: u32,
        started_at_unix_millis: u64,
        graceful_cleanup_millis: u64,
        termination_phase_millis: u64,
    ) -> Self {
        let limits = &config.limits;
        Self {
            evidence: AcceptanceEvidence {
                schema_version: ACCEPTANCE_EVIDENCE_SCHEMA_VERSION,
                run_identity: config.run_identity.clone(),
                started_at_unix_millis,
                finished_at_unix_millis: 0,
                duration_millis: 0,
                launch_mode: config.launch_mode,
                fixture: AcceptanceFixtureEvidence {
                    executable_path: executable_path.clone(),
                    executable_bytes,
                    executable_sha256,
                    isolated_home: config.isolated_home.clone(),
                    execution_workspace: config.execution_workspace.clone(),
                    evidence_path: config.evidence_path.clone(),
                },
                limits: limits_evidence(config, graceful_cleanup_millis, termination_phase_millis),
                process: AcceptanceProcessEvidence {
                    diagnostic_child_pid: pid,
                    executable_path,
                    isolated_home: config.isolated_home.clone(),
                    execution_workspace: config.execution_workspace.clone(),
                },
                requests: Vec::with_capacity(limits.max_requests.min(32)),
                stderr: AcceptanceStderrEvidence::default(),
                cleanup: AcceptanceCleanupEvidence {
                    exact_process_tree_termination_available: cfg!(target_os = "windows"),
                    attempts: Vec::with_capacity(2),
                    final_state: "pending".to_string(),
                    retained_process: None,
                    external_state_swept: false,
                },
                publication: AcceptancePublicationEvidence {
                    outcome: "pending".to_string(),
                    error: None,
                },
            },
            output_bytes_remaining: limits.max_output_bytes,
        }
    }

    pub(super) fn record_success(
        &mut self,
        sequence: usize,
        request_id: Option<String>,
        protocol_identity_range: Option<AcceptanceProtocolIdentityRangeEvidence>,
        command: &str,
        serialized_params: &[u8],
        timeout_millis: u64,
        duration_millis: u64,
        serialized_response: &[u8],
    ) {
        let response_text =
            std::str::from_utf8(serialized_response).expect("serde_json responses are valid UTF-8");
        let response = bounded_utf8_prefix(response_text, &mut self.output_bytes_remaining);
        self.evidence.requests.push(AcceptanceRequestEvidence {
            sequence,
            request_id,
            protocol_identity_range,
            command: command.to_string(),
            params_serialized_bytes: serialized_params.len(),
            params_sha256: Sha256::digest_hex(serialized_params),
            timeout_millis,
            duration_millis,
            outcome: "success".to_string(),
            response: Some(AcceptanceResponseEvidence {
                serialized_bytes: serialized_response.len(),
                sha256: Sha256::digest_hex(serialized_response),
                bounded_prefix: response.bounded_prefix,
                prefix_bytes: response.prefix_bytes,
                truncated: response.truncated,
            }),
            error: None,
        });
    }

    pub(super) fn record_error(
        &mut self,
        sequence: usize,
        request_id: Option<String>,
        protocol_identity_range: Option<AcceptanceProtocolIdentityRangeEvidence>,
        command: &str,
        serialized_params: &[u8],
        timeout_millis: u64,
        duration_millis: u64,
        error: &str,
    ) {
        let error = payload_evidence(error, &mut self.output_bytes_remaining);
        self.evidence.requests.push(AcceptanceRequestEvidence {
            sequence,
            request_id,
            protocol_identity_range,
            command: command.to_string(),
            params_serialized_bytes: serialized_params.len(),
            params_sha256: Sha256::digest_hex(serialized_params),
            timeout_millis,
            duration_millis,
            outcome: "error".to_string(),
            response: None,
            error: Some(error),
        });
    }

    pub(super) fn complete_cleanup(
        &mut self,
        finished_at_unix_millis: u64,
        duration_millis: u64,
        attempts: Vec<PendingCleanupAttemptEvidence>,
        retained_process: Option<AcceptanceKnownProcessIdentityEvidence>,
        stderr: DiagnosticStderrSnapshot,
    ) {
        self.evidence.finished_at_unix_millis = finished_at_unix_millis;
        self.evidence.duration_millis = duration_millis;
        self.evidence.cleanup.attempts = attempts
            .into_iter()
            .map(|attempt| AcceptanceCleanupAttemptEvidence {
                ordinal: attempt.ordinal,
                budget_millis: attempt.budget_millis,
                duration_millis: attempt.duration_millis,
                phase: attempt.phase,
                termination_method: attempt.termination_method,
                error: attempt
                    .error
                    .as_deref()
                    .map(|error| payload_evidence(error, &mut self.output_bytes_remaining)),
                known_process: attempt.known_process,
                residue: attempt.residue,
            })
            .collect();
        self.evidence.cleanup.final_state = if retained_process.is_some() {
            "indeterminate".to_string()
        } else {
            "verified_reclaimed".to_string()
        };
        self.evidence.cleanup.retained_process = retained_process;
        self.evidence.stderr = stderr_evidence(stderr, &mut self.output_bytes_remaining);
    }

    pub(super) fn publish(mut self, path: &Path) -> (AcceptanceEvidence, Result<(), String>) {
        self.evidence.publication.outcome = "published".to_string();
        let result: Result<(), String> = (|| {
            let bytes =
                serde_json::to_vec_pretty(&self.evidence).map_err(|error| error.to_string())?;
            let parent = path
                .parent()
                .ok_or_else(|| "evidence path did not have a parent directory".to_string())?;
            let mut pending = NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
            pending
                .write_all(&bytes)
                .and_then(|_| pending.as_file().sync_all())
                .map_err(|error| error.to_string())?;
            pending
                .persist_noclobber(path)
                .map_err(|error| error.error.to_string())?;
            Ok(())
        })();
        if let Err(error) = &result {
            let mut publication_bytes_remaining = MAX_PUBLICATION_ERROR_BYTES;
            let error = payload_evidence(error, &mut publication_bytes_remaining);
            self.evidence.publication.outcome = "failed".to_string();
            self.evidence.publication.error = Some(error);
        }
        (self.evidence, result)
    }
}

pub(super) fn executable_identity(path: &Path) -> Result<(u64, String), AcceptanceSessionError> {
    let mut file = File::open(path).map_err(|source| AcceptanceSessionError::PathIo {
        action: "open executable for hashing",
        path: path.to_path_buf(),
        source,
    })?;
    let mut length = 0_u64;
    let mut sha = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| AcceptanceSessionError::PathIo {
                action: "hash executable",
                path: path.to_path_buf(),
                source,
            })?;
        if read == 0 {
            break;
        }
        length = length
            .checked_add(u64::try_from(read).expect("executable hash buffer length fits u64"))
            .ok_or_else(|| AcceptanceSessionError::PathIo {
                action: "count executable bytes while hashing",
                path: path.to_path_buf(),
                source: std::io::Error::other("executable byte count exceeded u64"),
            })?;
        sha.update(&buffer[..read]);
    }
    Ok((length, sha.finalize_hex()))
}

fn limits_evidence(
    config: &AcceptanceSessionConfig,
    graceful_cleanup_millis: u64,
    termination_phase_millis: u64,
) -> AcceptanceLimitsEvidence {
    let limits = &config.limits;
    AcceptanceLimitsEvidence {
        startup_timeout_millis: millis(limits.startup_timeout),
        request_timeout_millis: millis(limits.request_timeout),
        runtime_timeout_millis: millis(limits.runtime_timeout),
        max_requests: limits.max_requests,
        max_output_bytes: limits.max_output_bytes,
        cleanup_timeout_millis: millis(limits.cleanup_timeout),
        recovery_cleanup_timeout_millis: millis(config.recovery_cleanup_timeout),
        graceful_cleanup_millis,
        termination_phase_millis,
    }
}

fn millis(duration: std::time::Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

struct BoundedPrefix {
    bounded_prefix: String,
    prefix_bytes: usize,
    truncated: bool,
}

fn bounded_utf8_prefix(value: &str, bytes_remaining: &mut usize) -> BoundedPrefix {
    let mut prefix_end = (*bytes_remaining).min(value.len());
    while prefix_end > 0 && !value.is_char_boundary(prefix_end) {
        prefix_end -= 1;
    }
    *bytes_remaining = bytes_remaining.saturating_sub(prefix_end);
    BoundedPrefix {
        bounded_prefix: value[..prefix_end].to_string(),
        prefix_bytes: prefix_end,
        truncated: prefix_end < value.len(),
    }
}

fn payload_evidence(value: &str, bytes_remaining: &mut usize) -> AcceptancePayloadEvidence {
    let prefix = bounded_utf8_prefix(value, bytes_remaining);
    AcceptancePayloadEvidence {
        total_bytes: value.len(),
        sha256: Sha256::digest_hex(value.as_bytes()),
        bounded_prefix: prefix.bounded_prefix,
        prefix_bytes: prefix.prefix_bytes,
        truncated: prefix.truncated,
    }
}

fn stderr_evidence(
    stderr: DiagnosticStderrSnapshot,
    bytes_remaining: &mut usize,
) -> AcceptanceStderrEvidence {
    let retained_raw_bytes = stderr.raw_prefix.len() as u64;
    let (bounded_prefix, consumed_raw_bytes) =
        bounded_lossy_utf8_prefix(&stderr.raw_prefix, *bytes_remaining);
    let prefix_bytes = bounded_prefix.len();
    *bytes_remaining = bytes_remaining.saturating_sub(prefix_bytes);
    AcceptanceStderrEvidence {
        total_bytes: stderr.total_bytes,
        sha256: stderr.sha256,
        bounded_prefix,
        prefix_bytes,
        truncated: !stderr.complete || stderr.truncated || consumed_raw_bytes < retained_raw_bytes,
        capture_complete: stderr.complete,
    }
}

fn bounded_lossy_utf8_prefix(bytes: &[u8], encoded_limit: usize) -> (String, u64) {
    let mut output = String::new();
    let mut consumed = 0;
    while consumed < bytes.len() {
        match std::str::from_utf8(&bytes[consumed..]) {
            Ok(valid) => {
                append_valid_prefix(valid, encoded_limit, &mut output, &mut consumed);
                break;
            }
            Err(error) => {
                let valid = std::str::from_utf8(&bytes[consumed..consumed + error.valid_up_to()])
                    .expect("UTF-8 error prefix is valid");
                if !append_valid_prefix(valid, encoded_limit, &mut output, &mut consumed) {
                    break;
                }
                let invalid_bytes = error.error_len().unwrap_or(bytes.len() - consumed);
                if output.len().saturating_add('\u{FFFD}'.len_utf8()) > encoded_limit {
                    break;
                }
                output.push('\u{FFFD}');
                consumed += invalid_bytes;
            }
        }
    }
    (output, consumed as u64)
}

fn append_valid_prefix(
    valid: &str,
    encoded_limit: usize,
    output: &mut String,
    consumed: &mut usize,
) -> bool {
    for character in valid.chars() {
        if output.len().saturating_add(character.len_utf8()) > encoded_limit {
            return false;
        }
        output.push(character);
        *consumed += character.len_utf8();
    }
    true
}
