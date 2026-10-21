use super::*;

#[test]
fn preparation_refuses_missing_or_unbound_peer_without_writes() {
    for unbound in [false, true] {
        let home = open("missing-peer");
        let revision = home.revision();
        let home_revision = home.store.home_revision().unwrap();
        let prep = home.preparation(target());
        let mut command = HomeCommand::new(home_revision);
        prep.add_to(&mut command).unwrap();
        if unbound {
            command
                .add_validation(home.peer.validation(
                    home.peer_revision(),
                    PeerGuard {
                        pair: None,
                        reject: false,
                    },
                ))
                .unwrap();
        }
        assert!(matches!(
            rejected(home.store.execute(command)),
            CommandError::InvalidCommandPair
        ));
        assert_eq!(home.revision(), revision);
        assert_eq!(home.store.home_revision().unwrap(), home_revision);
        assert!(
            home.state
                .assets()
                .generated_media_admission(&home.store, target())
                .unwrap()
                .is_none()
        );
        home.store.close().unwrap();
    }
}

#[test]
fn preparation_refuses_wrong_source_asset_operation_or_participant_kind() {
    for mismatch in 0..4 {
        let home = open("mismatched-peer");
        let revision = home.revision();
        let prep = home.preparation(target());
        let mut digest = asset_digest(1, target(), prep.facts());
        if mismatch == 0 {
            let substituted = GeneratedMediaTarget::new(
                target().resource(),
                target().thread(),
                target().turn(),
                target().item(),
                [9; 32],
            );
            digest = asset_digest(1, substituted, prep.facts());
        } else if mismatch == 1 {
            let substituted = GeneratedMediaFacts::new(
                prep.facts().asset(),
                AssetMediaType::new("image/jpeg").unwrap(),
                prep.facts().dimensions(),
            )
            .unwrap();
            digest = asset_digest(1, target(), &substituted);
        }
        let mut command = HomeCommand::new(home.store.home_revision().unwrap());
        prep.add_to(&mut command).unwrap();
        if mismatch == 3 {
            command
                .add(home.peer.contribution(
                    home.peer_revision(),
                    PeerWrite {
                        pair: Some(pair::<1>(CommandParticipantKind::Mutation, digest)),
                        value: 2,
                    },
                ))
                .unwrap();
        } else {
            let requirement = if mismatch == 2 {
                pair::<2>(CommandParticipantKind::Mutation, digest)
            } else {
                pair::<1>(CommandParticipantKind::Mutation, digest)
            };
            command
                .add_validation(home.peer.validation(
                    home.peer_revision(),
                    PeerGuard {
                        pair: Some(requirement),
                        reject: false,
                    },
                ))
                .unwrap();
        }
        assert!(matches!(
            rejected(home.store.execute(command)),
            CommandError::InvalidCommandPair
        ));
        assert_eq!(home.revision(), revision);
        assert_eq!(home.peer_value(), Some(1));
        home.store.close().unwrap();
    }
}

#[test]
fn matching_pair_still_requires_the_peer_semantic_validation() {
    let home = open("peer-semantic-refusal");
    let prep = home.preparation(target());
    let digest = asset_digest(1, target(), prep.facts());
    let mut command = HomeCommand::new(home.store.home_revision().unwrap());
    prep.add_to(&mut command).unwrap();
    command
        .add_validation(home.peer.validation(
            home.peer_revision(),
            PeerGuard {
                pair: Some(pair::<1>(CommandParticipantKind::Mutation, digest)),
                reject: true,
            },
        ))
        .unwrap();
    rejected(home.store.execute(command));
    assert!(
        home.state
            .assets()
            .generated_media_admission(&home.store, target())
            .unwrap()
            .is_none()
    );
    home.store.close().unwrap();
}

#[test]
fn preparation_refuses_existing_metadata_with_conflicting_media_or_dimensions() {
    for (media, dims) in [
        ("image/jpeg", None),
        ("image/png", Some(dimensions(21, 30))),
    ] {
        let home = open("metadata-conflict");
        seed_metadata(&home, media, dims);
        let token = sidecar(&home.store, BYTES, "images");
        let bytes = facts(&token, Some(dimensions(20, 30)));
        assert!(matches!(
            home.state.assets().prepare_generated_media(
                &home.store,
                home.revision(),
                token,
                target(),
                bytes
            ),
            Err(GeneratedMediaError::Rejected)
        ));
        home.store.close().unwrap();
    }
}

#[test]
fn preparation_requires_exact_sidecar_namespace_and_content_identity() {
    for mismatch_namespace in [false, true] {
        let home = open("sidecar-identity");
        let original = sidecar(&home.store, BYTES, "images");
        let bytes = facts(&original, None);
        let token = sidecar(
            &home.store,
            if mismatch_namespace {
                BYTES
            } else {
                b"different bytes"
            },
            if mismatch_namespace {
                "other-images"
            } else {
                "images"
            },
        );
        assert!(matches!(
            home.state.assets().prepare_generated_media(
                &home.store,
                home.revision(),
                token,
                target(),
                bytes
            ),
            Err(GeneratedMediaError::Rejected)
        ));
        home.store.close().unwrap();
    }
}

