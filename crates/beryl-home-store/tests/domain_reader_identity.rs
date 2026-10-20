mod support;

use std::{convert::Infallible, marker::PhantomData};

use beryl_home_store::{
    DomainHandle, DomainMutation, DomainReader, DomainRegistrationReader, DomainSchemaVersion,
    DomainValidator, HomeCommand, HomeDomainRequirements, HomeOpenCandidate, HomeProofCommand,
    HomeProofProtocol, HomeStore, KeyspaceSchemaVersion, MutationBuilder, PointReadLimit,
    ProofCorrelationBytes, ProofDomain, ProofProtocolIdentity, ReconciliationReservation,
    RecordFamily, StorageDomain, WholeHomeScrubTrigger,
};
use beryl_model::BerylHomeId;
use support::{BytesRecord, FixtureMutationError, PutBytes, committed, not_committed};

struct IdentitySource;
struct IdentityWitness;

macro_rules! identity_domain {
    ($domain:ident,$name:literal) => {
        impl StorageDomain for $domain {
            const NAME: &'static str = $name;
            const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
            const FAMILIES: &'static [RecordFamily<Self>] =
                &[RecordFamily::new::<BytesRecord<Self>>(
                    KeyspaceSchemaVersion::new(1),
                )];
            type ValidationError = FixtureMutationError;
            type RuntimeAttachment = ();
            type RuntimeAttachmentError = Infallible;

            fn create_runtime_attachment(
                _: &DomainRegistrationReader<'_, Self>,
            ) -> Result<(), Infallible> {
                Ok(())
            }

            fn validate(reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
                let saved = reader
                    .point::<BytesRecord<Self>>(&1, PointReadLimit::new(64).unwrap())
                    .map_err(|_| FixtureMutationError::Rejected("identity record read failed"))?;
                if saved
                    .as_deref()
                    .is_some_and(|saved| saved != reader.home_id().as_bytes())
                {
                    return Err(FixtureMutationError::Rejected(
                        "reader identity differs from validated header",
                    ));
                }
                Ok(())
            }
        }

        impl ProofDomain for $domain {
            type SourceInput = BerylHomeId;
            type WitnessInput = BerylHomeId;
            type Error = FixtureMutationError;

            fn source_protocol(_: &BerylHomeId) -> ProofProtocolIdentity {
                ProofProtocolIdentity::of::<HomeIdentityProtocol>()
            }
            fn witness_protocol(_: &BerylHomeId) -> ProofProtocolIdentity {
                ProofProtocolIdentity::of::<HomeIdentityProtocol>()
            }
            fn expected_source_correlation(expected: &BerylHomeId) -> ProofCorrelationBytes {
                ProofCorrelationBytes::new(*expected.as_bytes())
            }
            fn prove_source(
                expected: &BerylHomeId,
                reader: &DomainReader<'_, Self>,
            ) -> Result<ProofCorrelationBytes, Self::Error> {
                prove_identity(expected, reader)
            }
            fn prove_witness(
                expected: &BerylHomeId,
                reader: &DomainReader<'_, Self>,
            ) -> Result<ProofCorrelationBytes, Self::Error> {
                prove_identity(expected, reader)
            }
        }
    };
}

identity_domain!(IdentitySource, "identity_source");
identity_domain!(IdentityWitness, "identity_witness");

struct HomeIdentityProtocol;
impl HomeProofProtocol for HomeIdentityProtocol {
    type Correlation = [u8; 16];
    const PROTOCOL_ID: u64 = 0x686f6d655f6964;
    const OPERATION_ID: u64 = 1;
    const CORRELATION_BYTES: usize = 16;
}

fn prove_identity<D: StorageDomain>(
    expected: &BerylHomeId,
    reader: &DomainReader<'_, D>,
) -> Result<ProofCorrelationBytes, FixtureMutationError> {
    if reader.home_id() != *expected {
        return Err(FixtureMutationError::Rejected(
            "proof reader belongs to another home",
        ));
    }
    Ok(ProofCorrelationBytes::new(*reader.home_id().as_bytes()))
}

struct StoreIdentity(BerylHomeId);
impl DomainMutation<IdentitySource> for StoreIdentity {
    type Error = FixtureMutationError;
    type Prepared = BerylHomeId;

    fn prepare(
        self,
        reader: &DomainReader<'_, IdentitySource>,
    ) -> Result<Self::Prepared, Self::Error> {
        if reader.home_id() != self.0 {
            return Err(FixtureMutationError::Rejected(
                "mutation reader belongs to another home",
            ));
        }
        Ok(reader.home_id())
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, IdentitySource>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<BytesRecord<IdentitySource>>(1)?;
        Ok(())
    }
    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, IdentitySource>,
    ) -> Result<(), Self::Error> {
        mutations.put::<BytesRecord<IdentitySource>>(&1, &prepared.as_bytes().to_vec())?;
        Ok(())
    }
}

