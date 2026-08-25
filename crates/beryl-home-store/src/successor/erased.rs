use std::{
    any::{Any, TypeId},
    marker::PhantomData,
    mem,
};

use crate::{
    MutationBuildError, ReconciliationReader, StorageDomain,
    command::MaterializedDomainDescriptor,
    domain::{DomainOwnerId, RegisteredDomain},
};

use super::{
    SUCCESSOR_PROTOCOL_FIXED_BYTES, SUCCESSOR_ROLE_FIXED_BYTES, SuccessorCorrelation,
    SuccessorObservation, SuccessorProtocol, SuccessorSource, SuccessorWitness,
    reader::{ReservedSuccessorRead, SuccessorPointReader, SuccessorReadReservation},
};

#[derive(Clone, Copy)]
pub(crate) struct SuccessorProtocolIdentity {
    pub(crate) protocol_type: TypeId,
    pub(crate) correlation_type: TypeId,
    pub(crate) name: &'static str,
    pub(crate) correlation_bytes: usize,
}

impl SuccessorProtocolIdentity {
    pub(crate) fn matches(self, other: Self) -> bool {
        self.protocol_type == other.protocol_type
            && self.correlation_type == other.correlation_type
            && self.name == other.name
            && self.correlation_bytes == other.correlation_bytes
    }
}

pub(crate) enum SuccessorRoleReservation {
    Source(ErasedSuccessorSource),
    Witness(ErasedSuccessorWitness),
}

impl SuccessorRoleReservation {
    pub(crate) fn identity(&self) -> SuccessorProtocolIdentity {
        match self {
            Self::Source(source) => source.identity,
            Self::Witness(witness) => witness.identity,
        }
    }

    pub(crate) fn is_source(&self) -> bool {
        matches!(self, Self::Source(_))
    }
}

pub(crate) struct ErasedSuccessorSource {
    pub(super) identity: SuccessorProtocolIdentity,
    pub(super) domain: &'static str,
    pub(super) owner: DomainOwnerId,
    pub(super) state: Box<dyn Any + Send + Sync>,
    pub(super) authenticate: SourceAuthenticator,
}

pub(crate) struct ErasedSuccessorWitness {
    pub(super) identity: SuccessorProtocolIdentity,
    pub(super) domain: &'static str,
    pub(super) owner: DomainOwnerId,
    pub(super) state: Box<dyn Any + Send + Sync>,
    pub(super) reads: Vec<ReservedSuccessorRead>,
    pub(super) authenticate: WitnessAuthenticator,
}

type SourceAuthenticator =
    fn(
        &dyn Any,
        &fjall::Snapshot,
        &RegisteredDomain,
        &MaterializedDomainDescriptor,
    ) -> Result<ErasedObservation, crate::domain::callback::ErasedCallbackError>;

type WitnessAuthenticator =
    fn(
        &dyn Any,
        &dyn Any,
        &fjall::Snapshot,
        &RegisteredDomain,
        &[ReservedSuccessorRead],
    ) -> Result<WitnessExecution, crate::domain::callback::ErasedCallbackError>;

pub(crate) struct SuccessorDescriptor {
    pub(crate) identity: SuccessorProtocolIdentity,
    pub(crate) roles: Vec<SuccessorRoleDescriptor>,
}

pub(crate) struct SuccessorRoleDescriptor {
    pub(crate) domain_slot: usize,
    pub(crate) role: SuccessorRoleReservation,
}

pub(super) enum ErasedObservation {
    Authenticated {
        correlation: Box<dyn Any + Send + Sync>,
        encoded: Box<[u8]>,
    },
    Unresolved,
    Collision,
}

pub(super) struct WitnessExecution {
    pub(super) observation: ErasedWitnessObservation,
    pub(super) rejected: bool,
    pub(super) facts: Vec<DerivedReadFact>,
}

pub(super) enum ErasedWitnessObservation {
    Authenticated { agrees: bool, encoded: Box<[u8]> },
    Unresolved,
    Collision,
}

