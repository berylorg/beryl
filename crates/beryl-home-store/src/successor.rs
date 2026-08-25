use sha2::{Digest, Sha256};

use crate::{MutationBuildError, ReconciliationReader, RecordCodec, StorageDomain};

mod erased;
mod execution;
mod reader;

pub(crate) use erased::{
    DerivedReadFact, SuccessorDescriptor, SuccessorExecution, SuccessorProtocolIdentity,
    SuccessorRoleDescriptor, SuccessorRoleKind, SuccessorRoleReservation, SuccessorRoleResult,
    reserve_source, reserve_witness,
};
pub use reader::{
    SuccessorPointReader, SuccessorPointRecord, SuccessorReadRejection, SuccessorReadReservation,
};

const SUCCESSOR_PROTOCOL_FIXED_BYTES: usize = 256;
const SUCCESSOR_ROLE_FIXED_BYTES: usize = 192;
const SUCCESSOR_READ_FIXED_BYTES: usize = 192;

pub trait SuccessorCorrelation: Copy + Eq + Send + Sync + 'static {
    const ENCODED_BYTES: usize;

    fn encode(&self, output: &mut [u8]);
}

pub trait SuccessorProtocol: Send + Sync + 'static {
    const NAME: &'static str;
    type Correlation: SuccessorCorrelation;
}

pub struct FirstAcceptancePromotionProtocolV1;

impl SuccessorProtocol for FirstAcceptancePromotionProtocolV1 {
    const NAME: &'static str = "first-acceptance-promotion-v1";
    type Correlation = beryl_model::FirstAcceptancePromotionSuccessorV1;
}

impl SuccessorCorrelation for beryl_model::FirstAcceptancePromotionSuccessorV1 {
    const ENCODED_BYTES: usize = 184;

    fn encode(&self, output: &mut [u8]) {
        output.fill(0);
        output[..16].copy_from_slice(self.accepted_input_id().as_bytes());
        output[16..32].copy_from_slice(self.submitted_item_id().as_bytes());
        let Some(proof) = self.asset_reference_set() else {
            return;
        };
        output[32] = 1;
        output[33..49].copy_from_slice(proof.set_id().as_bytes());
        output[49..81].copy_from_slice(&proof.sequential().marker_digest());
        output[81..89].copy_from_slice(&proof.sequential().marker_count().to_be_bytes());
        output[89..97].copy_from_slice(
            &proof
                .sequential()
                .maximum_image_label()
                .map_or(0, beryl_model::ImageLabelOrdinal::get)
                .to_be_bytes(),
        );
        output[97..129].copy_from_slice(&proof.ordered_assets().marker_asset_digest());
        output[129..137].copy_from_slice(&proof.ordered_assets().marker_count().to_be_bytes());
        output[137..145].copy_from_slice(&proof.entry_frontier().to_be_bytes());
        output[145..177].copy_from_slice(&proof.asset_chain_digest().as_bytes());
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SuccessorObservation<C> {
    Authenticated(C),
    Unresolved,
    Collision,
}

pub trait SuccessorSource<D, P>: Copy + Send + Sync + 'static
where
    D: StorageDomain,
    P: SuccessorProtocol,
{
    const MAX_RETAINED_BYTES: usize;

    fn authenticate(
        &self,
        reader: &ReconciliationReader<'_, D>,
    ) -> Result<SuccessorObservation<P::Correlation>, D::ValidationError>;
}

pub trait SuccessorPointRead<D, P>: Send + Sync + 'static
where
    D: StorageDomain,
    P: SuccessorProtocol,
{
    type Record: RecordCodec<D>;
    const MAX_DECODED_BYTES: usize;

    fn derive_key(
        correlation: &P::Correlation,
        ordinal: usize,
    ) -> <Self::Record as RecordCodec<D>>::Key;

    fn expected_value(
        correlation: &P::Correlation,
        ordinal: usize,
    ) -> <Self::Record as RecordCodec<D>>::Value;
}

pub trait SuccessorWitness<D, P>: Copy + Send + Sync + 'static
where
    D: StorageDomain,
    P: SuccessorProtocol,
{
    const MAX_RETAINED_BYTES: usize;

    fn reserve_reads(
        &self,
        reservation: &mut SuccessorReadReservation<'_, D, P>,
    ) -> Result<(), MutationBuildError>;

    fn authenticate(
        &self,
        reader: &mut SuccessorPointReader<'_, D, P>,
    ) -> Result<SuccessorObservation<P::Correlation>, D::ValidationError>;
}

pub(crate) fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
