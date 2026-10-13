use super::*;
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
};
use beryl_model::{SyndicDraftId, SyndicItemId};
use beryl_state::BerylState;
use syndic_storage::*;

pub(super) fn open(path: &Path) -> HomeOpenOutcome {
    let mut candidate =
        HomeOpenCandidate::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let syndic = SyndicStorage::register(&mut candidate).unwrap();
    let candidate = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap();
    HomeOpenOutcome::Ready {
        candidate,
        state,
        syndic,
    }
}

pub(super) fn history_home() -> (
    PathBuf,
    beryl_model::SyndicThreadId,
    beryl_model::SyndicThreadId,
    crate::syndic_transcript::ProviderRevision,
) {
    let (directory, _, first) = super::super::creation_context::selected_home();
    let path = directory.keep();
    let ledger = std::env::var_os("BERYL_NATIVE_RETAINED_FIXTURE_LEDGER")
        .expect("native fixture ledger is required");
    let mut output = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ledger)
        .unwrap();
    std::io::Write::write_all(&mut output, format!("{}\n", path.display()).as_bytes()).unwrap();
    let HomeOpenOutcome::Ready {
        candidate,
        state,
        syndic,
    } = open(&path)
    else {
        unreachable!()
    };
    let home = candidate.publish().unwrap();
    let limit = SyndicPointReadLimit::new(1_000_000).unwrap();
    let execution = syndic
        .thread_execution(&home, first, limit)
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    let target = beryl_model::SyndicThreadId::from_bytes([0x72; 16]);
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(syndic.create_thread(
            syndic.revision(&home).unwrap(),
            CreateThread::ordinary(
                target,
                SyndicDraftId::from_bytes([0x73; 16]),
                execution,
                SyndicTimestamp::from_unix_millis(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let text = "history paragraph\n\n".repeat(4096) + "```text\n" + &"x".repeat(18_000) + "\n```\n";
    let item = SyndicItemId::from_bytes([0x74; 16]);
    let time = SyndicTimestamp::from_unix_millis;
    let turn = crate::support::exact_cas::submit_current_draft(
        &home,
        syndic.clone(),
        target,
        SyndicDraftId::from_bytes([0x75; 16]),
        item,
        &text,
        time(100),
    );
    let source =
        crate::support::exact_cas::establish_turn(&home, syndic.clone(), target, turn, time(101));
    crate::support::exact_cas::admit_event(
        &home,
        syndic.clone(),
        target,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        time(102),
    );
    crate::support::exact_cas::correlate_user_item(
        &home,
        syndic.clone(),
        target,
        turn,
        item,
        &source,
        time(103),
    );
    crate::support::exact_cas::admit_event(
        &home,
        syndic.clone(),
        target,
        turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        time(104),
    );
    crate::support::exact_cas::converge_and_release_terminal_history(
        &home,
        syndic.clone(),
        target,
        turn,
    );
    let revision = crate::syndic_transcript::ProviderRevision(
        syndic
            .transcript_view_head(&home, target, limit)
            .unwrap()
            .unwrap()
            .revision()
            .get(),
    );
    drop((state, syndic));
    home.close().unwrap();
    (path, first, target, revision)
}
