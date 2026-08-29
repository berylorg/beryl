use std::sync::atomic::{AtomicU64, Ordering};

use beryl_model::HomeRevision;
use thiserror::Error;

use crate::{CommandCancellation, HomeGeneration, domain::StoreInstanceId};

use super::{
    HomeProofProtocol, MAX_PROOF_CORRELATION_BYTES, MAX_PROOF_ROLES, ProofCorrelation,
    ProofReceiptConsumer, ProofSourceContribution, ProofWitnessContribution,
};

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProofCommandBuildError {
    #[error("proof role input does not bind the requested proof protocol")]
    ProtocolMismatch,
    #[error("proof command contains duplicate participation for domain {domain}")]
    DuplicateDomain { domain: &'static str },
    #[error("proof command has more than the fixed {limit} role limit")]
    RoleLimit { limit: usize },
    #[error(
        "proof protocol correlation is {actual} bytes, but declared {declared} bytes and must not exceed the fixed {limit}-byte limit"
    )]
    CorrelationSize {
        actual: usize,
        declared: usize,
        limit: usize,
    },
    #[error(
        "proof source expectation is {actual} bytes, but the requested protocol requires {expected} bytes"
    )]
    ExpectedCorrelationShape { actual: usize, expected: usize },
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ProofCommandSealError {
    #[error("the process-local proof command identity space is exhausted")]
    IdentityExhausted,
}

pub struct HomeProofCommand<P: HomeProofProtocol> {
    pub(crate) expected_generation: HomeGeneration,
    pub(crate) expected_home_revision: HomeRevision,
    pub(crate) cancellation: CommandCancellation,
    pub(crate) source: ProofSourceContribution<P>,
    pub(crate) witnesses: Vec<ProofWitnessContribution<P>>,
}

pub struct ExecutableHomeProofCommand<P: HomeProofProtocol> {
    pub(crate) command: HomeProofCommand<P>,
    pub(crate) command_id: u64,
}

struct ProofCommandIdentityAllocator {
    next: AtomicU64,
}

impl ProofCommandIdentityAllocator {
    const fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
        }
    }

    fn allocate(&self) -> Result<u64, ProofCommandSealError> {
        self.next
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                (current != 0).then_some(current.checked_add(1).unwrap_or(0))
            })
            .map_err(|_| ProofCommandSealError::IdentityExhausted)
    }
}

static NEXT_PROOF_COMMAND_ID: ProofCommandIdentityAllocator = ProofCommandIdentityAllocator::new();

#[cfg(feature = "test-faults")]
pub struct ProofCommandIdentityTestHarness {
    allocator: ProofCommandIdentityAllocator,
}

#[cfg(feature = "test-faults")]
impl ProofCommandIdentityTestHarness {
    #[must_use]
    pub fn at_exhaustion_boundary() -> Self {
        Self {
            allocator: ProofCommandIdentityAllocator {
                next: AtomicU64::new(u64::MAX),
            },
        }
    }

    pub fn allocate(&self) -> Result<u64, ProofCommandSealError> {
        self.allocator.allocate()
    }
}

impl<P: HomeProofProtocol> HomeProofCommand<P> {
    pub fn new(
        expected_generation: HomeGeneration,
        expected_home_revision: HomeRevision,
        source: ProofSourceContribution<P>,
    ) -> Result<Self, ProofCommandBuildError> {
        validate_correlation_size::<P>()?;
        if !source.plan.protocol.matches::<P>() {
            return Err(ProofCommandBuildError::ProtocolMismatch);
        }
        if ProofCorrelation::<P>::from_bytes(source.expected_correlation).is_none() {
            return Err(ProofCommandBuildError::ExpectedCorrelationShape {
                actual: usize::from(source.expected_correlation.len),
                expected: P::CORRELATION_BYTES,
            });
        }
        Ok(Self {
            expected_generation,
            expected_home_revision,
            cancellation: CommandCancellation::new(),
            source,
            witnesses: Vec::with_capacity(MAX_PROOF_ROLES - 1),
        })
    }

    #[must_use]
    pub fn with_cancellation(mut self, cancellation: CommandCancellation) -> Self {
        self.cancellation = cancellation;
        self
    }

    pub fn add_witness(
        &mut self,
        witness: ProofWitnessContribution<P>,
    ) -> Result<&mut Self, ProofCommandBuildError> {
        if !witness.plan.protocol.matches::<P>() {
            return Err(ProofCommandBuildError::ProtocolMismatch);
        }
        if self.role_count() == MAX_PROOF_ROLES {
            return Err(ProofCommandBuildError::RoleLimit {
                limit: MAX_PROOF_ROLES,
            });
        }
        if self.contains_domain(witness.plan.store, witness.plan.slot) {
            return Err(ProofCommandBuildError::DuplicateDomain {
                domain: witness.plan.domain,
            });
        }
        self.witnesses.push(witness);
        Ok(self)
    }

    pub fn seal(
        self,
    ) -> Result<(ExecutableHomeProofCommand<P>, ProofReceiptConsumer<P>), ProofCommandSealError>
    {
        let command_id = NEXT_PROOF_COMMAND_ID.allocate()?;
        let consumer = ProofReceiptConsumer::from_command(&self, command_id);
        Ok((
            ExecutableHomeProofCommand {
                command: self,
                command_id,
            },
            consumer,
        ))
    }

    fn role_count(&self) -> usize {
        self.witnesses.len() + 1
    }

    fn contains_domain(&self, store: StoreInstanceId, slot: usize) -> bool {
        (self.source.plan.store == store && self.source.plan.slot == slot)
            || self
                .witnesses
                .iter()
                .any(|witness| witness.plan.store == store && witness.plan.slot == slot)
    }
}

fn validate_correlation_size<P: HomeProofProtocol>() -> Result<(), ProofCommandBuildError> {
    let actual = std::mem::size_of::<P::Correlation>();
    if P::CORRELATION_BYTES != actual || actual > MAX_PROOF_CORRELATION_BYTES {
        return Err(ProofCommandBuildError::CorrelationSize {
            actual,
            declared: P::CORRELATION_BYTES,
            limit: MAX_PROOF_CORRELATION_BYTES,
        });
    }
    Ok(())
}
