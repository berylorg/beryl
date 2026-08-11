use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
fn production_sources_exclude_removed_whole_dom_ordinary_apis() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let removed_names = [
        "parse_turn_stream_event",
        "TurnStreamEvent",
        "BackendEvent",
        "ProtocolPhase",
        "ToolActivityEvent",
        "ToolActivityAgentLabel",
        "ToolActivityCollabAgentSpawnMetadata",
        "ToolActivityFileChangeSummary",
        "ToolActivityLifecycle",
        "ToolActivitySource",
        "mod activity;",
        "pub use activity",
        "ThreadSummary",
        "ThreadItem",
        "ItemDeltaPayload",
        "ItemDelta",
        "UserMessageItem",
        "GenericThreadItem",
        "RawCapture",
    ];
    let root_dom_patterns = [
        "serde_json::Value",
        "serde_json::{Value",
        "serde_json::from_value",
        "serde_json::from_slice",
        "serde_json::from_str",
        "serde_json::from_reader",
        "serde_json::Deserializer",
        "serde_json::value::RawValue",
        "Value::deserialize",
    ];
    let mut offenders = Vec::new();

    for removed_path in [src_dir.join("activity.rs"), src_dir.join("activity")] {
        if removed_path.exists() {
            offenders.push(format!(
                "{} retains the orphan activity aggregate module",
                removed_path.display()
            ));
        }
    }

    for path in rust_files_under(&src_dir) {
        let source = fs::read_to_string(&path).expect("production source should be readable");
        let relative = path.strip_prefix(&src_dir).unwrap_or(&path);

        for removed in removed_names {
            if source.contains(removed) {
                offenders.push(format!("{} contains {removed}", relative.display()));
            }
        }

        if is_provider_ingress_source(relative) {
            let compact = source.split_whitespace().collect::<String>();
            for root_dom in root_dom_patterns {
                if compact.contains(root_dom) {
                    offenders.push(format!("{} contains {root_dom}", relative.display()));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "production ordinary-ingress boundary violations remain: {offenders:?}"
    );
}

#[test]
fn foreground_websocket_ingress_calls_only_the_incremental_decoder() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let provider_path = src_dir.join("websocket_transport").join("provider.rs");
    let provider =
        fs::read_to_string(provider_path).expect("WebSocket provider source is readable");
    let reader = "let reader = WebSocketPayloadReader::new(";
    let incremental = "incoming_json::decode_reader_with_provider(";

    let reader_position = provider
        .find(reader)
        .expect("WebSocket ingress must construct its bounded payload reader");
    let decoder_position = provider
        .find(incremental)
        .expect("WebSocket ingress must invoke the incremental decoder");
    assert!(
        reader_position < decoder_position,
        "the bounded WebSocket reader must feed the incremental decoder"
    );
    assert_eq!(
        provider.matches(incremental).count(),
        1,
        "WebSocket ingress must have one incremental decoder entry"
    );

    for forbidden in [
        "incoming_json::decode_reader(",
        "serde_json::Value",
        "serde_json::from_",
        "serde_json::Deserializer",
        "read_to_end(",
    ] {
        assert!(
            !provider.contains(forbidden),
            "WebSocket provider ingress contains root-DOM route {forbidden}"
        );
    }

    let transport = fs::read_to_string(src_dir.join("websocket_transport.rs"))
        .expect("WebSocket transport source is readable");
    let foreground_start = transport
        .find("impl ForegroundWebSocketTransport")
        .expect("foreground WebSocket transport implementation must exist");
    let foreground_end = transport[foreground_start..]
        .find("impl RequestOnlyWebSocketTransport")
        .map(|offset| foreground_start + offset)
        .expect("request-only implementation must follow foreground implementation");
    let foreground = &transport[foreground_start..foreground_end];
    assert!(
        foreground.contains("self.inner.recv_json_value_timeout("),
        "foreground WebSocket reads must use the shared incremental receive implementation"
    );
}

#[test]
fn production_sources_exclude_runtime_probe_admission_and_target_touching_regressions() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let removed_probe_symbols = [
        "CompatibilityProbe",
        "CompatibilityProbeSet",
        "CompatibilityProbeResult",
        "ManagedBackendProbeReport",
        "probe_compatibility",
        "CompatibilityRequest",
        "CompatibilityResultMachine",
        "CompatibilityProbeRecognized",
        "CompatibilityMutatingSuccess",
        "CompatibilityUnsafeSuccess",
        "CompatibilityManagedLaunchProvenanceMissing",
        "CompatibilityEffectiveConfigUnproven",
        "ThreadBranchCapabilities",
        "BoundedResponseResult::Compatibility",
        "ResponseFamily::Compatibility",
        "compatibility admission",
        "Compatibility admission",
    ];
    let mut offenders = Vec::new();

    for path in rust_files_under(&src_dir) {
        let source = fs::read_to_string(&path).expect("production source should be readable");
        let relative = path.strip_prefix(&src_dir).unwrap_or(&path);
        let display = relative.display();

        if relative == Path::new("thread_branch.rs")
            || relative.ends_with(Path::new("compatibility.rs"))
        {
            offenders.push(format!("{display} retains a removed probe module"));
        }

        for symbol in removed_probe_symbols {
            if source.contains(symbol) {
                offenders.push(format!("{display} retains removed probe surface {symbol}"));
            }
        }
    }

    let bounded_request = fs::read_to_string(src_dir.join("session").join("bounded_request.rs"))
        .expect("bounded request source should be readable");
    let (_, after_admission) = bounded_request
        .split_once("pub fn admit_release(")
        .expect("production release admission must remain explicit");
    let (admission, _) = after_admission
        .split_once("/// Exercises the exact release-admission request protocol")
        .expect("lifecycle seam must follow production release admission");

    assert!(
        admission.contains("self.read_config(cwd, timeout)?"),
        "release admission must obtain its proof through one same-session config/read"
    );
    for forbidden in [
        "list_model",
        "start_turn",
        "steer_turn",
        "compact_",
        "unsubscribe_thread",
        "thread_id",
        "turn_id",
        "thread/",
        "turn/",
    ] {
        if admission.contains(forbidden) {
            offenders.push(format!(
                "release admission contains target-touching or product request surface {forbidden}"
            ));
        }
    }

    assert!(
        offenders.is_empty(),
        "runtime probe admission or target-touching regressions remain: {offenders:?}"
    );
}