#[test]
fn foreign_generation_sidecar_cannot_prepare_matching_metadata() {
    let original = open("foreign-sidecar-source");
    let token = sidecar(&original.store, BYTES, "images");
    let bytes = facts(&token, None);
    let home = open("foreign-sidecar-destination");
    let prep = home
        .state
        .assets()
        .prepare_generated_media(&home.store, home.revision(), token, target(), bytes)
        .unwrap();
    let digest = asset_digest(1, target(), prep.facts());
    let mut command = HomeCommand::new(home.store.home_revision().unwrap());
    prep.add_to(&mut command).unwrap();
    command
        .add_validation(home.peer.validation(
            home.peer_revision(),
            PeerGuard {
                pair: Some(pair::<1>(CommandParticipantKind::Mutation, digest)),
                reject: false,
            },
        ))
        .unwrap();
    rejected(home.store.execute(command));
    assert!(
        home.state
            .assets()
            .generated_media_admission(&home.store, target())
            .unwrap()
            .is_none()
    );
    home.store.close().unwrap();
    original.store.close().unwrap();
}

#[test]
fn source_or_natural_target_substitution_is_a_collision() {
    let home = open("source-substitution");
    home.prepared(target());
    let wrong = GeneratedMediaTarget::new(
        target().resource(),
        target().thread(),
        target().turn(),
        target().item(),
        [99; 32],
    );
    assert!(matches!(
        home.state
            .assets()
            .generated_media_admission(&home.store, wrong),
        Err(GeneratedMediaError::Rejected)
    ));
    let token = sidecar(&home.store, BYTES, "images");
    let bytes = facts(&token, Some(dimensions(20, 30)));
    assert!(matches!(
        home.state.assets().prepare_generated_media(
            &home.store,
            home.revision(),
            token,
            wrong,
            bytes
        ),
        Err(GeneratedMediaError::Rejected)
    ));
    let wrong = GeneratedMediaTarget::new(
        target().resource(),
        target().thread(),
        target().turn(),
        beryl_model::SyndicItemId::from_bytes([99; 16]),
        target().fingerprint(),
    );
    assert!(matches!(
        home.state
            .assets()
            .generated_media_admission(&home.store, wrong),
        Err(GeneratedMediaError::Rejected)
    ));
    home.store.close().unwrap();
}

#[test]
fn unavailable_refuses_any_prepared_or_published_record_and_binds_reason() {
    for publish in [false, true] {
        let home = open("unavailable-cannot-downgrade");
        let admission = home.prepared(target());
        if publish {
            committed(home.store.execute(home.publication_command(&admission)));
        }
        let revision = home.revision();
        rejected(
            home.store.execute(
                home.unavailable_command(target(), GeneratedMediaUnavailableReason::Missing),
            ),
        );
        assert_eq!(home.revision(), revision);
        home.store.close().unwrap();
    }
    let home = open("unavailable-reason");
    let mut command = HomeCommand::new(home.store.home_revision().unwrap());
    command
        .add_validation(home.state.assets().validate_generated_media_absence(
            home.revision(),
            target(),
            GeneratedMediaUnavailableReason::Missing,
        ))
        .unwrap();
    command
        .add(home.peer.contribution(
            home.peer_revision(),
            PeerWrite {
                pair: Some(pair::<3>(
                    CommandParticipantKind::Validation,
                    unavailable_digest(target(), GeneratedMediaUnavailableReason::ProducerFailed),
                )),
                value: 3,
            },
        ))
        .unwrap();
    assert!(matches!(
        rejected(home.store.execute(command)),
        CommandError::InvalidCommandPair
    ));
    assert_eq!(home.peer_value(), Some(1));
    home.store.close().unwrap();
}

#[test]
fn publication_cannot_execute_without_its_matching_syndic_mutation() {
    let home = open("publication-split");
    let admission = home.prepared(target());
    let revision = home.revision();
    let contribution = home
        .state
        .assets()
        .publish_generated_media(&home.store, revision, &admission)
        .unwrap();
    let mut command = HomeCommand::new(home.store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        rejected(home.store.execute(command)),
        CommandError::InvalidCommandPair
    ));
    assert_eq!(
        home.admission(target()).lifecycle(),
        GeneratedMediaLifecycle::Prepared
    );
    assert_eq!(home.revision(), revision);
    home.store.close().unwrap();
}

#[test]
fn duplicate_asset_participants_cannot_split_the_metadata_and_reference_work() {
    let home = open("duplicate-domain");
    let preparation = home.preparation(target());
    let mut command = HomeCommand::new(home.store.home_revision().unwrap());
    preparation.add_to(&mut command).unwrap();
    assert!(
        command
            .add_validation(home.state.assets().validate_generated_media_absence(
                home.revision(),
                target(),
                GeneratedMediaUnavailableReason::Missing
            ))
            .is_err()
    );
    home.store.close().unwrap();
}
