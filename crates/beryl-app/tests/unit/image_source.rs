use std::io::{BufRead, Read};

use beryl_home_store::{HomeStore, SidecarByteLimit, SidecarNamespace};

use beryl_state::{AssetDimensions, AssetMediaType, PublishAssetMetadata};
use syndic_storage::{
    DraftEditorCandidateActivationBindingV1, DraftEditorCandidateSessionIdV1,
    DraftEditorCandidateSessionReadOutcomeV1,
};

use super::*;

#[path = "../syndic_composer_host/support.rs"]
mod host_fixture;

fn limits() -> MediaResourceLimits {
    MediaResourceLimits::new(super::super::MediaResourceConfiguration::default()).unwrap()
}

fn source_provider(
    home: &HomeStore,
    storage: SyndicStorage,
    assets: AssetState,
) -> ImageSourceProvider {
    ImageSourceProvider::new(home.service_reference(), storage, assets, limits()).unwrap()
}

fn binding(
    home: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    session_seed: u8,
) -> ComposerHostBinding {
    let draft = host_fixture::current(storage.clone(), home, thread);
    let DraftEditorCandidateSessionReadOutcomeV1::Active(session) = storage
        .draft_editor_candidate_session(
            home,
            draft.draft().id(),
            DraftEditorCandidateSessionIdV1::from_bytes([session_seed; 16]),
        )
        .unwrap()
    else {
        panic!("fixture candidate session must remain active");
    };
    ComposerHostBinding::new(
        home.home_id(),
        home.health().generation().unwrap(),
        crate::composer_host::ComposerHostGeneration::FIRST,
        DraftEditorCandidateActivationBindingV1::from_head(&session),
        NonZeroU64::MIN,
    )
}

#[test]
fn exact_composer_identity_authenticates_both_same_anchor_markers_and_rejects_forgery() {
    let (_directory, home, state, storage, thread) =
        host_fixture::fixture_with_state("image-source", 71);
    let empty_root = host_fixture::current(storage.clone(), &home, thread)
        .root()
        .reference();
    let (left, right) = host_fixture::populate(storage.clone(), &home, thread, 91);
    let current = binding(&home, &storage, thread, 91);
    let provider = source_provider(&home, storage, state.assets());
    let cancellation = CommandCancellation::new();
    for marker in [left, right] {
        assert_eq!(
            provider
                .resolve_composer(
                    current,
                    marker.marker_id(),
                    marker.order_key(),
                    &cancellation
                )
                .unwrap(),
            marker.asset_id()
        );
        assert!(matches!(
            provider.resolve(
                &ImageSourceDemand::Composer {
                    binding: current,
                    marker_id: marker.marker_id(),
                    order_key: marker.order_key()
                },
                &cancellation
            ),
            Err(ImageSourceError::Unavailable)
        ));
    }
    assert!(matches!(
        provider.resolve_composer(
            current,
            left.marker_id(),
            left.order_key() + 100,
            &cancellation
        ),
        Err(ImageSourceError::Identity)
    ));
    assert!(matches!(
        provider.resolve_composer(
            current,
            SyndicDraftMarkerId::from_bytes([199; 16]),
            left.order_key(),
            &cancellation
        ),
        Err(ImageSourceError::Stale)
    ));
    let candidate = current.candidate();
    let forged_binding = |generation, root| {
        ComposerHostBinding::new(
            current.home_id(),
            current.home_generation(),
            current.host_generation(),
            DraftEditorCandidateActivationBindingV1::new(
                candidate.draft_id(),
                candidate.session_id(),
                candidate.session_generation(),
                generation,
                root,
                candidate.history(),
                candidate.logical_extent(),
            ),
            current.presentation_generation(),
        )
    };
    for stale in [
        forged_binding(candidate.candidate_generation() + 1, candidate.root()),
        forged_binding(candidate.candidate_generation(), empty_root),
    ] {
        assert!(matches!(
            provider.resolve_composer(stale, left.marker_id(), left.order_key(), &cancellation),
            Err(ImageSourceError::Stale)
        ));
    }
    cancellation.cancel();
    assert!(matches!(
        provider.resolve(
            &ImageSourceDemand::Composer {
                binding: current,
                marker_id: left.marker_id(),
                order_key: left.order_key()
            },
            &cancellation
        ),
        Err(ImageSourceError::Cancelled)
    ));
}

