use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock, atomic::Ordering},
};

use super::{AdmittedProjectionSession, NEXT_TOKEN, ProviderTestKey};

static PROVIDER_SUBMIT_RECEIVER_LOSS: OnceLock<Mutex<HashMap<ProviderTestKey, u64>>> =
    OnceLock::new();

/// Keeps one exact broker's next provider submit armed for typed receiver loss.
pub struct ProviderSubmitReceiverLossController {
    key: ProviderTestKey,
    token: u64,
}

/// Makes this exact broker's next provider operation return its ownership as `ReceiverLost`.
pub fn install_provider_submit_receiver_loss(
    session: &AdmittedProjectionSession,
) -> ProviderSubmitReceiverLossController {
    let key = session.provider_test_key();
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    receiver_loss_registry()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(key, token);
    ProviderSubmitReceiverLossController { key, token }
}

impl Drop for ProviderSubmitReceiverLossController {
    fn drop(&mut self) {
        let mut registry = receiver_loss_registry()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if registry.get(&self.key) == Some(&self.token) {
            registry.remove(&self.key);
        }
    }
}

pub(crate) fn take_provider_submit_receiver_loss(key: ProviderTestKey) -> bool {
    receiver_loss_registry()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .remove(&key)
        .is_some()
}

fn receiver_loss_registry() -> &'static Mutex<HashMap<ProviderTestKey, u64>> {
    PROVIDER_SUBMIT_RECEIVER_LOSS.get_or_init(|| Mutex::new(HashMap::new()))
}
