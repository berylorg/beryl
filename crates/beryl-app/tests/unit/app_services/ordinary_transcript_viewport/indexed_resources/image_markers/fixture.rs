use super::*;
use beryl_home_store::{CommandOutcome, HomeCommand, MutationContribution};
use beryl_model::{ImageLabelOrdinal, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId};
use beryl_state::{AssetOwner, AssetOwnerHeadUpdate, UpdateAssetOwnerHeads};
use syndic_storage::{ComposerAtom, ComposerImageMarker, ComposerPayload, PreparedContent, SourceEventPayload, SyndicTimestamp, TurnEndStatus};

mod host_support {
    use super::*;
    pub fn execute(home: &HomeStore, contribution: MutationContribution) -> CommandOutcome {
        let mut command = HomeCommand::new(home.home_revision().unwrap());
        command.add(contribution).unwrap();
        home.execute(command)
    }
    pub fn committed(outcome: CommandOutcome) {
        assert!(matches!(outcome, CommandOutcome::Committed { later_failure: None, .. }));
    }
}
mod assets {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/historical_replacement_host/assets.rs"));
}

pub(super) fn seed(path: &std::path::Path) {
    use crate::support::exact_cas;
    let StartupHomeOpen::Ready { candidate, state, syndic } = support::open(path) else { panic!("marker transcript fixture Home"); };
    let home = candidate.publish().unwrap();
    let thread = state.session().minimal_bootstrap(&home).unwrap().unwrap().windows()[0].selected_thread().unwrap().thread_id();
    let item = SyndicItemId::from_bytes([0xB3; 16]);
    let markers = [(SyndicDraftMarkerId::from_bytes([0xB1; 16]), ImageLabelOrdinal::FIRST),
        (SyndicDraftMarkerId::from_bytes([0xB2; 16]), ImageLabelOrdinal::new(2).unwrap())];
    let asset_state = state.assets();
    let asset = assets::publish_asset(&home, &asset_state, b"saved image original fixture");
    let proof = assets::seal_marker_set(&home, &asset_state, &markers, asset);
    host_support::committed(host_support::execute(&home, asset_state.update_owner_heads(asset_state.revision(&home).unwrap(),
        UpdateAssetOwnerHeads::new(Box::new([AssetOwnerHeadUpdate::replace(AssetOwner::SubmittedTurnItem(item), None, Some(proof))])).unwrap())));
    let payload = ComposerPayload::new(vec![ComposerAtom::Text("before ".into()),
        ComposerAtom::ImageMarker(ComposerImageMarker::new(markers[0].0, markers[0].1)),
        ComposerAtom::ImageMarker(ComposerImageMarker::new(markers[1].0, markers[1].1)),
        ComposerAtom::Text(" after\n\n```rust\nlet sample = 1;\n```\n\n| A | B |\n| --- | --- |\n| one | two |\n".into()),
    ]).unwrap();
    let at = |offset: u64| SyndicTimestamp::from_unix_millis(1000_u64 + offset);
    let turn = exact_cas::submit_prepared_current_draft(&home, syndic.clone(), thread,
        SyndicDraftId::from_bytes([0xB4; 16]), item, &PreparedContent::composer(&payload).unwrap(), Some(proof), at(0));
    let source = exact_cas::establish_turn(&home, syndic.clone(), thread, turn, at(1));
    exact_cas::admit_event(&home, syndic.clone(), thread, turn, &source, SourceEventPayload::TurnActivated, at(2));
    exact_cas::correlate_user_item(&home, syndic.clone(), thread, turn, item, &source, at(3));
    exact_cas::admit_event(&home, syndic.clone(), thread, turn, &source, SourceEventPayload::TurnEnded(TurnEndStatus::complete()), at(4));
    exact_cas::converge_and_release_terminal_history(&home, syndic.clone(), thread, turn);
    drop((state, syndic, asset_state));
    home.close().unwrap();
}
