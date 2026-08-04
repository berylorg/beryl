#[allow(dead_code)]
#[path = "../phase31_bounded_dispatch/support.rs"]
mod support;

mod exact;
mod lifecycle;

use std::net::TcpStream;

use tungstenite::WebSocket;

fn send_thread_read_response(
    socket: &mut WebSocket<TcpStream>,
    id: u64,
    thread_id: &str,
    status: &str,
    nickname: Option<&str>,
    incidental: &str,
) {
    let nickname = nickname
        .map(|value| format!(r#""{value}""#))
        .unwrap_or_else(|| "null".to_owned());
    support::send_json(
        socket,
        &format!(
            r#"{{"id":{id},"result":{{"thread":{{"id":"{thread_id}","extra":null,"sessionId":"session-id","forkedFromId":null,"parentThreadId":null,"preview":"{incidental}","ephemeral":false,"historyMode":"legacy","modelProvider":"openai","createdAt":1,"updatedAt":2,"recencyAt":null,"status":{status},"path":null,"cwd":"C:\\work\\beryl","cliVersion":"0.146.0","source":{{"subAgent":{{"thread_spawn":{{"parent_thread_id":"parent","depth":1,"agent_path":null,"agent_nickname":{nickname},"agent_role":null}}}}}},"threadSource":null,"agentNickname":{nickname},"agentRole":null,"gitInfo":null,"name":null,"turns":[{{"items":[{{"text":"{incidental}"}}]}}]}}}}}}"#,
        ),
    );
}
