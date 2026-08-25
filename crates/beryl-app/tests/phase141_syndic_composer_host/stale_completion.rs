use super::*;

#[test]
fn stale_same_id_completion_and_cancellation_are_inert_across_rebind_and_release() {
    let (_home, store, storage, thread) = fixture("same-id-aba", 75);
    populate(storage, &store, thread, 76);
    assert_stale_request_after_service_replacement(storage, &store, thread, 77, 78, 79, 80, 1);
    assert_stale_request_after_service_replacement(storage, &store, thread, 81, 82, 83, 84, 2);
}

#[allow(clippy::too_many_arguments)]
fn assert_stale_request_after_service_replacement(
    storage: syndic_storage::SyndicStorage,
    store: &beryl_home_store::HomeStore,
    thread: beryl_model::SyndicThreadId,
    old_session: u8,
    old_operation: u8,
    new_session: u8,
    new_operation: u8,
    request_id: u64,
) {
    let mut host = SyndicComposerHost::new(storage);
    let ComposerHostActivationOutcome::Activated {
        binding: old_binding,
        ..
    } = host
        .activate(
            store,
            activation(thread, old_session, old_operation, Vec::new()),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("fixture activation failed");
    };
    let old_key = key(old_binding, request_id);
    let old_pending = host.begin_request(old_key, text_request()).unwrap();
    let old_execution = host.execute_pending(store, old_pending);

    host.dispose_composer_service(store).unwrap();
    let mut host = SyndicComposerHost::new(storage);
    let ComposerHostActivationOutcome::Activated {
        binding: current_binding,
        ..
    } = host
        .activate(
            store,
            activation(thread, new_session, new_operation, Vec::new()),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("fixture session did not rebind after service replacement");
    };
    let current_key = key(current_binding, request_id);
    let current_pending = host.begin_request(current_key, text_request()).unwrap();
    assert!(matches!(
        host.complete_request(old_execution),
        Err(ComposerHostError::RequestMismatch)
    ));
    assert_eq!(host.pending_request_count(), 1);
    assert!(!host.cancel_request(old_key));
    assert_eq!(host.pending_request_count(), 1);
    let current_execution = host.execute_pending(store, current_pending);
    assert!(matches!(
        host.complete_request(current_execution),
        Ok(response) if response.key() == current_key
    ));
    assert_eq!(host.pending_request_count(), 0);
}