#[test]
fn foreign_home_cannot_adopt_an_equal_candidate_occurrence() {
    let (_directory, home, state, storage, thread) =
        host_fixture::fixture_with_state("image-source-origin", 72);
    let (left, _) = host_fixture::populate(storage.clone(), &home, thread, 92);
    let binding = binding(&home, &storage, thread, 92);
    let (_foreign_directory, foreign_home, foreign_state, foreign_storage, _) =
        host_fixture::fixture_with_state("image-source-foreign", 72);
    let provider = source_provider(&foreign_home, foreign_storage, foreign_state.assets());
    assert!(matches!(
        provider.resolve(
            &ImageSourceDemand::Composer {
                binding,
                marker_id: left.marker_id(),
                order_key: left.order_key()
            },
            &CommandCancellation::new()
        ),
        Err(ImageSourceError::Stale)
    ));
    drop(state);
}

#[test]
fn prepared_initial_provider_is_fenced_until_exact_candidate_publication() {
    use beryl_home_store::{HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion};
    let directory = tempfile::tempdir().unwrap();
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = beryl_state::BerylState::register(&mut candidate).unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let publication = candidate
        .prepare_publication(
            beryl_state::BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap();
    let scope = publication.generation_identity().unwrap();
    let provider =
        ImageSourceProvider::prepare_initial(&publication, storage, state.assets(), limits())
            .unwrap();
    assert_eq!(provider.scope, scope);
    assert!(matches!(
        provider.check_scope(&CommandCancellation::new()),
        Err(ImageSourceError::Stale)
    ));
    let (home, published_scope) = publication.publish_with_identity().unwrap();
    assert_eq!(published_scope, scope);
    provider.check_scope(&CommandCancellation::new()).unwrap();
    drop(provider);
    home.close().unwrap();
}
#[test]
fn opaque_source_opens_exact_admitted_bytes_and_rejects_foreign_scope_and_missing_bytes() {
    let (_directory, home, state, storage, _) =
        host_fixture::fixture_with_state("image-source-read", 73);
    let assets = state.assets();
    let bytes = b"admitted ordinary source bytes";
    let admitted = home
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            bytes,
            SidecarByteLimit::new(NonZeroU64::new(1024).unwrap()),
        )
        .unwrap();
    let address = admitted.address().clone();
    let asset = AssetId::sha256_v1(
        address.digest().as_bytes(),
        NonZeroU64::new(address.length()).unwrap(),
    );
    let revision = assets.revision(&home).unwrap();
    let contribution = assets
        .publish_metadata(
            revision,
            admitted,
            PublishAssetMetadata::new(
                asset,
                AssetMediaType::new("image/png").unwrap(),
                Some(AssetDimensions::new(
                    NonZeroU64::new(2).unwrap(),
                    NonZeroU64::new(3).unwrap(),
                )),
                revision.checked_next().unwrap(),
            ),
        )
        .unwrap();
    let mut command = beryl_home_store::HomeCommand::new(home.home_revision().unwrap());
    contribution.add_to(&mut command).unwrap();
    host_fixture::committed(home.execute(command));
    let metadata = assets.metadata(&home, asset).unwrap().unwrap();
    let source = ResolvedImageSource {
        scope: home.generation_identity().unwrap(),
        dimensions: admitted_dimensions(&metadata, limits()).unwrap(),
        metadata,
    };
    let provider = source_provider(&home, storage, assets);
    let mut reader = provider
        .open_verified(&source, CommandCancellation::new())
        .unwrap();
    let path = reader.verified_sidecar().path().to_path_buf();
    assert_eq!(source.asset_id(), asset);
    assert_eq!(source.dimensions(), [2, 3]);
    assert_eq!(source.metadata().asset_id(), asset);
    assert!(reader.fill_buf().unwrap().len() <= limits().configuration().sidecar_io_page_bytes);
    let mut found = Vec::new();
    reader.read_to_end(&mut found).unwrap();
    assert_eq!(found, bytes);
    drop(reader);
    let (_foreign_directory, foreign_home, foreign_state, foreign_storage, _) =
        host_fixture::fixture_with_state("image-source-reader-foreign", 74);
    let foreign_provider = source_provider(&foreign_home, foreign_storage, foreign_state.assets());
    assert!(matches!(
        foreign_provider.open_verified(&source, CommandCancellation::new()),
        Err(ImageSourceError::Stale)
    ));
    std::fs::remove_file(path).unwrap();
    assert!(matches!(
        provider.open_verified(&source, CommandCancellation::new()),
        Err(ImageSourceError::Sidecar(SidecarError::Missing))
    ));
}

