use std::{any::TypeId, error::Error, marker::PhantomData};

use beryl_model::DomainRevision;
use thiserror::Error;

use crate::{
    DomainCallbackError, DomainCallbackSource, DomainHandle, DomainReader, HealthGateError,
    HomeGeneration, RevisionConflict, StorageDomain,
    domain::{
        DomainOwnerId, RegisteredDomain, StoreInstanceId,
        callback::{ErasedCallbackError, callback_failure_severity},
    },
    health::FailureSeverity,
    read::{read_domain_metadata, read_home_revision},
    store::StoreGeneration,
    writer::{ActiveWriter, FailClosedOnWriterPanic},
};

mod command;
mod execution;
mod receipt;

#[cfg(feature = "test-faults")]
pub use command::ProofCommandIdentityTestHarness;
pub use command::{
    ExecutableHomeProofCommand, HomeProofCommand, ProofCommandBuildError, ProofCommandSealError,
};
pub use receipt::{HomeProofReceipt, ProofReceiptConsumer, ProofReceiptError};

pub const MAX_PROOF_ROLES: usize = 8;
pub const MAX_PROOF_CORRELATION_BYTES: usize = 64;

pub trait HomeProofProtocol: Send + Sync + 'static {
    type Correlation: InlineProofCorrelation;

    const PROTOCOL_ID: u64;
    const OPERATION_ID: u64;
    const CORRELATION_BYTES: usize;
}

mod inline_correlation {
    pub trait Sealed {}
}

pub trait InlineProofCorrelation:
    inline_correlation::Sealed + Copy + Eq + Send + Sync + 'static
{
    fn into_bytes(self) -> ProofCorrelationBytes;
}

macro_rules! inline_proof_correlations {
    ($($length:expr),+ $(,)?) => {
        $(
            impl inline_correlation::Sealed for [u8; $length] {}
            impl InlineProofCorrelation for [u8; $length] {
                fn into_bytes(self) -> ProofCorrelationBytes {
                    ProofCorrelationBytes::from_array(self)
                }
            }
        )+
    };
}

inline_proof_correlations!(
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49,
    50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64,
);

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ProofCorrelationBytes {
    bytes: [u8; MAX_PROOF_CORRELATION_BYTES],
    len: u8,
}

impl ProofCorrelationBytes {
    fn from_array<const N: usize>(value: [u8; N]) -> Self {
        let mut bytes = [0; MAX_PROOF_CORRELATION_BYTES];
        bytes[..N].copy_from_slice(&value);
        Self {
            bytes,
            len: u8::try_from(N).expect("sealed inline correlation length fits u8"),
        }
    }

    #[must_use]
    pub fn new<C: InlineProofCorrelation>(value: C) -> Self {
        value.into_bytes()
    }
}

pub struct ProofProtocolIdentity {
    protocol: TypeId,
    protocol_id: u64,
    operation_id: u64,
    correlation_bytes: usize,
}

impl ProofProtocolIdentity {
    #[must_use]
    pub fn of<P: HomeProofProtocol>() -> Self {
        Self {
            protocol: TypeId::of::<P>(),
            protocol_id: P::PROTOCOL_ID,
            operation_id: P::OPERATION_ID,
            correlation_bytes: P::CORRELATION_BYTES,
        }
    }

    fn matches<P: HomeProofProtocol>(&self) -> bool {
        self.protocol == TypeId::of::<P>()
            && self.protocol_id == P::PROTOCOL_ID
            && self.operation_id == P::OPERATION_ID
            && self.correlation_bytes == P::CORRELATION_BYTES
    }
}

pub struct ProofCorrelation<P: HomeProofProtocol> {
    value: ProofCorrelationBytes,
    _protocol: PhantomData<fn(P) -> P>,
}

impl<P: HomeProofProtocol> Copy for ProofCorrelation<P> {}

impl<P: HomeProofProtocol> Clone for ProofCorrelation<P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<P: HomeProofProtocol> ProofCorrelation<P> {
    #[must_use]
    pub fn new(value: P::Correlation) -> Self {
        Self {
            value: value.into_bytes(),
            _protocol: PhantomData,
        }
    }

    pub(crate) fn agrees_with(self, other: Self) -> bool {
        self.value == other.value
    }

    fn from_bytes(value: ProofCorrelationBytes) -> Option<Self> {
        (usize::from(value.len) == P::CORRELATION_BYTES).then_some(Self {
            value,
            _protocol: PhantomData,
        })
    }
}

