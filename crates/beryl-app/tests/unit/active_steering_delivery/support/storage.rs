use std::{thread, time::Instant};

use beryl_home_store::{HomeCommand, HomeStore};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
    SyndicAcceptedInputId, SyndicThreadId,
};
use syndic_storage::{
    AcceptedInputLifecycle, AcceptedRouteEffectiveState, ComposerAtom, ComposerPayload,
    ContentAppend, ContentBuild, DraftPayloadUpdate, DraftPayloadUpdateDecision, PreparedContent,
    SelectedPathProof, SyndicPointReadLimit, SyndicReadError, SyndicStorage, SyndicTimestamp,
};

use super::{EXECUTION_ROOT, POINT_READ_BYTES, TIMEOUT};

pub(super) fn route_state_for(
    home: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    input_id: SyndicAcceptedInputId,
) -> AcceptedRouteEffectiveState {
    route_entry(home, storage, thread_id, input_id).0
}

pub(super) fn route_entry(
    home: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    input_id: SyndicAcceptedInputId,
) -> (AcceptedRouteEffectiveState, AcceptedInputLifecycle) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let gate = match storage.input_gate(home, thread_id, point_limit()) {
            Ok(Some(gate)) => gate,
            Ok(None) => panic!("accepted-input history retains its input gate"),
            Err(SyndicReadError::ConcurrentChange { .. }) => {
                assert!(
                    Instant::now() < deadline,
                    "concurrent input-gate reads did not converge",
                );
                thread::yield_now();
                continue;
            }
            Err(error) => panic!("input-gate read failed: {error}"),
        };
        let route = gate
            .selected_route()
            .expect("accepted-input history retains its route generation");
        let page = match storage.accepted_route_page(
            home,
            thread_id,
            route.generation(),
            route.revision(),
            None,
        ) {
            Ok(page) => page,
            Err(
                SyndicReadError::StaleAcceptedRoute
                | SyndicReadError::ConcurrentChange { .. },
            ) => {
                assert!(
                    Instant::now() < deadline,
                    "concurrent accepted-route reads did not converge",
                );
                thread::yield_now();
                continue;
            }
            Err(error) => panic!("accepted-route read failed: {error}"),
        };
        let entry = page
            .records()
            .iter()
            .find(|entry| entry.input().id() == input_id)
            .expect("permanent accepted order retains the tested input");
        return (entry.effective_state(), entry.leaf().lifecycle());
    }
}

pub(super) fn replace_current_text(
    home: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    text: &str,
    updated_at: SyndicTimestamp,
) {
    let prepared = PreparedContent::composer(
        &ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap(),
    )
    .unwrap();
    stage_prepared_content(home, storage, &prepared);
    let current = storage
        .current_draft(home, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let DraftPayloadUpdateDecision::Update(update) =
        DraftPayloadUpdate::prepare(&current, &prepared, updated_at).unwrap()
    else {
        panic!("fixture text must replace the current draft payload")
    };
    execute(
        home,
        storage.update_draft_payload(storage.revision(home).unwrap(), update),
    );
}

pub(super) fn stage_prepared_content(
    home: &HomeStore,
    storage: SyndicStorage,
    content: &PreparedContent,
) {
    execute(
        home,
        storage.begin_content(
            storage.revision(home).unwrap(),
            ContentBuild::from_prepared(content),
        ),
    );
    let mut manifest = content.building_manifest();
    while let Some(append) = ContentAppend::prepare(&manifest, content).unwrap() {
        manifest = append.next_manifest().clone();
        execute(
            home,
            storage.append_content(storage.revision(home).unwrap(), append),
        );
    }
}

pub(super) fn selected_path(
    home: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
) -> SelectedPathProof {
    let thread = storage
        .thread(home, thread_id, point_limit())
        .unwrap()
        .unwrap();
    SelectedPathProof::new(
        thread.committed_tail(),
        thread.revision(),
        thread.selected_path_digest(),
    )
}

pub(super) fn execution_binding(runtime_id: RuntimeId, seed: u8) -> ExecutionBinding {
    ExecutionBinding::new(
        runtime_id,
        RootId::from_bytes([seed.wrapping_add(7); 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            EXECUTION_ROOT,
        )
        .unwrap(),
    )
}

pub(super) fn execute(
    home: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    home.execute(command).unwrap();
}

pub(super) fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(POINT_READ_BYTES).unwrap()
}

pub(super) fn timestamp(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}
