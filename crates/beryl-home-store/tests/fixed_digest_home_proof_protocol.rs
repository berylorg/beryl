mod support;

use std::any::TypeId;

use beryl_home_store::{
    DomainReader, FixedDigestHomeProofProtocol, HomeProofCommand, HomeProofProtocol,
    ProofCommandBuildError, ProofCorrelationBytes, ProofDomain, ProofProtocolIdentity,
};
use tempfile::tempdir;

use support::{AlphaDomain, BetaDomain, FixtureMutationError, open_home};

type SourceProtocol = FixedDigestHomeProofProtocol<0x7072_6f74_6f63_6f6c, 0x6f70_6572_6174_696f>;
type WitnessProtocol = FixedDigestHomeProofProtocol<0x7072_6f74_6f63_6f6c, 0x6f70_6572_6174_696f>;
type DifferentProtocolId =
    FixedDigestHomeProofProtocol<0x7072_6f74_6f63_6f6d, 0x6f70_6572_6174_696f>;
type DifferentOperationId =
    FixedDigestHomeProofProtocol<0x7072_6f74_6f63_6f6c, 0x6f70_6572_6174_696e>;

fn proof_correlation(input: &[u8; 32]) -> Result<ProofCorrelationBytes, FixtureMutationError> {
    Ok(ProofCorrelationBytes::new(*input))
}

macro_rules! fixed_digest_proof_domain {
    ($domain:ty) => {
        impl ProofDomain for $domain {
            type SourceInput = [u8; 32];
            type WitnessInput = [u8; 32];
            type Error = FixtureMutationError;

            fn source_protocol(_input: &Self::SourceInput) -> ProofProtocolIdentity {
                ProofProtocolIdentity::of::<SourceProtocol>()
            }

            fn expected_source_correlation(input: &Self::SourceInput) -> ProofCorrelationBytes {
                ProofCorrelationBytes::new(*input)
            }

            fn witness_protocol(_input: &Self::WitnessInput) -> ProofProtocolIdentity {
                ProofProtocolIdentity::of::<WitnessProtocol>()
            }

            fn prove_source(
                input: &Self::SourceInput,
                _reader: &DomainReader<'_, Self>,
            ) -> Result<ProofCorrelationBytes, Self::Error> {
                proof_correlation(input)
            }

            fn prove_witness(
                input: &Self::WitnessInput,
                _reader: &DomainReader<'_, Self>,
            ) -> Result<ProofCorrelationBytes, Self::Error> {
                proof_correlation(input)
            }
        }
    };
}

fixed_digest_proof_domain!(AlphaDomain);
fixed_digest_proof_domain!(BetaDomain);

#[test]
fn fixed_digest_protocol_shares_matching_ids_and_rejects_different_ids() {
    assert_eq!(
        TypeId::of::<SourceProtocol>(),
        TypeId::of::<WitnessProtocol>()
    );
    assert_eq!(SourceProtocol::CORRELATION_BYTES, 32);
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();
    let generation = store.health().generation().unwrap();
    let home_revision = store.home_revision().unwrap();
    let correlation = [7; 32];

    let mut command = HomeProofCommand::<SourceProtocol>::new(
        generation,
        home_revision,
        alpha.proof_source::<SourceProtocol>(store.domain_revision(&alpha).unwrap(), correlation),
    )
    .unwrap();
    command
        .add_witness(
            beta.proof_witness::<WitnessProtocol>(
                store.domain_revision(&beta).unwrap(),
                correlation,
            ),
        )
        .unwrap();
    let (command, consumer) = command.seal().unwrap();
    let receipt = store.compose_proof(command).unwrap();
    store.consume_proof_receipt(consumer, receipt).unwrap();

    assert!(matches!(
        HomeProofCommand::<DifferentProtocolId>::new(
            generation,
            home_revision,
            alpha.proof_source::<DifferentProtocolId>(
                store.domain_revision(&alpha).unwrap(),
                correlation,
            ),
        ),
        Err(ProofCommandBuildError::ProtocolMismatch)
    ));
    assert!(matches!(
        HomeProofCommand::<DifferentOperationId>::new(
            generation,
            home_revision,
            alpha.proof_source::<DifferentOperationId>(
                store.domain_revision(&alpha).unwrap(),
                correlation,
            ),
        ),
        Err(ProofCommandBuildError::ProtocolMismatch)
    ));
}
