#[allow(dead_code)]
#[path = "../phase31_bounded_dispatch/support.rs"]
mod support;

mod exact;
mod lifecycle;

use std::net::TcpStream;

use tungstenite::WebSocket;

fn send_lineage_response(
    socket: &mut WebSocket<TcpStream>,
    id: u64,
    thread_id: &str,
    status: &str,
    resume: bool,
) {
    let initial_turns_page = if resume {
        r#","initialTurnsPage":null,"turnsBackwardsCursor":null,"itemsBackwardsCursor":null"#
    } else {
        ""
    };
    support::send_json(
        socket,
        &format!(
            r#"{{"id":{id},"result":{{"thread":{{"id":"{thread_id}","extra":null,"sessionId":"session-id","forkedFromId":null,"parentThreadId":null,"preview":"preview","ephemeral":false,"historyMode":"legacy","modelProvider":"openai","createdAt":1,"updatedAt":2,"recencyAt":null,"status":{status},"path":null,"cwd":"C:\\work\\beryl","cliVersion":"0.146.0","source":"appServer","threadSource":null,"agentNickname":null,"agentRole":null,"gitInfo":null,"name":null,"turns":[]}},"model":"gpt-5.6","modelProvider":"openai","serviceTier":null,"cwd":"C:\\work\\beryl","runtimeWorkspaceRoots":[],"instructionSources":[],"approvalPolicy":"never","approvalsReviewer":"user","sandbox":{{}},"activePermissionProfile":null,"reasoningEffort":"high","multiAgentMode":"explicitRequestOnly"{initial_turns_page}}}}}"#,
        ),
    );
}
