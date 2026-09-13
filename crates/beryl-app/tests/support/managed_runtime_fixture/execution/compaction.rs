use super::*;

pub(super) fn wait_release(name: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !std::path::Path::new(name).exists() {
        assert!(std::time::Instant::now() < deadline, "{name} timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}

pub(super) fn serve(socket: &mut WebSocket<TcpStream>) {
    let request = read_json(socket);
    assert_eq!(request["method"], "thread/compact/start");
    assert_eq!(request["params"]["threadId"], THREAD);
    fs::write("compaction-requested", "ready").unwrap();
    wait_release("compaction-begin");
    send_json(socket, json!({"id":request["id"],"result":{}}));
    send_json(
        socket,
        json!({"method":"thread/status/changed","params":{
            "threadId":THREAD,"status":{"type":"active","activeFlags":[]}
        }}),
    );
    send_json(
        socket,
        json!({"method":"turn/started","params":{
            "threadId":THREAD,"turn":{"id":"managed-compaction","items":[],"itemsView":"notLoaded",
                "status":"inProgress","error":null,"startedAt":37_004,"completedAt":null,"durationMs":null}
        }}),
    );
    for (method, field, timestamp) in [
        ("item/started", "startedAtMs", 37_005),
        ("item/completed", "completedAtMs", 37_006),
    ] {
        let mut params = json!({"item":{"type":"contextCompaction","id":"managed-compaction-marker"},
            "threadId":THREAD,"turnId":"managed-compaction"});
        params[field] = json!(timestamp);
        send_json(socket, json!({"method":method,"params":params}));
    }
    fs::write("compaction-started", "ready").unwrap();
    wait_release("compaction-release");
    send_json(
        socket,
        json!({"method":"thread/status/changed","params":{
            "threadId":THREAD,"status":{"type":"idle"}
        }}),
    );
    send_json(
        socket,
        json!({"method":"turn/completed","params":{
            "threadId":THREAD,"turn":{"id":"managed-compaction","items":[],"itemsView":"notLoaded",
                "status":"completed","error":null,"startedAt":37_004,"completedAt":37_007,"durationMs":3}
        }}),
    );
}