#[test]
fn queued_source_demands_have_fixed_owners_and_no_drop_custody() {
    fn assert_worker_safe<T: Send + Sync + Copy>() {}
    assert_worker_safe::<ImageSourceDemand>();
    assert!(!std::mem::needs_drop::<ImageSourceDemand>());
    assert!(std::mem::size_of::<ImageSourceDemand>() <= 1024);
}

#[test]
fn exact_original_reader_is_independent_of_format_dimensions_and_rendition_length() {
    let (_directory, home, state, storage, _) =
        host_fixture::fixture_with_state("original-source-limits", 77);
    let assets = state.assets();
    let bytes = b"an exact original unsupported by the rendition decoder";
    let admitted = home
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            bytes,
            SidecarByteLimit::new(NonZeroU64::new(1024).unwrap()),
        )
        .unwrap();
    let asset = AssetId::sha256_v1(
        admitted.address().digest().as_bytes(),
        NonZeroU64::new(admitted.address().length()).unwrap(),
    );
    let revision = assets.revision(&home).unwrap();
    let contribution = assets
        .publish_metadata(
            revision,
            admitted,
            PublishAssetMetadata::new(
                asset,
                AssetMediaType::new("image/webp").unwrap(),
                None,
                revision.checked_next().unwrap(),
            ),
        )
        .unwrap();
    let mut command = beryl_home_store::HomeCommand::new(home.home_revision().unwrap());
    contribution.add_to(&mut command).unwrap();
    host_fixture::committed(home.execute(command));
    let metadata = assets.metadata(&home, asset).unwrap().unwrap();
    let mut config = limits().configuration();
    config.rendition_source_max_bytes = 1;
    let limits = MediaResourceLimits::new(config).unwrap();
    assert!(matches!(
        admitted_dimensions(&metadata, limits),
        Err(ImageSourceError::Unsupported)
    ));
    let original = ResolvedOriginalImageSource {
        scope: home.generation_identity().unwrap(),
        metadata,
    };
    let provider =
        ImageSourceProvider::new(home.service_reference(), storage, assets, limits).unwrap();
    let mut reader = provider
        .open_original_verified(&original, CommandCancellation::new())
        .unwrap();
    assert!(reader.fill_buf().unwrap().len() <= config.sidecar_io_page_bytes);
    let mut found = Vec::new();
    reader.read_to_end(&mut found).unwrap();
    assert_eq!(found, bytes);
    drop(reader);
    let path = provider
        .open_original_verified(&original, CommandCancellation::new())
        .unwrap()
        .verified_sidecar()
        .path()
        .to_path_buf();
    std::fs::write(path, b"corrupted").unwrap();
    assert!(matches!(
        provider.open_original_verified(&original, CommandCancellation::new()),
        Err(ImageSourceError::Sidecar(SidecarError::ContentMismatch))
    ));
}

#[test]
fn original_authentication_keeps_exact_occurrence_checks_without_rendition_admission() {
    let (_directory, home, state, storage, thread) =
        host_fixture::fixture_with_state("original-occurrence", 78);
    let (marker, _) = host_fixture::populate(storage.clone(), &home, thread, 79);
    let current = binding(&home, &storage, thread, 79);
    let provider = source_provider(&home, storage, state.assets());
    assert!(matches!(
        provider.resolve_original(
            &ImageSourceDemand::Composer {
                binding: current,
                marker_id: marker.marker_id(),
                order_key: marker.order_key() + 1
            },
            &CommandCancellation::new()
        ),
        Err(ImageSourceError::Identity)
    ));
    assert!(matches!(
        provider.resolve_original(
            &ImageSourceDemand::Composer {
                binding: current,
                marker_id: marker.marker_id(),
                order_key: marker.order_key()
            },
            &CommandCancellation::new()
        ),
        Err(ImageSourceError::Unavailable)
    ));
}
