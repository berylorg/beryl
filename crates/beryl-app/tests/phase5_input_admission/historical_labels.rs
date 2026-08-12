use super::{
    AcceptedInputAdmission, AssetOwner, Fixture, IdleSubmission, InputAdmissionBuildError,
    InputAdmissionStatus, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId, point_limit,
    prepare_accepted_input_admission, time,
};
use crate::assets::{admit_asset_at_label, admit_reference_set};
use beryl_model::{AssetId, SealedAssetReferenceSetProof};
use syndic_storage::ImageLabelOrdinal;

struct Origin {
    asset_id: AssetId,
    proof: SealedAssetReferenceSetProof,
}

fn establish_origin(
    fixture: &mut Fixture,
    label: ImageLabelOrdinal,
    asset_bytes: &[u8],
    seed: u8,
) -> Origin {
    let marker = SyndicDraftMarkerId::from_bytes([seed; 16]);
    fixture.publish_marker_at(marker, label, 2);
    let draft = fixture.draft;
    let (asset_id, proof) = admit_asset_at_label(
        fixture,
        marker,
        label,
        asset_bytes,
        draft,
        seed.wrapping_add(1),
    );
    let current = fixture.current_draft();
    let gate = fixture
        .syndic
        .input_gate(fixture.home().home(), fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let submission = IdleSubmission::new(
        fixture.thread,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        SyndicDraftId::from_bytes([seed.wrapping_add(2); 16]),
        SyndicItemId::from_bytes([seed.wrapping_add(3); 16]),
        Some(proof),
        time(3),
    );
    fixture
        .store
        .execute_idle_submission(fixture.state.assets(), submission)
        .unwrap();
    Origin { asset_id, proof }
}

fn prepare_reuse(
    fixture: &mut Fixture,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
    seed: u8,
) -> AcceptedInputAdmission {
    let marker = SyndicDraftMarkerId::from_bytes([seed; 16]);
    fixture.publish_marker_at(marker, label, 4);
    let current = fixture.current_draft();
    let proof = admit_reference_set(
        fixture,
        marker,
        label,
        asset_id,
        current.draft().id(),
        seed.wrapping_add(1),
    );
    let current = fixture.current_draft();
    let gate = fixture
        .syndic
        .input_gate(fixture.home().home(), fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    AcceptedInputAdmission::new(
        fixture.thread,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        SyndicDraftId::from_bytes([seed.wrapping_add(2); 16]),
        Some(proof),
        time(5),
    )
}

#[test]
fn current_label_reuse_with_the_exact_origin_asset_is_accepted() {
    let mut fixture = Fixture::new(80);
    let label = ImageLabelOrdinal::new(3).unwrap();
    let origin = establish_origin(&mut fixture, label, b"origin image", 82);
    let resolved = fixture
        .syndic
        .resolve_image_label_origin_span(
            fixture.home().home(),
            fixture.thread,
            label,
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(resolved.span().start_label(), ImageLabelOrdinal::FIRST);
    assert_eq!(resolved.span().end_label(), label);
    assert_eq!(resolved.span().asset_reference_set(), origin.proof);

    let admission = prepare_reuse(&mut fixture, label, origin.asset_id, 86);
    let input = admission.accepted_input_id();
    let prepared = prepare_accepted_input_admission(
        fixture.home().home(),
        fixture.syndic,
        fixture.state.assets(),
        admission.clone(),
    )
    .unwrap();
    fixture
        .store
        .execute_accepted_input_admission(prepared)
        .unwrap();

    let stored = fixture
        .syndic
        .accepted_input(fixture.home().home(), input, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.asset_reference_set(),
        admission.asset_reference_set()
    );
    assert!(
        fixture
            .state
            .assets()
            .owner_head(fixture.home().home(), AssetOwner::AcceptedInput(input))
            .unwrap()
            .is_some()
    );
    fixture
        .store
        .live_home_command()
        .unwrap()
        .home()
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn current_label_reuse_with_a_different_asset_is_rejected() {
    let mut fixture = Fixture::new(100);
    let label = ImageLabelOrdinal::new(7).unwrap();
    establish_origin(&mut fixture, label, b"first asset", 102);

    let marker = SyndicDraftMarkerId::from_bytes([106; 16]);
    fixture.publish_marker_at(marker, label, 4);
    let draft = fixture.current_draft().draft().id();
    let (_different_asset, proof) =
        admit_asset_at_label(&mut fixture, marker, label, b"different asset", draft, 107);
    let current = fixture.current_draft();
    let gate = fixture
        .syndic
        .input_gate(fixture.home().home(), fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let admission = AcceptedInputAdmission::new(
        fixture.thread,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        SyndicDraftId::from_bytes([108; 16]),
        Some(proof),
        time(5),
    );

    assert!(matches!(
        prepare_accepted_input_admission(
            fixture.home().home(),
            fixture.syndic,
            fixture.state.assets(),
            admission.clone(),
        ),
        Err(InputAdmissionBuildError::HistoricalImageLabelMismatch { label: actual })
            if actual == label
    ));
    assert_eq!(
        fixture
            .syndic
            .accepted_input_status(fixture.home().home(), &admission, point_limit())
            .unwrap(),
        InputAdmissionStatus::Absent
    );
    assert_eq!(
        fixture
            .state
            .assets()
            .owner_head(fixture.home().home(), AssetOwner::CurrentDraft(draft))
            .unwrap()
            .unwrap()
            .set(),
        proof
    );
}

#[test]
fn a_gap_reserved_by_an_earlier_maximum_label_cannot_be_reused() {
    let mut fixture = Fixture::new(120);
    let maximum = ImageLabelOrdinal::new(9).unwrap();
    let origin = establish_origin(&mut fixture, maximum, b"maximum-label asset", 122);
    let reserved_gap = ImageLabelOrdinal::new(4).unwrap();
    let admission = prepare_reuse(&mut fixture, reserved_gap, origin.asset_id, 126);
    let draft = admission.draft_id();
    let proof = admission.asset_reference_set().unwrap();

    assert!(matches!(
        prepare_accepted_input_admission(
            fixture.home().home(),
            fixture.syndic,
            fixture.state.assets(),
            admission.clone(),
        ),
        Err(InputAdmissionBuildError::ReservedImageLabel { label })
            if label == reserved_gap
    ));
    assert_eq!(
        fixture
            .syndic
            .accepted_input_status(fixture.home().home(), &admission, point_limit())
            .unwrap(),
        InputAdmissionStatus::Absent
    );
    assert_eq!(
        fixture
            .state
            .assets()
            .owner_head(fixture.home().home(), AssetOwner::CurrentDraft(draft))
            .unwrap()
            .unwrap()
            .set(),
        proof
    );
}
