use super::*;

#[test]
fn stale_same_id_completion_and_cancellation_are_inert_across_rebind_and_release() {
    let (_home, store, storage, thread) = fixture("same-id-aba", 75);
    populate(storage, &store, thread, 76);
    let mut host = SyndicComposerHost::new(storage);
    let original_activation = activation(thread, 77, 78, Vec::new());
    let ComposerHostActivationOutcome::Activated {
        binding: original_binding,
        ..
    } = host
        .activate(
            &store,
            original_activation.clone(),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("fixture activation failed");
    };
    let old_key = key(original_binding, 1);
    let old_pending = host.begin_request(old_key, text_request()).unwrap();
    let old_execution = host.execute_pending(&store, old_pending);

    let ComposerHostActivationOutcome::Activated {
        disposition: ComposerHostOpenDisposition::ExactReplay,
        binding: rebound_binding,
    } = host
        .activate(&store, original_activation, &CommandCancellation::new())
        .unwrap()
    else {
        panic!("fixture session did not rebind by exact replay");
    };
    let rebound_key = key(rebound_binding, 1);
    let rebound_pending = host.begin_request(rebound_key, text_request()).unwrap();
    assert!(matches!(
        host.complete_request(old_execution),
        Err(ComposerHostError::RequestMismatch)
    ));
    assert_eq!(host.pending_request_count(), 1);
    assert!(!host.cancel_request(old_key));
    assert_eq!(host.pending_request_count(), 1);
    let rebound_execution = host.execute_pending(&store, rebound_pending);
    assert!(matches!(
        host.complete_request(rebound_execution),
        Ok(response) if response.key() == rebound_key
    ));

    let release_old_key = key(rebound_binding, 2);
    let release_old_pending = host.begin_request(release_old_key, text_request()).unwrap();
    let release_old_execution = host.execute_pending(&store, release_old_pending);
    assert!(host.release().unwrap());
    let ComposerHostActivationOutcome::Activated {
        binding: released_binding,
        ..
    } = host
        .activate(
            &store,
            activation(thread, 79, 80, Vec::new()),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("fixture did not reactivate after release");
    };
    let released_key = key(released_binding, 2);
    let released_pending = host.begin_request(released_key, text_request()).unwrap();
    assert!(matches!(
        host.complete_request(release_old_execution),
        Err(ComposerHostError::RequestMismatch)
    ));
    assert_eq!(host.pending_request_count(), 1);
    assert!(!host.cancel_request(release_old_key));
    assert_eq!(host.pending_request_count(), 1);
    let released_execution = host.execute_pending(&store, released_pending);
    assert!(matches!(
        host.complete_request(released_execution),
        Ok(response) if response.key() == released_key
    ));
    assert_eq!(host.pending_request_count(), 0);
}