pub trait ProofDomain: StorageDomain {
    type SourceInput: Send + 'static;
    type WitnessInput: Send + 'static;
    type Error: DomainCallbackError;

    fn source_protocol(input: &Self::SourceInput) -> ProofProtocolIdentity;

    fn expected_source_correlation(input: &Self::SourceInput) -> ProofCorrelationBytes;

    fn witness_protocol(input: &Self::WitnessInput) -> ProofProtocolIdentity;

    fn prove_source(
        input: &Self::SourceInput,
        reader: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Self::Error>;

    fn prove_witness(
        input: &Self::WitnessInput,
        reader: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Self::Error>;
}

trait ErasedProofRole<P: HomeProofProtocol>: Send {
    fn prove(
        &self,
        snapshot: &fjall::Snapshot,
        domain: &RegisteredDomain,
    ) -> Result<ProofCorrelationBytes, ErasedCallbackError>;
}

struct TypedProofSource<P: HomeProofProtocol, D: ProofDomain> {
    input: D::SourceInput,
    _typed: PhantomData<fn(P, D)>,
}

impl<P: HomeProofProtocol, D: ProofDomain> ErasedProofRole<P> for TypedProofSource<P, D> {
    fn prove(
        &self,
        snapshot: &fjall::Snapshot,
        domain: &RegisteredDomain,
    ) -> Result<ProofCorrelationBytes, ErasedCallbackError> {
        D::prove_source(&self.input, &DomainReader::new(snapshot, domain))
            .map_err(ErasedCallbackError::from_typed)
    }
}

struct TypedProofWitness<P: HomeProofProtocol, D: ProofDomain> {
    input: D::WitnessInput,
    _typed: PhantomData<fn(P, D)>,
}

impl<P: HomeProofProtocol, D: ProofDomain> ErasedProofRole<P> for TypedProofWitness<P, D> {
    fn prove(
        &self,
        snapshot: &fjall::Snapshot,
        domain: &RegisteredDomain,
    ) -> Result<ProofCorrelationBytes, ErasedCallbackError> {
        D::prove_witness(&self.input, &DomainReader::new(snapshot, domain))
            .map_err(ErasedCallbackError::from_typed)
    }
}

pub(crate) struct ProofRolePlan<P: HomeProofProtocol> {
    pub(crate) store: StoreInstanceId,
    pub(crate) slot: usize,
    pub(crate) owner: DomainOwnerId,
    pub(crate) domain: &'static str,
    protocol: ProofProtocolIdentity,
    callback: Box<dyn ErasedProofRole<P>>,
}

pub struct ProofSourceContribution<P: HomeProofProtocol> {
    pub(crate) plan: ProofRolePlan<P>,
    pub(crate) expected_revision: DomainRevision,
    pub(crate) expected_correlation: ProofCorrelationBytes,
}

pub struct ProofWitnessContribution<P: HomeProofProtocol> {
    pub(crate) plan: ProofRolePlan<P>,
    pub(crate) expected_revision: DomainRevision,
}

impl<P: HomeProofProtocol> std::fmt::Debug for ProofSourceContribution<P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProofSourceContribution")
            .field("domain", &self.plan.domain)
            .field("expected_revision", &self.expected_revision)
            .finish_non_exhaustive()
    }
}

impl<P: HomeProofProtocol> std::fmt::Debug for ProofWitnessContribution<P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProofWitnessContribution")
            .field("domain", &self.plan.domain)
            .field("expected_revision", &self.expected_revision)
            .finish_non_exhaustive()
    }
}

impl<D: StorageDomain> DomainHandle<D> {
    pub fn proof_source<P: HomeProofProtocol>(
        &self,
        expected_revision: DomainRevision,
        input: <D as ProofDomain>::SourceInput,
    ) -> ProofSourceContribution<P>
    where
        D: ProofDomain,
    {
        let expected_correlation = D::expected_source_correlation(&input);
        ProofSourceContribution {
            plan: ProofRolePlan {
                store: self.store,
                slot: self.slot,
                owner: self.owner,
                domain: D::NAME,
                protocol: D::source_protocol(&input),
                callback: Box::new(TypedProofSource::<P, D> {
                    input,
                    _typed: PhantomData,
                }),
            },
            expected_revision,
            expected_correlation,
        }
    }

