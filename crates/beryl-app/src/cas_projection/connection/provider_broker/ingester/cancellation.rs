use std::sync::{Arc, Weak};

use super::ProviderBrokerControl;

pub(in crate::cas_projection::connection) struct ProviderIngressCancellation {
    broker: Weak<ProviderBrokerControl>,
}

impl ProviderIngressCancellation {
    pub(in crate::cas_projection::connection) fn request(&self) {
        if let Some(broker) = self.broker.upgrade() {
            broker.request_cancel();
        }
    }
}

impl ProviderBrokerControl {
    pub(in crate::cas_projection::connection) fn cancellation_handle(
        self: &Arc<Self>,
    ) -> ProviderIngressCancellation {
        ProviderIngressCancellation {
            broker: Arc::downgrade(self),
        }
    }
}