#[test]
fn production_sources_exclude_removed_hard_stop_and_coarse_cleanup_surfaces() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert!(!src_dir.join("hard_stop.rs").exists());
    assert!(!src_dir.join("persistent_failure_interrupt.rs").exists());

    let forbidden = [
        "CoarseThreadCleanup",
        "SameSessionCleanupOrdering",
        "ExactHardStopLimitation",
        "clean_exact_thread_background_terminals",
        "admits_exact_thread_background_terminals_cleanup",
        "ThreadBackgroundTerminalsClean",
        "thread/backgroundTerminals/clean",
        "experimental_api_negotiated",
        "PersistentFailureInterrupt",
    ];
    let mut offenders = Vec::new();
    for path in rust_files_under(&src_dir) {
        let source = fs::read_to_string(&path).expect("production source should be readable");
        let relative = path.strip_prefix(&src_dir).unwrap_or(&path);
        for removed in forbidden {
            if source.contains(removed) {
                offenders.push(format!("{} retains {removed}", relative.display()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "removed hard-stop or coarse-cleanup surfaces remain: {offenders:?}"
    );
}

fn is_provider_ingress_source(relative: &Path) -> bool {
    relative == Path::new("incoming_json.rs")
        || relative.starts_with("incoming_json")
        || relative == Path::new("websocket_transport.rs")
        || relative.starts_with("websocket_transport")
        || relative == Path::new("session.rs")
        || relative.starts_with("session")
}

fn rust_files_under(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();

    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path).expect("source directory should be readable") {
            let entry = entry.expect("source directory entry should be readable");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    files
}
