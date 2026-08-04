use std::sync::{Arc, Mutex, Weak};

use crate::cas_projection::{
    ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionError, ScheduledOrdinaryAdmissionResult,
    ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionProviderFactory,
    ScheduledOrdinaryExecutionUnavailable, ScheduledOrdinaryProviderEpochContext,
};

type ProviderEpochControl = Arc<Mutex<Option<Box<dyn ScheduledOrdinaryExecutionProvider>>>>;
type WeakProviderEpochControl = Weak<Mutex<Option<Box<dyn ScheduledOrdinaryExecutionProvider>>>>;

/// Sole idempotent owner of the process-wide provider factory and its stable session pool.
pub(super) struct ProviderFactoryOwner {
    inner: Option<Box<dyn ScheduledOrdinaryExecutionProviderFactory>>,
    epoch_controls: Vec<WeakProviderEpochControl>,
}

/// Idempotent shutdown fence around one factory-issued service-epoch provider.
///
/// Service construction has several fallible steps before the provider reaches the service's
/// ordinary shutdown path. This guard returns the epoch checkout to the process factory even when
/// one of those steps rejects the service.
pub(super) struct FactoryEpochProvider {
    control: ProviderEpochControl,
}

impl ProviderFactoryOwner {
    pub(super) fn new(factory: Box<dyn ScheduledOrdinaryExecutionProviderFactory>) -> Self {
        Self {
            inner: Some(factory),
            epoch_controls: Vec::new(),
        }
    }

    pub(super) fn create_epoch(
        &mut self,
        context: ScheduledOrdinaryProviderEpochContext,
    ) -> Result<
        Box<dyn ScheduledOrdinaryExecutionProvider>,
        Box<dyn std::error::Error + Send + Sync + 'static>,
    > {
        let provider = self
            .inner
            .as_deref_mut()
            .expect("the process provider factory remains live until final shutdown")
            .create_epoch(context)?;
        Ok(self.track_epoch(provider))
    }

    fn track_epoch(
        &mut self,
        provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
    ) -> Box<dyn ScheduledOrdinaryExecutionProvider> {
        self.epoch_controls
            .retain(|control| control.strong_count() != 0);
        let control = Arc::new(Mutex::new(Some(provider)));
        self.epoch_controls.push(Arc::downgrade(&control));
        Box::new(FactoryEpochProvider { control })
    }

    pub(super) fn shutdown(&mut self) {
        for control in self
            .epoch_controls
            .drain(..)
            .filter_map(|control| control.upgrade())
        {
            FactoryEpochProvider::close_control(&control);
        }
        if let Some(mut factory) = self.inner.take() {
            factory.shutdown();
        }
    }
}

impl Drop for ProviderFactoryOwner {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl FactoryEpochProvider {
    #[cfg(test)]
    pub(super) fn guard(
        provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
    ) -> Box<dyn ScheduledOrdinaryExecutionProvider> {
        Box::new(Self {
            control: Arc::new(Mutex::new(Some(provider))),
        })
    }

    fn close(&mut self) {
        Self::close_control(&self.control);
    }

    fn close_control(control: &ProviderEpochControl) {
        let mut provider = control.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(mut provider) = provider.take() {
            provider.shutdown();
        }
    }
}

impl ScheduledOrdinaryExecutionProvider for FactoryEpochProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        let mut provider = self
            .control
            .lock()
            .map_err(|_| ScheduledOrdinaryAdmissionError::ProviderPoisoned)?;
        match provider.as_mut() {
            Some(provider) => provider.try_issue(admission),
            None => Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::ShuttingDown)),
        }
    }

    fn shutdown(&mut self) {
        self.close();
    }
}

impl Drop for FactoryEpochProvider {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    struct ProviderProbe(Arc<AtomicUsize>);

    struct FactoryProbe(Arc<AtomicUsize>);

    struct OrderedProviderProbe(Arc<Mutex<Vec<&'static str>>>);

    struct OrderedFactoryProbe(Arc<Mutex<Vec<&'static str>>>);

    impl ScheduledOrdinaryExecutionProviderFactory for FactoryProbe {
        fn create_epoch(
            &mut self,
            _context: crate::cas_projection::ScheduledOrdinaryProviderEpochContext,
        ) -> Result<
            Box<dyn ScheduledOrdinaryExecutionProvider>,
            Box<dyn std::error::Error + Send + Sync + 'static>,
        > {
            unreachable!("the factory ownership test does not issue an epoch")
        }

        fn shutdown(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl ScheduledOrdinaryExecutionProvider for ProviderProbe {
        fn try_issue(
            &mut self,
            admission: ScheduledOrdinaryAdmission,
        ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
            Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
        }

        fn shutdown(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl ScheduledOrdinaryExecutionProviderFactory for OrderedFactoryProbe {
        fn create_epoch(
            &mut self,
            _context: ScheduledOrdinaryProviderEpochContext,
        ) -> Result<
            Box<dyn ScheduledOrdinaryExecutionProvider>,
            Box<dyn std::error::Error + Send + Sync + 'static>,
        > {
            unreachable!("the ordered owner test injects one already-created epoch")
        }

        fn shutdown(&mut self) {
            let mut events = self.0.lock().unwrap();
            assert_eq!(events.as_slice(), ["epoch"]);
            events.push("factory");
        }
    }

    impl ScheduledOrdinaryExecutionProvider for OrderedProviderProbe {
        fn try_issue(
            &mut self,
            admission: ScheduledOrdinaryAdmission,
        ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
            Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
        }

        fn shutdown(&mut self) {
            self.0.lock().unwrap().push("epoch");
        }
    }

    #[test]
    fn construction_drop_returns_the_factory_epoch_once() {
        let shutdowns = Arc::new(AtomicUsize::new(0));
        drop(FactoryEpochProvider::guard(Box::new(ProviderProbe(
            Arc::clone(&shutdowns),
        ))));

        assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn explicit_shutdown_and_drop_remain_idempotent() {
        let shutdowns = Arc::new(AtomicUsize::new(0));
        let mut provider =
            FactoryEpochProvider::guard(Box::new(ProviderProbe(Arc::clone(&shutdowns))));

        provider.shutdown();
        provider.shutdown();
        drop(provider);

        assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn process_factory_shutdown_and_drop_remain_idempotent() {
        let shutdowns = Arc::new(AtomicUsize::new(0));
        let mut factory = ProviderFactoryOwner::new(Box::new(FactoryProbe(Arc::clone(&shutdowns))));

        factory.shutdown();
        factory.shutdown();
        drop(factory);

        assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn process_factory_fences_a_still_live_epoch_before_releasing_the_pool() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut factory =
            ProviderFactoryOwner::new(Box::new(OrderedFactoryProbe(Arc::clone(&events))));
        let provider = factory.track_epoch(Box::new(OrderedProviderProbe(Arc::clone(&events))));

        factory.shutdown();
        assert_eq!(events.lock().unwrap().as_slice(), ["epoch", "factory"]);
        drop(provider);
        drop(factory);
        assert_eq!(events.lock().unwrap().as_slice(), ["epoch", "factory"]);
    }

    #[test]
    fn process_factory_drop_releases_the_stable_session_pool() {
        let shutdowns = Arc::new(AtomicUsize::new(0));
        drop(ProviderFactoryOwner::new(Box::new(FactoryProbe(
            Arc::clone(&shutdowns),
        ))));

        assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    }
}