    pub fn proof_witness<P: HomeProofProtocol>(
        &self,
        expected_revision: DomainRevision,
        input: <D as ProofDomain>::WitnessInput,
    ) -> ProofWitnessContribution<P>
    where
        D: ProofDomain,
    {
        ProofWitnessContribution {
            plan: ProofRolePlan {
                store: self.store,
                slot: self.slot,
                owner: self.owner,
                domain: D::NAME,
                protocol: D::witness_protocol(&input),
                callback: Box::new(TypedProofWitness::<P, D> {
                    input,
                    _typed: PhantomData,
                }),
            },
            expected_revision,
        }
    }
}

#[derive(Debug, Error)]
pub enum ProofCompositionError {
    #[error(transparent)]
    HealthGate(#[from] HealthGateError),
    #[error("proof command was cancelled before writer admission")]
    CancelledBeforeAdmission,
    #[error("reentrant use of the same Beryl-home writer is forbidden")]
    ReentrantWriter,
    #[error("the Beryl-home writer mutex is poisoned")]
    WriterPoisoned,
    #[error("proof composition could not confirm storage health: {source}")]
    StorageHealth {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("the Beryl-home generation lock is poisoned")]
    GenerationPoisoned,
    #[error("proof command belongs to another or obsolete Beryl-home generation")]
    StaleGeneration,
    #[error("proof role `{domain}` does not belong to this home generation")]
    ForeignDomain { domain: &'static str },
    #[error("registered domain `{domain}` no longer matches its persistent declaration")]
    DomainRegistrationInvariant { domain: &'static str },
    #[error("proof command conflicts with {conflicts_len} current revision(s)")]
    Conflict {
        conflicts_len: usize,
        conflicts: Vec<crate::RevisionConflict>,
    },
    #[error("proof role `{domain}` rejected its callback: {source}")]
    Callback {
        domain: &'static str,
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("proof role `{domain}` failed storage access: {source}")]
    CallbackAccess {
        domain: &'static str,
        #[source]
        source: DomainCallbackSource,
    },
    #[error("proof role `{domain}` disagreed with the source correlation")]
    Disagreement { domain: &'static str },
    #[error("proof role `{domain}` returned a correlation with the wrong fixed inline size")]
    CorrelationShape { domain: &'static str },
    #[error("proof revision snapshot failed: {source}")]
    RevisionRead {
        #[source]
        source: crate::ReadError,
    },
}

pub(crate) struct PreparedProofRole<'a, P: HomeProofProtocol> {
    pub(crate) plan: &'a ProofRolePlan<P>,
    pub(crate) domain: &'a RegisteredDomain,
    pub(crate) revision: DomainRevision,
}

pub(crate) fn prove<P: HomeProofProtocol>(
    role: &ProofRolePlan<P>,
    snapshot: &fjall::Snapshot,
    domain: &RegisteredDomain,
) -> Result<ProofCorrelation<P>, ErasedCallbackError> {
    role.callback.prove(snapshot, domain).and_then(|value| {
        ProofCorrelation::from_bytes(value).ok_or_else(|| {
            ErasedCallbackError::Rejected(Box::new(std::io::Error::other(
                "proof correlation has wrong inline size",
            )))
        })
    })
}

pub(crate) fn callback_error(
    domain: &'static str,
    source: ErasedCallbackError,
) -> ProofCompositionError {
    match source {
        ErasedCallbackError::Access(source) => {
            ProofCompositionError::CallbackAccess { domain, source }
        }
        ErasedCallbackError::Rejected(source) => ProofCompositionError::Callback { domain, source },
    }
}

pub(crate) fn failure_severity(error: &ProofCompositionError) -> Option<FailureSeverity> {
    match error {
        ProofCompositionError::CallbackAccess { source, .. } => callback_failure_severity(source),
        ProofCompositionError::GenerationPoisoned
        | ProofCompositionError::DomainRegistrationInvariant { .. }
        | ProofCompositionError::RevisionRead { .. } => Some(FailureSeverity::Structural),
        ProofCompositionError::HealthGate(_)
        | ProofCompositionError::CancelledBeforeAdmission
        | ProofCompositionError::ReentrantWriter
        | ProofCompositionError::WriterPoisoned
        | ProofCompositionError::StorageHealth { .. }
        | ProofCompositionError::StaleGeneration
        | ProofCompositionError::ForeignDomain { .. }
        | ProofCompositionError::Conflict { .. }
        | ProofCompositionError::Callback { .. }
        | ProofCompositionError::Disagreement { .. }
        | ProofCompositionError::CorrelationShape { .. } => None,
    }
}
