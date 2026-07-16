use serde_json::{Value, json};

use super::{
    ServerSocket, assert_request_header, read_json, send_error, send_notification, send_result,
    send_server_request,
};

#[derive(Clone, Debug)]
pub enum TurnStartReply {
    Exact { turn: &'static str },
    ExactThenDisconnect { turn: &'static str },
    Reject { code: i64, message: &'static str },
    WithholdAndDisconnect,
}

impl TurnStartReply {
    pub const fn disconnects(&self) -> bool {
        matches!(
            self,
            Self::ExactThenDisconnect { .. } | Self::WithholdAndDisconnect
        )
    }
}

#[derive(Clone, Debug)]
pub enum TurnStartAction {
    Notification {
        method: &'static str,
        params: Value,
    },
    DynamicToolCall {
        request_id: u64,
        params: Value,
        expected_result: Value,
    },
}

impl TurnStartAction {
    pub fn notification(method: &'static str, params: Value) -> Self {
        Self::Notification { method, params }
    }

    pub fn dynamic_tool_call(request_id: u64, params: Value, expected_result: Value) -> Self {
        Self::DynamicToolCall {
            request_id,
            params,
            expected_result,
        }
    }
}

pub(super) fn handle_turn_start(
    socket: &mut ServerSocket,
    request_id: u64,
    target: &'static str,
    expected_input: &'static str,
    before_reply: Vec<TurnStartAction>,
    reply: TurnStartReply,
    after_reply: Vec<TurnStartAction>,
) -> u64 {
    let request = read_json(socket);
    assert_request_header(&request, request_id, "turn/start");
    assert_eq!(
        request["params"],
        json!({
            "threadId": target,
            "input": [{ "type": "text", "text": expected_input }]
        })
    );

    run_actions(socket, before_reply);
    match reply {
        TurnStartReply::Exact { turn } | TurnStartReply::ExactThenDisconnect { turn } => {
            send_result(
                socket,
                request_id,
                json!({ "turn": { "id": turn, "status": "inProgress" } }),
            );
        }
        TurnStartReply::Reject { code, message } => {
            send_error(socket, request_id, code, message);
        }
        TurnStartReply::WithholdAndDisconnect => {}
    }
    run_actions(socket, after_reply);
    request_id + 1
}

fn run_actions(socket: &mut ServerSocket, actions: Vec<TurnStartAction>) {
    for action in actions {
        match action {
            TurnStartAction::Notification { method, params } => {
                send_notification(socket, method, params);
            }
            TurnStartAction::DynamicToolCall {
                request_id,
                params,
                expected_result,
            } => {
                send_server_request(socket, request_id, "item/tool/call", params);
                let response = read_json(socket);
                assert_eq!(response["jsonrpc"], json!("2.0"));
                assert_eq!(response["id"], json!(request_id));
                assert_eq!(response["result"], expected_result);
                assert!(response.get("error").is_none());
            }
        }
    }
}
