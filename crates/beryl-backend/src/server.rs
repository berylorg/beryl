use std::{
    io, thread,
    time::{Duration, Instant},
};

use crate::{
    BackendWebSocketEndpoint, ManagedBackendError, ManagedBackendSession,
    session::ManagedBackendClientOptions,
};

#[derive(Clone)]
/// A cloneable capability for opening authenticated clients to one live CAS runtime.
///
/// Runtime launch and ownership remain outside this transport value. The target
/// runtime supervisor creates connectors only after admitting an exact configured
/// executable and its authentication boundary.
pub struct ManagedBackendClientConnector {
    endpoint: BackendWebSocketEndpoint,
    authorization_header_value: String,
}

impl ManagedBackendClientConnector {
    /// Returns the admitted WebSocket endpoint for this runtime.
    pub fn endpoint(&self) -> &BackendWebSocketEndpoint {
        &self.endpoint
    }

    /// Opens and initializes a foreground client session.
    pub fn connect_client(
        &self,
        timeout: Duration,
    ) -> Result<ManagedBackendSession, ManagedBackendError> {
        self.connect_client_with_options(ManagedBackendClientOptions::foreground(), timeout)
    }

    /// Opens and initializes a client session with explicit client options.
    pub fn connect_client_with_options(
        &self,
        options: ManagedBackendClientOptions,
        timeout: Duration,
    ) -> Result<ManagedBackendSession, ManagedBackendError> {
        let mut session = self.connect_client_uninitialized_until(timeout)?;
        session.initialize_client_with_options(&options, timeout)?;
        Ok(session)
    }

    /// Opens and initializes a request-only client session.
    pub fn connect_request_client(
        &self,
        timeout: Duration,
    ) -> Result<ManagedBackendSession, ManagedBackendError> {
        self.connect_client_with_options(ManagedBackendClientOptions::request_only(), timeout)
    }

    fn connect_client_uninitialized_until(
        &self,
        timeout: Duration,
    ) -> Result<ManagedBackendSession, ManagedBackendError> {
        let deadline = Instant::now() + timeout;

        loop {
            match ManagedBackendSession::connect_websocket_uninitialized(
                self.endpoint.clone(),
                self.authorization_header_value.clone(),
            ) {
                Ok(session) => return Ok(session),
                Err(error) if retry_websocket_connect(&error) => {
                    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                        return Err(error);
                    };
                    thread::sleep(remaining.min(Duration::from_millis(50)));
                }
                Err(error) => return Err(error),
            }
        }
    }
}

impl std::fmt::Debug for ManagedBackendClientConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedBackendClientConnector")
            .field("endpoint", &self.endpoint)
            .field("authorization_header_value", &"<redacted>")
            .finish()
    }
}

fn retry_websocket_connect(error: &ManagedBackendError) -> bool {
    let ManagedBackendError::ConnectWebSocket { source, .. } = error else {
        return false;
    };
    matches!(
        source.io_error_kind(),
        Some(
            io::ErrorKind::ConnectionRefused
                | io::ErrorKind::NotConnected
                | io::ErrorKind::TimedOut
                | io::ErrorKind::WouldBlock
        )
    )
}
