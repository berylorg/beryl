use std::sync::Mutex;

use beryl_home_store::HomeStore;

use crate::SyndicStorage;

type Hook = Box<dyn FnOnce(&HomeStore, SyndicStorage) + Send>;

static HOOK: Mutex<Option<Hook>> = Mutex::new(None);

pub fn arm_draft_piece_candidate_read_fault(
    hook: impl FnOnce(&HomeStore, SyndicStorage) + Send + 'static,
) {
    let mut slot = HOOK.lock().expect("draft-piece fault hook lock is healthy");
    assert!(slot.is_none());
    *slot = Some(Box::new(hook));
}

pub(crate) fn run_draft_piece_candidate_read_fault(store: &HomeStore, storage: SyndicStorage) {
    let hook = HOOK
        .lock()
        .expect("draft-piece fault hook lock is healthy")
        .take();
    if let Some(hook) = hook {
        hook(store, storage);
    }
}