struct RequireIdentity<D>(BerylHomeId, PhantomData<fn(D) -> D>);
impl<D: StorageDomain> DomainValidator<D> for RequireIdentity<D> {
    type Error = FixtureMutationError;
    fn validate(&self, reader: &DomainReader<'_, D>) -> Result<(), Self::Error> {
        if reader.home_id() != self.0 {
            return Err(FixtureMutationError::Rejected(
                "validator reader belongs to another home",
            ));
        }
        Ok(())
    }
}

fn requirements() -> HomeDomainRequirements {
    HomeDomainRequirements::new()
        .with_domain::<IdentitySource>()
        .unwrap()
        .with_domain::<IdentityWitness>()
        .unwrap()
}

fn publish(
    mut candidate: HomeOpenCandidate,
) -> (
    HomeStore,
    DomainHandle<IdentitySource>,
    DomainHandle<IdentityWitness>,
) {
    let source = candidate
        .register_domain_with_schema_validation::<IdentitySource>()
        .unwrap();
    let witness = candidate
        .register_domain_with_schema_validation::<IdentityWitness>()
        .unwrap();
    let home = candidate
        .prepare_publication(requirements())
        .unwrap()
        .publish()
        .unwrap();
    (home, source, witness)
}

fn prove(
    home: &HomeStore,
    source: &DomainHandle<IdentitySource>,
    witness: &DomainHandle<IdentityWitness>,
) {
    let before = home.home_revision().unwrap();
    let expected = home.home_id();
    let mut proof = HomeProofCommand::<HomeIdentityProtocol>::new(
        home.health().generation().unwrap(),
        before,
        source
            .proof_source::<HomeIdentityProtocol>(home.domain_revision(source).unwrap(), expected),
    )
    .unwrap();
    proof
        .add_witness(witness.proof_witness(home.domain_revision(witness).unwrap(), expected))
        .unwrap();
    let (proof, consumer) = proof.seal().unwrap();
    let receipt = home.compose_proof(proof).unwrap();
    home.consume_proof_receipt(consumer, receipt).unwrap();
    assert_eq!(home.home_revision().unwrap(), before);
    assert!(home.pending_reconciliations().is_empty());
}

#[test]
fn command_and_proof_readers_observe_their_actual_home_across_distinct_homes() {
    let first_directory = tempfile::tempdir().unwrap();
    let second_directory = tempfile::tempdir().unwrap();
    let (first, first_source, first_witness) = publish(support::open_home(first_directory.path()));
    let (second, second_source, second_witness) =
        publish(support::open_home(second_directory.path()));
    assert_ne!(first.home_id(), second.home_id());
    for (home, source, witness, foreign) in [
        (&first, &first_source, &first_witness, second.home_id()),
        (&second, &second_source, &second_witness, first.home_id()),
    ] {
        let mut command = HomeCommand::new(home.home_revision().unwrap());
        command
            .add(source.contribution(
                home.domain_revision(source).unwrap(),
                StoreIdentity(home.home_id()),
            ))
            .unwrap();
        command
            .add_validation(witness.validation(
                home.domain_revision(witness).unwrap(),
                RequireIdentity::<IdentityWitness>(home.home_id(), PhantomData),
            ))
            .unwrap();
        committed(home.execute(command));
        let revision = home.home_revision().unwrap();
        not_committed(home.execute_current(source.current_command(StoreIdentity(foreign))));
        assert_eq!(home.home_revision().unwrap(), revision);
        assert_eq!(
            home.read_point::<IdentitySource, BytesRecord<IdentitySource>>(
                source,
                &1,
                PointReadLimit::new(64).unwrap()
            )
            .unwrap(),
            Some(home.home_id().as_bytes().to_vec())
        );
        prove(home, source, witness);
        home.scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .unwrap();
    }
    first.close().unwrap();
    second.close().unwrap();
}

