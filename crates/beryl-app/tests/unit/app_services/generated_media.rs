use super::*;
use crate::app_services::preparation::PreparedAppServices;
use crate::app_services::recovery_retirement::{RetiredProcessWorkError, ServiceGraphRetirementError};
use beryl_home_store::HomeGeneration;

fn without_window(run: impl FnOnce(gpui::ImageResourceDomain) + 'static) {
    let complete = std::rc::Rc::new(std::cell::Cell::new(false));
    let done = complete.clone();
    gpui::Application::new().with_quit_on_last_window_close(false).run(move |app| {
        let domain = app.image_resource_domain(crate::image_processing::generated_media_qualification::domain_configuration()).unwrap();
        run(domain);
        done.set(true);
        app.quit();
    });
    assert!(complete.get());
}

fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "generated service lifecycle did not settle");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn retire(owner: &mut ProcessServiceOwner, generation: HomeGeneration) {
    match owner.retire_failed_service_graph(generation) {
        Ok(()) | Err(ServiceGraphRetirementError::Incomplete) => {},
        Err(error) => panic!("generated graph retirement failed: {error:?}"),
    }
    wait(|| match owner.finish_service_graph_retirement(generation) {
        Ok(()) => true,
        Err(ServiceGraphRetirementError::Incomplete) => false,
        Err(error) => panic!("generated graph retirement failed: {error:?}"),
    });
}

#[test]
#[ignore = "requires an announced native GPUI qualification run; opens no window"]
fn generated_cohort_prepares_before_initial_publication_and_cancels_held_workers() {
    without_window(|domain| {
        let (directory, candidate, state, syndic, _) = fixture();
        let owner = owner(&candidate);
        let mut config = configuration();
        config.media_domain = Some(domain);
        let prepared = PreparedAppServices::prepare(&owner, candidate, state, syndic, config,
            SyndicTimestamp::from_unix_millis(1), &CommandCancellation::new()).unwrap();
        let generated = prepared.generated_media.as_ref().expect("cohort belongs to unpublished complete graph");
        for _ in 0..16 { generated.waker().wake_by_ref(); }
        assert_eq!(generated.diagnostics(), (0, 0, false, 2));
        prepared.dispose().unwrap();
        wait(|| owner.begin_media_process_shutdown() == gpui::ImageResourceDrain::Drained);
        let process = owner.media_process().unwrap();
        let diagnostics = process.diagnostics().unwrap();
        assert_eq!((diagnostics.path_slots, diagnostics.verification_slots, diagnostics.active_workers), (0, 0, 0));
        assert_reopens(&directory);
    });
}

#[test]
#[ignore = "requires an announced native GPUI qualification run; opens no window"]
fn generated_cohort_recovery_mounts_fresh_before_publication_and_joins_before_home_close() {
    without_window(|domain| {
        let (directory, candidate, state, syndic, faults) = fixture();
        let mut owner = owner(&candidate);
        let mut config = configuration();
        config.media_domain = Some(domain);
        owner.open_initial(candidate, state, syndic, config.clone(), SyndicTimestamp::from_unix_millis(1), CommandCancellation::new()).unwrap();
        wait(|| owner.graph().unwrap().generated_media.as_ref().unwrap().diagnostics().2);
        let original = owner.graph().unwrap().generated_media.as_ref().unwrap().waker();
        let generation = owner.graph().unwrap().home().health().generation().unwrap();
        owner.graph.as_mut().unwrap().handoff.as_mut().unwrap().shutdown().unwrap();
        super::recovery_support::fail(&owner, &faults);
        retire(&mut owner, generation);
        let mut candidate = Some(owner.recover_retired_service_home(generation).unwrap());
        let replacement_generation = candidate.as_ref().unwrap().generation();
        let fresh_state = BerylState::reacquire_candidate(candidate.as_ref().unwrap()).unwrap();
        let fresh_syndic = SyndicStorage::reacquire_candidate(candidate.as_ref().unwrap()).unwrap();
        wait(|| match owner.settle_retired_process_work(
            &candidate.as_mut().unwrap().recovery_access().unwrap(), &fresh_state, &fresh_syndic,
            &CommandCancellation::new(),
        ) {
            Ok(()) => true,
            Err(RetiredProcessWorkError::CatalogQueryPending) => false,
            Err(error) => panic!("generated graph retained process work failed: {error:?}"),
        });
        owner.require_catalog_recovery_settlement(generation).unwrap();
        let prepared = owner.prepare_recovery_service_graph(generation, &mut candidate, config,
            SyndicTimestamp::from_unix_millis(2), &CommandCancellation::new()).unwrap();
        let generated = prepared.services.as_ref().unwrap().generated_media.as_ref().unwrap();
        assert!(!original.will_wake(&generated.waker()));
        assert_eq!(generated.diagnostics(), (0, 0, false, 2));
        let mut prepared = Some(prepared);
        let start = owner.publish_recovery_service_graph(generation, replacement_generation, &mut prepared, &CommandCancellation::new()).unwrap();
        assert!(prepared.is_none());
        assert_eq!(owner.graph().unwrap().generated_media.as_ref().unwrap().diagnostics(), (0, 0, false, 2));
        assert!(start.release());
        wait(|| owner.graph().unwrap().generated_media.as_ref().unwrap().diagnostics().2);
        owner.graph.as_mut().unwrap().handoff.as_mut().unwrap().shutdown().unwrap();
        super::recovery_support::fail(&owner, &faults);
        retire(&mut owner, replacement_generation);
        owner.take_retired_service_home(replacement_generation).unwrap().close().unwrap();
        wait(|| owner.begin_media_process_shutdown() == gpui::ImageResourceDrain::Drained);
        assert_reopens(&directory);
    });
}