#[derive(Clone)]
pub(crate) struct DerivedReadFact {
    pub(crate) _family_slot: usize,
    pub(crate) _key_digest: [u8; 32],
    pub(crate) _current_digest: Option<[u8; 32]>,
    pub(crate) _expected_digest: [u8; 32],
}

pub(crate) struct SuccessorExecution {
    pub(crate) identity: SuccessorProtocolIdentity,
    pub(crate) resolved: bool,
    pub(crate) roles: Vec<SuccessorRoleFact>,
    pub(crate) correlation_digest: Option<[u8; 32]>,
}

pub(crate) struct SuccessorRoleFact {
    pub(crate) domain_slot: usize,
    pub(crate) kind: SuccessorRoleKind,
    pub(crate) result: SuccessorRoleResult,
    pub(crate) correlation_digest: Option<[u8; 32]>,
    pub(crate) derived: Vec<DerivedReadFact>,
}

#[derive(Clone, Copy)]
pub(crate) enum SuccessorRoleKind {
    Source,
    Witness,
}

#[derive(Clone, Copy)]
pub(crate) enum SuccessorRoleResult {
    Authenticated,
    Unresolved,
    Collision,
    Mismatch,
    Missing,
}

pub(crate) fn reserve_source<D, P, S>(
    source: S,
) -> Result<(SuccessorRoleReservation, usize), MutationBuildError>
where
    D: StorageDomain,
    P: SuccessorProtocol,
    S: SuccessorSource<D, P>,
{
    validate_protocol::<D, P>()?;
    if mem::size_of::<S>() > S::MAX_RETAINED_BYTES {
        return Err(MutationBuildError::SuccessorRetainedStateTooLarge {
            domain: D::NAME,
            actual: mem::size_of::<S>(),
            maximum: S::MAX_RETAINED_BYTES,
        });
    }
    let correlation_bytes = P::Correlation::ENCODED_BYTES
        .checked_mul(4)
        .ok_or(MutationBuildError::SuccessorReservationOverflow { domain: D::NAME })?;
    let bytes = SUCCESSOR_PROTOCOL_FIXED_BYTES
        .checked_add(SUCCESSOR_ROLE_FIXED_BYTES)
        .and_then(|bytes| bytes.checked_add(correlation_bytes))
        .and_then(|bytes| bytes.checked_add(S::MAX_RETAINED_BYTES))
        .ok_or(MutationBuildError::SuccessorReservationOverflow { domain: D::NAME })?;
    Ok((
        SuccessorRoleReservation::Source(ErasedSuccessorSource {
            identity: identity::<P>(),
            domain: D::NAME,
            owner: DomainOwnerId::of::<D>(),
            state: Box::new(source),
            authenticate: authenticate_source::<D, P, S>,
        }),
        bytes,
    ))
}

pub(crate) fn reserve_witness<D, P, W>(
    witness: W,
) -> Result<(SuccessorRoleReservation, usize), MutationBuildError>
where
    D: StorageDomain,
    P: SuccessorProtocol,
    W: SuccessorWitness<D, P>,
{
    validate_protocol::<D, P>()?;
    if mem::size_of::<W>() > W::MAX_RETAINED_BYTES {
        return Err(MutationBuildError::SuccessorRetainedStateTooLarge {
            domain: D::NAME,
            actual: mem::size_of::<W>(),
            maximum: W::MAX_RETAINED_BYTES,
        });
    }
    let mut reservation = SuccessorReadReservation::<D, P>::new();
    witness.reserve_reads(&mut reservation)?;
    if reservation.reads.is_empty() {
        return Err(MutationBuildError::MissingSuccessorReadReservation { domain: D::NAME });
    }
    let bytes = SUCCESSOR_ROLE_FIXED_BYTES
        .checked_add(W::MAX_RETAINED_BYTES)
        .and_then(|bytes| bytes.checked_add(reservation.descriptor_bytes))
        .ok_or(MutationBuildError::SuccessorReservationOverflow { domain: D::NAME })?;
    Ok((
        SuccessorRoleReservation::Witness(ErasedSuccessorWitness {
            identity: identity::<P>(),
            domain: D::NAME,
            owner: DomainOwnerId::of::<D>(),
            state: Box::new(witness),
            reads: reservation.reads,
            authenticate: authenticate_witness::<D, P, W>,
        }),
        bytes,
    ))
}

