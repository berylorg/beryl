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
