use super::*;

pub(super) fn assistant() -> SyndicItemId {
    SyndicItemId::from_bytes([244; 16])
}
pub(in super::super) fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

pub(super) fn seed(path: &std::path::Path, text: &str, terminal: bool) {
    seed_with_status(path, text, terminal.then_some(TurnEndStatus::complete()));
}

pub(super) fn seed_incomplete(path: &std::path::Path, text: &str) {
    seed_with_status(
        path,
        text,
        Some(TurnEndStatus::incomplete(
            TurnIncompleteReason::ItemAuditFailed,
        )),
    );
}

fn seed_with_status(path: &std::path::Path, text: &str, end_status: Option<TurnEndStatus>) {
    let StartupHomeOpen::Ready {
        candidate,
        state,
        syndic,
    } = support::open(path)
    else {
        panic!("branch fixture home");
    };
    let home = candidate.publish().unwrap();
    let thread = state
        .session()
        .minimal_bootstrap(&home)
        .unwrap()
        .unwrap()
        .windows()[0]
        .selected_thread()
        .unwrap()
        .thread_id();
    append(&home, &syndic, thread, text, end_status);
    drop((state, syndic));
    home.close().unwrap();
}

pub(super) fn append_live(
    owner: &std::rc::Rc<std::cell::RefCell<crate::running_owner::RunningProcessOwner>>,
    text: &str,
) {
    let (home, storage, thread) = {
        let owner = owner.borrow();
        let graph = owner.test_services().graph().unwrap();
        (
            graph.home().service_reference(),
            graph.syndic().clone(),
            graph
                .state()
                .session()
                .minimal_bootstrap(graph.home())
                .unwrap()
                .unwrap()
                .windows()[0]
                .selected_thread()
                .unwrap()
                .thread_id(),
        )
    };
    append(&home, &storage, thread, text, None);
    let turn = storage
        .thread(&home, thread, limit())
        .unwrap()
        .unwrap()
        .committed_tail()
        .unwrap();
    assert_eq!(
        storage
            .turn_state(&home, turn, limit())
            .unwrap()
            .unwrap()
            .lifecycle(),
        TurnLifecycle::Active
    );
}

pub(in super::super) fn append(
    home: &beryl_home_store::HomeStore,
    syndic: &SyndicStorage,
    thread: SyndicThreadId,
    text: &str,
    end_status: Option<TurnEndStatus>,
) {
    let time = SyndicTimestamp::from_unix_millis;
    let user = SyndicItemId::from_bytes([243; 16]);
    let turn = exact_cas::submit_current_draft(
        &home,
        syndic.clone(),
        thread,
        SyndicDraftId::from_bytes([245; 16]),
        user,
        "branch question",
        time(3001),
    );
    let source = exact_cas::establish_turn(&home, syndic.clone(), thread, turn, time(3002));
    exact_cas::admit_event(
        &home,
        syndic.clone(),
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        time(3003),
    );
    exact_cas::correlate_user_item(
        &home,
        syndic.clone(),
        thread,
        turn,
        user,
        &source,
        time(3004),
    );
    exact_cas::admit_item_frame(
        &home,
        syndic.clone(),
        thread,
        turn,
        assistant(),
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::FIRST,
            CasItemId::new("captured-branch-assistant").unwrap(),
            ProviderItemObservationV1::Started {
                observed_at: ProviderLifecycleTimestampMsV1::new(3005),
                item: ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
                    text: ProviderTextV1::inline(text),
                    phase: None,
                    memory_citation: None,
                }),
            },
        ),
        time(3005),
    );
    exact_cas::admit_item_frame(
        &home,
        syndic.clone(),
        thread,
        turn,
        assistant(),
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(2).unwrap(),
            CasItemId::new("captured-branch-assistant").unwrap(),
            ProviderItemObservationV1::Completed {
                observed_at: ProviderLifecycleTimestampMsV1::new(3006),
                item: ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
                    text: ProviderTextV1::inline(text),
                    phase: None,
                    memory_citation: None,
                }),
            },
        ),
        time(3006),
    );
    if let Some(status) = end_status {
        exact_cas::admit_event(
            &home,
            syndic.clone(),
            thread,
            turn,
            &source,
            SourceEventPayload::TurnEnded(status),
            time(3007),
        );
        exact_cas::converge_and_release_terminal_history(&home, syndic.clone(), thread, turn);
    } else {
        exact_cas::project_item_if_needed(&home, syndic.clone(), user);
        exact_cas::project_item_if_needed(&home, syndic.clone(), assistant());
        exact_cas::converge_transcript(&home, syndic.clone(), thread);
    }
}