fn validate_protocol<D, P>() -> Result<(), MutationBuildError>
where
    D: StorageDomain,
    P: SuccessorProtocol,
{
    if P::NAME.is_empty()
        || P::NAME.len() > 64
        || !P::NAME
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || P::Correlation::ENCODED_BYTES == 0
        || mem::size_of::<P::Correlation>() > P::Correlation::ENCODED_BYTES
    {
        return Err(MutationBuildError::InvalidSuccessorProtocol { domain: D::NAME });
    }
    Ok(())
}

fn identity<P: SuccessorProtocol>() -> SuccessorProtocolIdentity {
    SuccessorProtocolIdentity {
        protocol_type: TypeId::of::<P>(),
        correlation_type: TypeId::of::<P::Correlation>(),
        name: P::NAME,
        correlation_bytes: P::Correlation::ENCODED_BYTES,
    }
}

fn authenticate_source<D, P, S>(
    state: &dyn Any,
    snapshot: &fjall::Snapshot,
    domain: &RegisteredDomain,
    descriptor: &MaterializedDomainDescriptor,
) -> Result<ErasedObservation, crate::domain::callback::ErasedCallbackError>
where
    D: StorageDomain,
    P: SuccessorProtocol,
    S: SuccessorSource<D, P>,
{
    let source = state
        .downcast_ref::<S>()
        .expect("successor source state retains its exact registered type");
    let observation = source
        .authenticate(&ReconciliationReader::new(snapshot, domain, descriptor))
        .map_err(crate::domain::callback::ErasedCallbackError::from_typed)?;
    Ok(erase_observation::<P>(observation))
}

fn authenticate_witness<D, P, W>(
    state: &dyn Any,
    correlation: &dyn Any,
    snapshot: &fjall::Snapshot,
    domain: &RegisteredDomain,
    reads: &[ReservedSuccessorRead],
) -> Result<WitnessExecution, crate::domain::callback::ErasedCallbackError>
where
    D: StorageDomain,
    P: SuccessorProtocol,
    W: SuccessorWitness<D, P>,
{
    let witness = state
        .downcast_ref::<W>()
        .expect("successor witness state retains its exact registered type");
    let correlation = correlation
        .downcast_ref::<P::Correlation>()
        .expect("successor correlation retains its exact registered type");
    let mut reader = SuccessorPointReader::<D, P> {
        snapshot,
        domain,
        correlation,
        reads,
        used: vec![0; reads.len()],
        rejected: false,
        facts: Vec::new(),
        _typed: PhantomData,
    };
    let observation = witness
        .authenticate(&mut reader)
        .map_err(crate::domain::callback::ErasedCallbackError::from_typed)?;
    let consumed = reader.used.iter().any(|count| *count != 0);
    let observation = match observation {
        SuccessorObservation::Authenticated(observed) => {
            let agrees = observed == *correlation;
            let mut encoded = vec![0; P::Correlation::ENCODED_BYTES];
            observed.encode(&mut encoded);
            ErasedWitnessObservation::Authenticated {
                agrees,
                encoded: encoded.into_boxed_slice(),
            }
        }
        SuccessorObservation::Unresolved => ErasedWitnessObservation::Unresolved,
        SuccessorObservation::Collision => ErasedWitnessObservation::Collision,
    };
    Ok(WitnessExecution {
        observation,
        rejected: reader.rejected || !consumed,
        facts: reader.facts,
    })
}

fn erase_observation<P: SuccessorProtocol>(
    observation: SuccessorObservation<P::Correlation>,
) -> ErasedObservation {
    match observation {
        SuccessorObservation::Authenticated(correlation) => {
            let mut encoded = vec![0; P::Correlation::ENCODED_BYTES];
            correlation.encode(&mut encoded);
            ErasedObservation::Authenticated {
                correlation: Box::new(correlation),
                encoded: encoded.into_boxed_slice(),
            }
        }
        SuccessorObservation::Unresolved => ErasedObservation::Unresolved,
        SuccessorObservation::Collision => ErasedObservation::Collision,
    }
}
