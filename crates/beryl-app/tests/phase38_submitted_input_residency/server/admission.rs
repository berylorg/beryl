use std::net::{TcpListener, TcpStream};

use beryl_backend::CompatibilityProbe;
use serde_json::Value;
use tungstenite::{Message, WebSocket, accept_hdr};

use super::{AUTHORIZATION, RunIdentity, TIMEOUT};

pub(super) fn accept_and_prepare(listener: TcpListener, identity: RunIdentity) -> (TcpStream, u64) {
    let (stream, _) = listener.accept().unwrap();
    let mut socket = accept_hdr(
        stream,
        |request: &tungstenite::handshake::server::Request, response| {
            assert_eq!(
                request
                    .headers()
                    .get("authorization")
                    .expect("client supplies authorization")
                    .to_str()
                    .unwrap(),
                AUTHORIZATION,
            );
            Ok(response)
        },
    )
    .unwrap();
    socket.get_mut().set_read_timeout(Some(TIMEOUT)).unwrap();
    socket.get_mut().set_write_timeout(Some(TIMEOUT)).unwrap();
    complete_admission(&mut socket);
    let request_id = complete_projection(&mut socket, identity);
    socket.flush().unwrap();
    (socket.into_inner(), request_id)
}

fn complete_admission(socket: &mut WebSocket<TcpStream>) {
    let initialize = read_json(socket).expect("initialize request");
    assert_eq!(initialize["method"], "initialize");
    let initialize_id = initialize["id"].as_u64().unwrap();
    send_json(
        socket,
        &format!(
            r#"{{"id":{initialize_id},"result":{{"userAgent":"beryl/0.146.0","codexHome":"C:\\codex","platformFamily":"windows","platformOs":"windows"}}}}"#,
        ),
    );
    let initialized = read_json(socket).expect("initialized notification");
    assert_eq!(initialized["method"], "initialized");

    for probe in CompatibilityProbe::ALL {
        let request = read_json(socket).expect("compatibility probe request");
        assert_eq!(request["method"], probe.method());
        let id = request["id"].as_u64().unwrap();
        let response = match probe {
            CompatibilityProbe::ConfigRead => format!(
                r#"{{"id":{id},"result":{{"config":{{"model":"gpt-5.6","model_reasoning_effort":"high","features":{{"multi_agent_v2":{{"enabled":true,"expose_spawn_agent_model_overrides":true}}}}}},"origins":{{"features.multi_agent_v2.enabled":{{"name":{{"type":"sessionFlags"}},"version":"0"}},"features.multi_agent_v2.expose_spawn_agent_model_overrides":{{"name":{{"type":"sessionFlags"}},"version":"0"}}}}}}}}"#,
            ),
            CompatibilityProbe::ModelList => {
                format!(r#"{{"id":{id},"result":{{"data":[],"nextCursor":null}}}}"#)
            }
            CompatibilityProbe::ThreadUnsubscribe => {
                format!(r#"{{"id":{id},"result":{{"status":"notLoaded"}}}}"#)
            }
            _ => format!(r#"{{"error":{{"code":-32600,"message":"recognized"}},"id":{id}}}"#,),
        };
        send_json(socket, &response);
    }
}

fn complete_projection(socket: &mut WebSocket<TcpStream>, identity: RunIdentity) -> u64 {
    let request = read_json(socket).expect("projection thread/start request");
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "thread/start");
    assert_eq!(request["params"]["ephemeral"], false);
    assert!(request["params"].get("input").is_none());
    let id = request["id"].as_u64().unwrap();
    send_json(
        socket,
        &format!(
            r#"{{"id":{id},"result":{{"thread":{{"id":"{}","extra":null,"sessionId":"session-id","forkedFromId":null,"parentThreadId":null,"preview":"preview","ephemeral":false,"historyMode":"legacy","modelProvider":"openai","createdAt":1,"updatedAt":2,"recencyAt":null,"status":{{"type":"idle"}},"path":null,"cwd":"C:\\work\\beryl","cliVersion":"0.146.0","source":"appServer","threadSource":null,"agentNickname":null,"agentRole":null,"gitInfo":null,"name":null,"turns":[]}},"model":"gpt-5.6","modelProvider":"openai","serviceTier":null,"cwd":"C:\\work\\beryl","runtimeWorkspaceRoots":[],"instructionSources":[],"approvalPolicy":"never","approvalsReviewer":"user","sandbox":{{}},"activePermissionProfile":null,"reasoningEffort":"high","multiAgentMode":"explicitRequestOnly"}}}}"#,
            identity.thread_id(),
        ),
    );
    id.checked_add(1)
        .expect("ordinary request id follows projection request")
}

fn read_json(socket: &mut WebSocket<TcpStream>) -> Option<Value> {
    loop {
        match socket.read() {
            Ok(Message::Text(text)) => return Some(serde_json::from_str(&text).unwrap()),
            Ok(Message::Close(_)) => return None,
            Ok(Message::Ping(payload)) => socket.send(Message::Pong(payload)).unwrap(),
            Ok(Message::Pong(_) | Message::Frame(_)) => {}
            Ok(Message::Binary(bytes)) => panic!("unexpected binary control message: {bytes:?}"),
            Err(error) => panic!("phase38 setup read failed: {error}"),
        }
    }
}

fn send_json(socket: &mut WebSocket<TcpStream>, value: &str) {
    socket.send(Message::Text(value.into())).unwrap();
}
