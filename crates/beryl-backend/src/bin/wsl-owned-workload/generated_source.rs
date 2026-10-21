use std::{error::Error, time::Duration};

use serde_json::Value;
use soketto::handshake::{Server, server::Response};
use tokio_util::compat::TokioAsyncReadCompatExt;

pub fn run(address: &str, token_path: &str) -> Result<(), Box<dyn Error>> {
    let token = std::fs::read_to_string(token_path)?;
    if token.len() != 64 {
        return Err("invalid fixture bearer".into());
    }
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        let listener = tokio::net::TcpListener::bind(address).await?;
        let (socket, _) = tokio::time::timeout(Duration::from_secs(10), listener.accept()).await??;
        let mut header = [0; 8192];
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let length = socket.peek(&mut header).await?;
                if length == 0 { return Err(std::io::Error::other("fixture handshake closed")); }
                if let Some(end) = header[..length].windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let headers = std::str::from_utf8(&header[..end]).map_err(std::io::Error::other)?;
                    if !headers.lines().any(|line| line.split_once(':').is_some_and(|(name, value)| name.eq_ignore_ascii_case("authorization") && value.trim() == format!("Bearer {token}"))) {
                        return Err(std::io::Error::other("fixture authorization mismatch"));
                    }
                    return Ok(());
                }
                if length == header.len() { return Err(std::io::Error::other("fixture handshake oversized")); }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }).await??;
        let mut handshake = Server::new(socket.compat());
        let key = handshake.receive_request().await?.key();
        handshake.send_response(&Response::Accept { key, protocol: None }).await?;
        let mut builder = handshake.into_builder();
        builder.set_max_message_size(16_384);
        builder.set_max_frame_size(16_384);
        let (mut sender, mut receiver) = builder.finish();
        let mut bytes = Vec::new();
        loop {
            bytes.clear();
            if receiver.receive_data(&mut bytes).await.is_err() { break; }
            let request: Value = serde_json::from_slice(&bytes)?;
            let result = match request["method"].as_str() {
                Some("initialize") => r#"{"userAgent":"beryl/0.146.0","codexHome":"/tmp/fixture-codex","platformFamily":"unix","platformOs":"linux"}"#.to_owned(),
                Some("initialized") => continue,
                Some("config/read") => r#"{"config":{"model":null,"model_reasoning_effort":null,"features":{"multi_agent_v2":{"enabled":true,"expose_spawn_agent_model_overrides":true}}},"origins":{"features.multi_agent_v2.enabled":{"name":{"type":"sessionFlags"},"version":"0"},"features.multi_agent_v2.expose_spawn_agent_model_overrides":{"name":{"type":"sessionFlags"},"version":"0"}}}"#.to_owned(),
                Some("thread/start") => {
                    let cwd = serde_json::to_string(request["params"]["cwd"].as_str().ok_or("missing fixture cwd")?)?;
                    format!(r#"{{"thread":{{"id":"generated-source-thread","extra":null,"sessionId":"generated-source-session","forkedFromId":null,"parentThreadId":null,"preview":"","ephemeral":false,"historyMode":"legacy","modelProvider":"openai","createdAt":1,"updatedAt":2,"recencyAt":null,"status":{{"type":"idle"}},"path":null,"cwd":{cwd},"cliVersion":"0.146.0","source":"appServer","threadSource":null,"agentNickname":null,"agentRole":null,"gitInfo":null,"name":null,"turns":[]}},"model":"fixture-model","modelProvider":"openai","serviceTier":null,"cwd":{cwd},"runtimeWorkspaceRoots":[],"instructionSources":[],"approvalPolicy":"never","approvalsReviewer":"user","sandbox":{{}},"activePermissionProfile":null,"reasoningEffort":null,"multiAgentMode":"explicitRequestOnly"}}"#)
                },
                Some("thread/unsubscribe") => r#"{"status":"unsubscribed"}"#.to_owned(),
                _ => return Err("unexpected fixed fixture method".into()),
            };
            let id = request["id"].as_u64().ok_or("invalid fixture request id")?;
            sender.send_text(format!(r#"{{"id":{id},"result":{result}}}"#)).await?;
            sender.flush().await?;
        }
        Ok::<(), Box<dyn Error>>(())
    })
}
