use super::*;
use crate::session::{ThreadClaimRecord, ThreadClaimState, codec::ClaimByWindowCodec};
use beryl_home_store::{
    CommandError, CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions,
    HomeSchemaVersion,
};
use beryl_model::{SyndicThreadId, WindowBounds, WindowDisplayState};

struct Collision {
    existing_header: bool,
    window_record: bool,
    id: WindowId,
    placement: WindowPlacement,
}

impl DomainMutation<SessionDomain> for Collision {
    type Error = SessionMutationError;
    type Prepared = Self;

    fn prepare(self, _: &DomainReader<'_, SessionDomain>) -> Result<Self, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<SessionHeaderCodec>(1)?;
        reservation.reserve_records::<SessionWindowCodec>(1)?;
        reservation.reserve_records::<ClaimByWindowCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self,
        mutations: &mut MutationBuilder<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        if prepared.existing_header {
            put_header(
                mutations,
                &SessionHeader {
                    revision: SessionRevision::new(9).unwrap(),
                    exit_intent: SessionExitIntent::Running,
                    fallback: None,
                    windows: vec![],
                },
            )?;
        }
        if prepared.window_record {
            put_window(
                mutations,
                &SessionWindowRecord {
                    window_id: prepared.id,
                    remembered_target: None,
                    selected_thread: None,
                    placement: prepared.placement,
                    revision: RecordRevision::INITIAL,
                },
            )?;
        } else {
            let claim = ThreadClaimRecord::new(
                prepared.id,
                SyndicThreadId::from_bytes([3; 16]),
                SessionRevision::new(9).unwrap(),
                ThreadClaimState::Restoring,
                super::super::shared::initial_claim_revision(),
            );
            mutations.put::<ClaimByWindowCodec>(&prepared.id, &claim)?;
        }
        Ok(())
    }
}

#[test]
fn initialization_rejects_orphan_window_and_claim_without_overwriting_records() {
    for existing_header in [false, true] {
        for window_record in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
                directory.path(),
                HomeSchemaVersion::CURRENT,
            ))
            .unwrap();
            let state = crate::BerylState::register(&mut candidate).unwrap();
            let store = candidate
                .prepare_publication(crate::BerylState::required_domains().unwrap())
                .unwrap()
                .publish()
                .unwrap();
            let session = state.session();
            let id = WindowId::from_bytes([2; 16]);
            let placement = WindowPlacement::new(
                WindowBounds::new(0, 0, 800, 600).unwrap(),
                WindowDisplayState::Normal,
                None,
                None,
            );
            let mut command = HomeCommand::new(store.home_revision().unwrap());
            command
                .add(session.handle.contribution(
                    session.revision(&store).unwrap(),
                    Collision {
                        existing_header,
                        window_record,
                        id,
                        placement: placement.clone(),
                    },
                ))
                .unwrap();
            assert!(matches!(
                store.execute(command),
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
            let home_revision = store.home_revision().unwrap();
            let domain_revision = session.revision(&store).unwrap();
            let request = if existing_header {
                InitializeThreadlessWindow::for_empty_session(
                    SessionRevision::new(9).unwrap(),
                    id,
                    placement,
                )
            } else {
                InitializeThreadlessWindow::new(id, placement)
            };
            let mut command = HomeCommand::new(home_revision);
            command
                .add(session.initialize_threadless(domain_revision, request))
                .unwrap();
            let CommandOutcome::NotCommitted {
                evidence: CommandError::ContributorValidation { source, .. },
            } = store.execute(command)
            else {
                panic!("expected initialization rejection")
            };
            assert!(
                matches!(source.downcast_ref::<SessionMutationError>(), Some(SessionMutationError::WindowExists { window_id }) if *window_id == id)
            );
            assert_eq!(store.home_revision().unwrap(), home_revision);
            assert_eq!(session.revision(&store).unwrap(), domain_revision);
        }
    }
}
