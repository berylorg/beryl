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