#[test]
fn reopened_provisional_and_registered_validators_use_the_header_instead_of_domain_records() {
    let directory = tempfile::tempdir().unwrap();
    let (home, source, _) = publish(support::open_home(directory.path()));
    let identity = home.home_id();
    committed(home.execute_current(source.current_command(StoreIdentity(identity))));
    home.close().unwrap();
    let (home, source, witness) = publish(support::open_home(directory.path()));
    assert_eq!(home.home_id(), identity);
    prove(&home, &source, &witness);
    home.scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let foreign = if identity.as_bytes() == &[7; 16] {
        BerylHomeId::from_bytes([8; 16])
    } else {
        BerylHomeId::from_bytes([7; 16])
    };
    committed(
        home.execute_current(source.current_command(PutBytes::<IdentitySource>::new(
            1,
            foreign.as_bytes().to_vec(),
        ))),
    );
    assert!(
        home.scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
    home.close().unwrap();
    let mut candidate = support::open_home(directory.path());
    assert!(matches!(
        candidate.register_domain_with_schema_validation::<IdentitySource>(),
        Err(beryl_home_store::DomainRegistrationError::Validation {
            domain: "identity_source",
            ..
        })
    ));
    candidate.close().unwrap();
}

#[test]
fn candidate_identity_matches_publication_and_distinguishes_closed_reopen_with_repeated_generation()
{
    let directory = tempfile::tempdir().unwrap();
    let mut candidate = support::open_home(directory.path());
    let first = candidate.generation_identity().unwrap();
    let generation = candidate.generation();
    let home_id = candidate.home_id();
    #[cfg(feature = "test-faults")]
    beryl_home_store::test_faults::with_initial_candidate_store(&candidate, |store| {
        assert!(store.generation_identity().is_err());
    });
    candidate.register_domain::<IdentitySource>().unwrap();
    candidate.register_domain::<IdentityWitness>().unwrap();
    let (home, published) = candidate
        .prepare_publication(requirements())
        .unwrap()
        .publish_with_identity()
        .unwrap();
    assert_eq!(published, first);
    assert_eq!(home.generation_identity().unwrap(), first);
    home.close().unwrap();
    let mut candidate = support::open_home(directory.path());
    assert_eq!(candidate.generation(), generation);
    assert_eq!(candidate.home_id(), home_id);
    let reopened = candidate.generation_identity().unwrap();
    assert_ne!(reopened, first);
    candidate.register_domain::<IdentitySource>().unwrap();
    candidate.register_domain::<IdentityWitness>().unwrap();
    let (home, published) = candidate
        .prepare_publication(requirements())
        .unwrap()
        .publish_with_identity()
        .unwrap();
    assert_eq!(published, reopened);
    assert_eq!(home.generation_identity().unwrap(), reopened);
    home.close().unwrap();
}

#[cfg(feature = "test-faults")]
#[test]
fn fresh_same_home_recovery_preserves_reader_identity_and_replaces_generation_authority() {
    use beryl_home_store::{
        HomeOpenOptions, HomeSchemaVersion,
        test_faults::{FaultController, FaultPoint},
    };
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let (home, old_source, _) = publish(candidate);
    let identity = home.home_id();
    let generation = home.generation_identity().unwrap();
    committed(home.execute_current(old_source.current_command(StoreIdentity(identity))));
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    let mut candidate = home.recover_same_home().unwrap();
    let candidate_identity = candidate.generation_identity().unwrap();
    assert_ne!(candidate_identity, generation);
    let fresh = candidate.domain_handle::<IdentitySource>().unwrap();
    let access = candidate.recovery_access().unwrap();
    assert_eq!(access.home_id(), identity);
    assert!(access.domain_revision(&old_source).is_err());
    committed(access.execute_current(fresh.current_command(StoreIdentity(identity))));
    let (home, published) = candidate.publish_with_identity().unwrap();
    assert_eq!(published, candidate_identity);
    assert_eq!(home.generation_identity().unwrap(), candidate_identity);
    assert_eq!(home.home_id(), identity);
    assert_ne!(home.generation_identity().unwrap(), generation);
    let source = home.domain_handle::<IdentitySource>().unwrap();
    let witness = home.domain_handle::<IdentityWitness>().unwrap();
    prove(&home, &source, &witness);
    home.scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    home.close().unwrap();
}

#[cfg(feature = "test-faults")]
#[test]
fn candidate_identity_capture_rejects_failed_opening_and_reopening_admission() {
    use beryl_home_store::{
        HomeOpenOptions, HomeSchemaVersion,
        test_faults::{FaultController, FaultPoint},
    };
    let opening_directory = tempfile::tempdir().unwrap();
    let mut opening = support::open_home(opening_directory.path());
    opening.register_domain::<IdentitySource>().unwrap();
    assert!(opening.generation_identity().is_ok());
    assert!(opening.register_domain::<IdentitySource>().is_err());
    assert!(opening.generation_identity().is_err());
    opening.close().unwrap();

    let recovery_directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(recovery_directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let (home, _, _) = publish(candidate);
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    let mut candidate = home.recover_same_home().unwrap();
    assert!(candidate.generation_identity().is_ok());
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .is_err()
    );
    assert!(candidate.generation_identity().is_err());
    candidate.abort().close().unwrap();
}
