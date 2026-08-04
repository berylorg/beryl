use std::{fs, path::PathBuf};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn source(path: &str) -> String {
    fs::read_to_string(crate_root().join(path)).expect("mounted source file is readable")
}

fn section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start = source.find(start).expect("section start remains mounted");
    let tail = &source[start..];
    let end = tail.find(end).expect("section end remains mounted");
    &tail[..end]
}

#[test]
fn context_compaction_late_response_matrix_is_closed_and_explicit() {
    let dispatch = source("src/cas_projection/context_compaction/coordinator/dispatch.rs");
    for required in [
        "CompactionRequestTransitionStatus::TerminalAlreadySettled",
        "if !unbind_failed",
        "TerminalResponseDisposition::CompletionUnknown",
        "TerminalResponseDisposition::Rejected",
        "TerminalResponseDisposition::ProvenNondispatch",
        "TerminalResponseReconciliation::InvariantFailure",
        "target.retire_context_compaction_connection()",
    ] {
        assert!(
            dispatch.contains(required),
            "missing late-response cut: {required}"
        );
    }
}

#[test]
fn context_compaction_router_terminal_publication_is_the_only_success_handoff() {
    let dispatch = source("src/cas_projection/context_compaction/coordinator/dispatch.rs");
    let wait = section(&dispatch, "fn await_terminal", "\n    }\n}");
    assert!(wait.contains("LiveEventPoll::Quiet => {}"));
    assert!(wait.contains("LiveEventPoll::ProvenTerminal"));
    assert!(wait.contains("into_proven_terminal_projection"));
    assert!(!wait.contains("Quiet => {\n                    if local.is_finished()"));
}

#[test]
fn context_compaction_startup_cuts_never_replay_or_synthesize_continuation() {
    let recovery = source("src/cas_projection/accepted_delivery_recovery.rs");
    let compaction = section(
        &recovery,
        "fn converge_compaction_restart",
        "\nfn publish_source_less_terminal",
    );
    for required in [
        "CancelBeforeDispatch",
        "FinishLocalNondispatch",
        "RetireRejectedTarget",
        "PossibleDispatch",
        "FinalizeSuccess",
        "FinalizeInterruptedWithIdleEvidence",
        "FinalizeFailure",
        "CompactionSettlement::ManualSuccess",
    ] {
        assert!(
            compaction.contains(required),
            "missing restart cut: {required}"
        );
    }
    assert!(!compaction.contains("SettleLifecycleCompaction"));
    assert!(!compaction.contains("publish_source_less_terminal"));
}

#[test]
fn lifecycle_compaction_uses_atomic_user_work_precedence_and_fixed_content() {
    let settlement = source("src/cas_projection/context_compaction/coordinator/settlement.rs");
    for required in [
        "prepare_lifecycle_continuation_content",
        "current_seal_lifecycle_continuation_content",
        "current_settle_lifecycle_compaction",
        "SettleLifecycleCompaction::new",
        "AcceptedInputWakeReason::ExecutionReady",
    ] {
        assert!(
            settlement.contains(required),
            "missing lifecycle cut: {required}"
        );
    }
    assert!(!settlement.contains("DraftPayloadUpdate"));
}

#[test]
fn compaction_stop_and_loss_stay_on_dedicated_authority() {
    let loss = source("src/cas_projection/connection/provider_broker/loss.rs");
    let router_loss = source("src/cas_projection/connection/router/loss.rs");
    let coordinator = source("src/cas_projection/context_compaction/coordinator.rs");
    let shell_status = source("src/shell/status_operation.rs");
    assert!(loss.contains("registration.compaction()"));
    assert!(loss.contains("abandon_target_loss(compaction)"));
    assert!(router_loss.contains("activation.is_none() && target.compaction.is_none()"));
    assert!(!coordinator.contains("ManagedBackendClientConnector"));
    assert!(!shell_status.contains("ContextCompactionFinished"));
}

#[test]
fn compaction_queue_workers_and_close_cancellation_are_bounded() {
    let coordinator = source("src/cas_projection/context_compaction/coordinator.rs");
    let admission = source("src/cas_projection/context_compaction/coordinator/admission.rs");
    let dispatch = source("src/cas_projection/context_compaction/coordinator/dispatch.rs");
    let settlement = source("src/cas_projection/context_compaction/coordinator/settlement.rs");
    let service = source("src/cas_projection/service.rs");
    assert!(coordinator.contains("const COMPACTION_WORKER_CAPACITY: usize = 8"));
    assert!(coordinator.contains("const COMPACTION_QUEUE_CAPACITY: usize = 64"));
    assert!(coordinator.contains("mpsc::sync_channel(COMPACTION_QUEUE_CAPACITY)"));
    assert!(coordinator.contains("closing: AtomicBool"));
    assert!(coordinator.contains("settlement_fence: Mutex<()>"));
    assert!(admission.contains("sender.try_send(work)"));
    assert!(!admission.contains("sender.send(work)"));
    assert!(admission.contains("into_context_compaction_nondispatch_projection"));
    assert!(dispatch.contains("CompactionSettlement::CancelledBeforeDispatch"));
    assert!(settlement.contains("LifecycleContentFailure::DefinitivePreparation"));
    assert!(settlement.contains("CompactionSettlement::ManualSuccess"));
    assert!(settlement.contains("LifecycleContentFailure::Home"));
    assert!(settlement.contains("take_terminal_lifecycle_yield"));
    assert!(settlement.contains("self.remove_local(local)"));
    assert!(service.contains("context_compaction.request_shutdown()"));
}

#[test]
fn compaction_timeout_validation_is_a_closed_whole_second_range() {
    let model = source("src/cas_projection/context_compaction/coordinator/model.rs");
    assert!(model.contains("const MAX_COMPLETION_TIMEOUT_SECONDS: u64 = 86_400"));
    assert!(model.contains("timeout.subsec_nanos() != 0"));
    assert!(model.contains("1..=MAX_COMPLETION_TIMEOUT_SECONDS"));
}
