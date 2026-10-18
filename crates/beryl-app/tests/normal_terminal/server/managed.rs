use super::*;
use std::path::{Path, PathBuf};

impl NormalTerminalServer {
    pub fn spawn_managed_controlled_connection_loss(root: PathBuf) -> Self {
        Self::spawn_scenario(ServerScenario::ManagedControlledConnectionLoss { root })
    }

    pub fn spawn_managed_admission_only(root: PathBuf) -> Self {
        Self::spawn_scenario(ServerScenario::ManagedAdmissionOnly { root })
    }
}

pub(super) fn accept_managed_pair(
    listener: TcpListener,
    root: &Path,
) -> (WebSocket<TcpStream>, thread::JoinHandle<()>) {
    let warmup = authenticated(&listener, root);
    warmup
        .get_ref()
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    let warmup = thread::Builder::new()
        .name("context-managed-warmup".to_owned())
        .spawn(move || {
            let mut socket = warmup;
            complete_admission(&mut socket);
            read_until_close(&mut socket).unwrap();
        })
        .unwrap();
    (authenticated(&listener, root), warmup)
}

pub(super) fn authenticated(listener: &TcpListener, root: &Path) -> WebSocket<TcpStream> {
    let (stream, _) = listener.accept().unwrap();
    stream.set_read_timeout(Some(TIMEOUT)).unwrap();
    stream.set_write_timeout(Some(TIMEOUT)).unwrap();
    let authorization = std::fs::read_to_string(root.join("context-proxy-authorization")).unwrap();
    accept_hdr(
        stream,
        |request: &tungstenite::handshake::server::Request, response| {
            assert_eq!(
                request
                    .headers()
                    .get("authorization")
                    .unwrap()
                    .to_str()
                    .unwrap(),
                authorization,
            );
            Ok(response)
        },
    )
    .unwrap()
}
