use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

struct OncePanickingReplyWake {
    signal: Weak<QuerySignal>,
    panicked: AtomicBool,
    saw_collection: AtomicBool,
}

impl Wake for OncePanickingReplyWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if !self.panicked.swap(true, Ordering::AcqRel) {
            let signal = self.signal.upgrade().expect("live query service");
            let collection_exists = signal.state.lock().unwrap().collections.len() == 1;
            self.saw_collection
                .store(collection_exists, Ordering::Release);
            assert!(
                collection_exists,
                "first reply must follow State collection admission"
            );
            panic!("registered catalog reply waker injected panic");
        }
    }
}

#[test]
fn registered_first_reply_waker_panic_drains_original_read_and_refuses_late_adoption() {
    let fixture = support::Fixture::new(r"C:\Work\Beryl");
    let (mut query, mut source) = service(&fixture);
    query.publish();
    let pause = pause(&query);
    let request = query
        .reader()
        .open(
            criteria(),
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new(),
        )
        .unwrap();
    pause.entered();
    let caller = Arc::new(OncePanickingReplyWake {
        signal: Arc::downgrade(&query.signal),
        panicked: AtomicBool::new(false),
        saw_collection: AtomicBool::new(false),
    });
    let waker = Waker::from(Arc::clone(&caller));
    let mut context = Context::from_waker(&waker);
    let mut response = Box::pin(request.receive());
    assert!(matches!(
        response.as_mut().poll(&mut context),
        Poll::Pending
    ));
    pause.release();
    until(|| caller.panicked.load(Ordering::Acquire));
    assert!(matches!(
        query.stop_and_join(),
        Err(CatalogQueryServiceError::Panicked)
    ));
    assert!(caller.saw_collection.load(Ordering::Acquire));
    assert!(query.work_drained() && query.reads_drained());
    assert!(query.signal.state.lock().unwrap().collections.is_empty());
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 1);
    assert!(matches!(
        response.as_mut().poll(&mut context),
        Poll::Ready(Err(
            CatalogQueryRequestError::Panicked | CatalogQueryRequestError::Retired
        ))
    ));
    drop(response);
    source.stop_and_join().unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
}
