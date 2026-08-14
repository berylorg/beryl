#[allow(dead_code)]
#[path = "../src/backend_failure.rs"]
mod backend_failure;

use std::io;

use beryl_backend::JsonRpcError;
use serde_json::json;

#[test]
fn source_chain_detail_includes_direct_source() {
    let source = io::Error::new(io::ErrorKind::NotFound, "codex executable was not found");

    let detail =
        backend_failure::source_chain_detail("failed to spawn backend process codex", &source);

    assert!(detail.contains("failed to spawn backend process codex"));
    assert!(detail.contains("codex executable was not found"));
}

#[test]
fn json_rpc_error_detail_includes_error_data() {
    let detail = backend_failure::json_rpc_error_detail(&JsonRpcError {
        code: -32603,
        message: "workspace load failed".to_string(),
        data: Some(json!({ "reason": "invalid config" })),
    });

    assert!(detail.contains("JSON-RPC code -32603"));
    assert!(detail.contains("workspace load failed"));
    assert!(detail.contains("invalid config"));
}

#[test]
fn non_empty_user_text_replaces_empty_messages() {
    assert_eq!(
        backend_failure::non_empty_user_text("  ", "fallback detail"),
        "fallback detail"
    );
}
