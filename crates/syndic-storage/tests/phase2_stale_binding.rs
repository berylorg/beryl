#![cfg(feature = "test-faults")]

mod support;

use beryl_model::{BindingRevision, CasThreadId, ThreadRevision};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use support::*;

#[test]
fn stale_binding_roundtrips_with_its_required_cas_thread_reservation() {
    let home = TestHome::new("stale-binding-roundtrip");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(60);
    let draft = draft_id(61);
    let cas_thread = CasThreadId::new("stale-roundtrip-thread").unwrap();
    let revision = BindingRevision::new(2).unwrap();
    let digest = empty_selected_path_digest();
    let selected = SelectedPathProof::new(None, ThreadRevision::new(1).unwrap(), digest);
    commit(&store, storage, batch(empty_thread_records(thread, draft)));
    commit(
        &store,
        storage,
        batch([
            FixtureRecord::Binding(BindingRecord::new(
                thread,
                revision,
                selected,
                BindingState::stale(
                    StaleCasBinding::new(
                        support::exact_cas::execution_binding(),
                        cas_thread.clone(),
                        Some(test_tool_profile()),
                        Some(CasRepresentedPrefixProof::new(
                            None,
                            ThreadRevision::new(1).unwrap(),
                            digest,
                        )),
                        None,
                        Some(beryl_model::CasNativeTurnCount::ZERO),
                        None,
                        "provider history diverged",
                        timestamp(2),
                    )
                    .unwrap(),
                ),
            )),
            FixtureRecord::BindingHead(BindingHeadRecord::new(
                thread,
                revision,
                BindingLifecycle::Stale,
                digest,
            )),
            FixtureRecord::CasThread(CasThreadIndexRecord::retired(
                cas_thread.clone(),
                thread,
                revision,
                revision,
            )),
            FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
                cas_thread.clone(),
                thread,
                revision,
            )),
        ]),
    );
    store.validate_registered_domains().unwrap();

    let current = storage
        .current_binding(&store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(current.head().lifecycle(), BindingLifecycle::Stale);
    assert_eq!(current.binding().state().cas_thread_id(), Some(&cas_thread));
    let BindingState::Stale(stale) = current.binding().state() else {
        panic!("current binding is not stale")
    };
    assert_eq!(stale.observed_tool_profile(), Some(test_tool_profile()));
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    let current = storage
        .current_binding(
            &reopened,
            thread,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let BindingState::Stale(stale) = current.binding().state() else {
        panic!("reopened binding is not stale")
    };
    assert_eq!(stale.observed_tool_profile(), Some(test_tool_profile()));
    reopened.close().unwrap();
}
