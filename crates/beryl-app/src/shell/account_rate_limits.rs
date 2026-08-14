use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use beryl_backend::{AccountRateLimitsResponse, ManagedBackendClientConnector};

pub(super) enum AccountRateLimitsUpdate {
    Finished(AccountRateLimitsOutcome),
}

pub(super) enum AccountRateLimitsOutcome {
    Loaded(AccountRateLimitsResponse),
    Failed { message: String },
}

pub(super) fn spawn_account_rate_limits_worker(
    connector: ManagedBackendClientConnector,
    timeout: Duration,
) -> Receiver<AccountRateLimitsUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || run_account_rate_limits_worker(connector, timeout, sender));
    receiver
}

fn run_account_rate_limits_worker(
    connector: ManagedBackendClientConnector,
    timeout: Duration,
    sender: Sender<AccountRateLimitsUpdate>,
) {
    let mut session = match connector.connect_request_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            let _ = sender.send(AccountRateLimitsUpdate::Finished(
                AccountRateLimitsOutcome::Failed {
                    message: format!("Beryl could not connect to the managed backend: {error}"),
                },
            ));
            return;
        }
    };

    let outcome = match session.read_account_rate_limits(timeout) {
        Ok(rate_limits) => AccountRateLimitsOutcome::Loaded(rate_limits),
        Err(error) => AccountRateLimitsOutcome::Failed {
            message: format!("Beryl could not read account rate limits: {error}"),
        },
    };

    let _ = sender.send(AccountRateLimitsUpdate::Finished(outcome));
}
