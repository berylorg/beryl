use super::*;
use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore, MutationContribution};
use beryl_model::{ImageLabelOrdinal, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId};
use beryl_state::{AssetOwner, AssetOwnerHeadUpdate, UpdateAssetOwnerHeads};
use std::num::NonZeroU64;
use syndic_storage::*;

mod host_support {
    use super::*;
    pub fn execute(home: &HomeStore, contribution: MutationContribution) -> CommandOutcome {
        let mut command = HomeCommand::new(home.home_revision().unwrap());
        command.add(contribution).unwrap();
        home.execute(command)
    }
    pub fn committed(outcome: CommandOutcome) {
        assert!(matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
    }
}

mod assets {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/historical_replacement_host/assets.rs"
    ));
}

pub(super) fn home() -> (
    PathBuf,
    beryl_model::SyndicThreadId,
    beryl_model::SyndicThreadId,
    crate::syndic_transcript::ProviderRevision,
) {
    let (path, first, target, _) = fixture::history_home();
    let HomeOpenOutcome::Ready {
        candidate,
        state,
        syndic,
    } = fixture::open(&path)
    else {
        unreachable!()
    };
    let home = candidate.publish().unwrap();
    let item = SyndicItemId::from_bytes([0x91; 16]);
    let markers = (1..=48u64)
        .map(|ordinal| {
            let mut bytes = [0x92; 16];
            bytes[0..8].copy_from_slice(&ordinal.to_le_bytes());
            (
                SyndicDraftMarkerId::from_bytes(bytes),
                ImageLabelOrdinal::new(ordinal).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let asset_state = state.assets();
    let asset = assets::publish_asset(&home, &asset_state, b"authored image marker fixture");
    let proof = assets::seal_marker_set(&home, &asset_state, &markers, asset);
    host_support::committed(host_support::execute(
        &home,
        asset_state.update_owner_heads(
            asset_state.revision(&home).unwrap(),
            UpdateAssetOwnerHeads::new(Box::new([AssetOwnerHeadUpdate::replace(
                AssetOwner::SubmittedTurnItem(item),
                None,
                Some(proof),
            )]))
            .unwrap(),
        ),
    ));
    let mut atoms = vec![ComposerAtom::Text("Image-heavy authored turn\n\n".into())];
    for &(marker, label) in &markers {
        atoms.push(ComposerAtom::ImageMarker(ComposerImageMarker::new(
            marker, label,
        )));
        atoms.push(ComposerAtom::Text(" ".into()));
    }
    atoms.push(ComposerAtom::Text(
        ("\n\n```text\n".to_owned() + &"y".repeat(18_000) + "\n```\n").into(),
    ));
    let payload = ComposerPayload::new(atoms).unwrap();
    let time = SyndicTimestamp::from_unix_millis;
    let turn = crate::support::exact_cas::submit_prepared_current_draft(
        &home,
        syndic.clone(),
        target,
        SyndicDraftId::from_bytes([0x93; 16]),
        item,
        &PreparedContent::composer(&payload).unwrap(),
        Some(proof),
        time(200),
    );
    let source =
        crate::support::exact_cas::establish_turn(&home, syndic.clone(), target, turn, time(201));
    crate::support::exact_cas::admit_event(
        &home,
        syndic.clone(),
        target,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        time(202),
    );
    crate::support::exact_cas::correlate_user_item(
        &home,
        syndic.clone(),
        target,
        turn,
        item,
        &source,
        time(203),
    );
    crate::support::exact_cas::admit_event(
        &home,
        syndic.clone(),
        target,
        turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        time(204),
    );
    crate::support::exact_cas::converge_and_release_terminal_history(
        &home,
        syndic.clone(),
        target,
        turn,
    );
    let revision = crate::syndic_transcript::ProviderRevision(
        syndic
            .transcript_view_head(&home, target, SyndicPointReadLimit::new(1_000_000).unwrap())
            .unwrap()
            .unwrap()
            .revision()
            .get(),
    );
    drop((state, syndic, asset_state));
    home.close().unwrap();
    (path, first, target, revision)
}
