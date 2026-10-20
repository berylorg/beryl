use super::*;

#[test]
fn first_discussion_draft_uses_initial_submission_preparation_with_inherited_history() {
    use beryl_app::cas_projection::SubmissionUnavailable;
    let mut fixture = Fixture::new(180);
    let worker = fixture.store.exact_stop_worker();
    let initial = worker.selected_submission_availability(fixture.thread);
    assert!(matches!(
        initial,
        Ok(()) | Err(SubmissionUnavailable::RuntimeUnavailable)
    ));
    let parent = fixture.thread;
    let child = create_unsubmitted_discussion(&mut fixture, "Exact inherited assistant context");
    let home = fixture.home();
    let record = fixture
        .storage
        .thread(&home, child, point())
        .unwrap()
        .unwrap();
    assert!(record.committed_tail().is_some());
    assert_eq!(
        record.context_owner_id(),
        Some(beryl_model::DiscussionContextOwnerId::Draft(
            record.current_draft_id()
        ))
    );
    assert_eq!(
        worker.selected_submission_availability(child),
        initial,
        "an exact initial discussion draft must retain the initial runtime-preparation route despite inherited history"
    );
    assert_eq!(
        worker.selected_submission_availability(parent),
        Err(SubmissionUnavailable::BindingUnavailable),
        "inherited context does not grant submission authority to an unloaded ordinary execution"
    );
    drop(home);
    fixture.submit_text_on(child, "first ordinary discussion input");
    assert_eq!(
        worker.selected_submission_availability(child),
        Err(SubmissionUnavailable::BindingUnavailable),
        "after first admission the discussion requires ordinary execution authority"
    );
    drop(worker);
    let _ = fixture.store.close().unwrap();
}
