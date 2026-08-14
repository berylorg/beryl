use std::time::Duration;

use beryl_backend::ThreadUnsubscribeStatus;
use tracing::warn;

use super::ThreadTitleBackend;

pub(super) fn cleanup_maintenance_thread<B>(
    backend: &mut B,
    maintenance_thread_id: &str,
    timeout: Duration,
) where
    B: ThreadTitleBackend,
{
    match backend.unsubscribe_thread(maintenance_thread_id, timeout) {
        Ok(response) => {
            if !matches!(
                response.status,
                ThreadUnsubscribeStatus::Unsubscribed | ThreadUnsubscribeStatus::NotLoaded
            ) {
                warn!(
                    maintenance_thread_id = %maintenance_thread_id,
                    status = ?response.status,
                    "thread-title maintenance thread was not subscribed during cleanup"
                );
            }
        }
        Err(error) => {
            warn!(
                maintenance_thread_id = %maintenance_thread_id,
                error = %error,
                "failed to request thread-title maintenance thread cleanup"
            );
        }
    }
}
