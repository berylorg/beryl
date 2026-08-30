mod support;

use std::{
    convert::Infallible,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use std::time::Duration;

#[cfg(feature = "test-faults")]
use std::sync::mpsc;

use beryl_home_store::{
    CommandCancellation, DomainHandle, DomainReader, DomainSchemaVersion,
    ExecutableHomeProofCommand, HomeCommand, HomeProofCommand, HomeProofProtocol, HomeStore,
    KeyspaceSchemaVersion, MAX_PROOF_CORRELATION_BYTES, MAX_PROOF_ROLES, PointReadLimit,
    ProofCommandBuildError, ProofCompositionError, ProofCorrelationBytes, ProofDomain,
    ProofProtocolIdentity, ProofReceiptError, StorageDomain,
};
use tempfile::tempdir;

use support::{AlphaDomain, BetaDomain, FixtureMutationError, PutBytes, committed, open_home};

#[cfg(feature = "test-faults")]
use beryl_home_store::{
    HomeHealthState, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};

macro_rules! empty_domain {
    ($domain:ident, $name:literal) => {
        struct $domain;

        impl StorageDomain for $domain {
            const NAME: &'static str = $name;
            const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
            const FAMILIES: &'static [beryl_home_store::RecordFamily<Self>] =
                &[beryl_home_store::RecordFamily::new::<
                    support::BytesRecord<Self>,
                >(KeyspaceSchemaVersion::new(1))];
            type ValidationError = Infallible;
            type RuntimeAttachment = ();
            type RuntimeAttachmentError = Infallible;

            fn create_runtime_attachment(
                _reader: &beryl_home_store::DomainRegistrationReader<'_, Self>,
            ) -> Result<(), Self::RuntimeAttachmentError> {
                Ok(())
            }

            fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
                Ok(())
            }
        }
    };
}

empty_domain!(RoleDomain1, "proof_role_1");
empty_domain!(RoleDomain2, "proof_role_2");
empty_domain!(RoleDomain3, "proof_role_3");
empty_domain!(RoleDomain4, "proof_role_4");
empty_domain!(RoleDomain5, "proof_role_5");
empty_domain!(RoleDomain6, "proof_role_6");
empty_domain!(RoleDomain7, "proof_role_7");
empty_domain!(OversizedDomain, "proof_oversized");
empty_domain!(MalformedExpectationDomain, "proof_malformed_expectation");
empty_domain!(CorrelatedSourceDomain, "proof_correlated_source");
empty_domain!(CorrelatedWitnessDomain, "proof_correlated_witness");

pub struct AgreementProtocol;

impl HomeProofProtocol for AgreementProtocol {
    type Correlation = [u8; 16];

    const PROTOCOL_ID: u64 = 0x706f6f66;
    const OPERATION_ID: u64 = 0x7631;
    const CORRELATION_BYTES: usize = 16;
}

pub struct OversizedProtocol;

impl HomeProofProtocol for OversizedProtocol {
    type Correlation = [u8; MAX_PROOF_CORRELATION_BYTES];

    const PROTOCOL_ID: u64 = 1;
    const OPERATION_ID: u64 = 2;
    const CORRELATION_BYTES: usize = MAX_PROOF_CORRELATION_BYTES + 1;
}

pub struct StoredValueProtocol;

impl HomeProofProtocol for StoredValueProtocol {
    type Correlation = [u8; 16];

    const PROTOCOL_ID: u64 = 0x706f6f67;
    const OPERATION_ID: u64 = 0x7631;
    const CORRELATION_BYTES: usize = 16;
}

#[derive(Clone)]
struct CallbackBlock {
    state: Arc<(Mutex<(bool, bool)>, Condvar)>,
}

impl CallbackBlock {
    fn new() -> Self {
        Self {
            state: Arc::new((Mutex::new((false, false)), Condvar::new())),
        }
    }

    fn wait_until_reached(&self, timeout: Duration) -> bool {
        let (state, changed) = &*self.state;
        let guard = state.lock().unwrap();
        let (guard, _) = changed
            .wait_timeout_while(guard, timeout, |status| !status.0)
            .unwrap();
        guard.0
    }

    fn release(&self) {
        let (state, changed) = &*self.state;
        state.lock().unwrap().1 = true;
        changed.notify_all();
    }

    fn reach_and_wait(&self) {
        let (state, changed) = &*self.state;
        let mut status = state.lock().unwrap();
        status.0 = true;
        changed.notify_all();
        while !status.1 {
            status = changed.wait(status).unwrap();
        }
    }
}

struct StoredValueRole {
    expected: [u8; 16],
    block: Option<CallbackBlock>,
}

impl StoredValueRole {
    fn plain(expected: [u8; 16]) -> Self {
        Self {
            expected,
            block: None,
        }
    }

    fn blocked(expected: [u8; 16], block: CallbackBlock) -> Self {
        Self {
            expected,
            block: Some(block),
        }
    }
}

fn prove_stored_value_role<D: StorageDomain>(
    input: &StoredValueRole,
    reader: &DomainReader<'_, D>,
) -> Result<ProofCorrelationBytes, Infallible> {
    let value = reader
        .point::<support::BytesRecord<D>>(&1, PointReadLimit::new(32).unwrap())
        .unwrap()
        .unwrap();
    if let Some(block) = &input.block {
        block.reach_and_wait();
    }
    let correlation: [u8; 16] = value.try_into().unwrap();
    Ok(ProofCorrelationBytes::new(correlation))
}

macro_rules! stored_value_proof_domain {
    ($domain:ty) => {
        impl ProofDomain for $domain {
            type SourceInput = StoredValueRole;
            type WitnessInput = StoredValueRole;
            type Error = Infallible;

            fn source_protocol(_input: &Self::SourceInput) -> ProofProtocolIdentity {
                ProofProtocolIdentity::of::<StoredValueProtocol>()
            }

            fn expected_source_correlation(input: &Self::SourceInput) -> ProofCorrelationBytes {
                ProofCorrelationBytes::new(input.expected)
            }

            fn witness_protocol(_input: &Self::WitnessInput) -> ProofProtocolIdentity {
                ProofProtocolIdentity::of::<StoredValueProtocol>()
            }

            fn prove_source(
                input: &Self::SourceInput,
                reader: &DomainReader<'_, Self>,
            ) -> Result<ProofCorrelationBytes, Self::Error> {
                prove_stored_value_role(input, reader)
            }

            fn prove_witness(
                input: &Self::WitnessInput,
                reader: &DomainReader<'_, Self>,
            ) -> Result<ProofCorrelationBytes, Self::Error> {
                prove_stored_value_role(input, reader)
            }
        }
    };
}

stored_value_proof_domain!(CorrelatedSourceDomain);
stored_value_proof_domain!(CorrelatedWitnessDomain);

pub struct Role<P> {
    correlation: P,
    reject: bool,
    called: Option<Arc<AtomicBool>>,
    cancellation: Option<CommandCancellation>,
    reentry: Option<ProofReentry>,
}

struct ProofReentry {
    store: Arc<HomeStore>,
    command: Mutex<Option<ExecutableHomeProofCommand<AgreementProtocol>>>,
    result: Arc<Mutex<Option<Result<(), ProofCompositionError>>>>,
}

impl<P> Role<P> {
    fn agreeing(correlation: P) -> Self {
        Self {
            correlation,
            reject: false,
            called: None,
            cancellation: None,
            reentry: None,
        }
    }

    fn rejecting(correlation: P) -> Self {
        Self {
            correlation,
            reject: true,
            called: None,
            cancellation: None,
            reentry: None,
        }
    }

    fn tracking(mut self, called: Arc<AtomicBool>) -> Self {
        self.called = Some(called);
        self
    }

    fn cancelling(mut self, cancellation: CommandCancellation) -> Self {
        self.cancellation = Some(cancellation);
        self
    }

    fn reentering(mut self, reentry: ProofReentry) -> Self {
        self.reentry = Some(reentry);
        self
    }
}

fn prove_agreement_role(
    role: &Role<[u8; 16]>,
) -> Result<ProofCorrelationBytes, FixtureMutationError> {
    if let Some(called) = &role.called {
        called.store(true, Ordering::SeqCst);
    }
    if let Some(cancellation) = &role.cancellation {
        cancellation.cancel();
    }
    if let Some(reentry) = &role.reentry {
        let command = reentry
            .command
            .lock()
            .unwrap()
            .take()
            .expect("proof reentry fixture command is consumed once");
        let result = reentry.store.compose_proof(command).map(|_| ());
        *reentry.result.lock().unwrap() = Some(result);
    }
    if role.reject {
        return Err(FixtureMutationError::Rejected("proof role rejected"));
    }
    Ok(ProofCorrelationBytes::new(role.correlation))
}

impl ProofDomain for AlphaDomain {
    type SourceInput = Role<[u8; 16]>;
    type WitnessInput = Role<[u8; 16]>;
    type Error = FixtureMutationError;

    fn source_protocol(_input: &Self::SourceInput) -> ProofProtocolIdentity {
        ProofProtocolIdentity::of::<AgreementProtocol>()
    }

    fn expected_source_correlation(input: &Self::SourceInput) -> ProofCorrelationBytes {
        ProofCorrelationBytes::new(input.correlation)
    }

    fn witness_protocol(_input: &Self::WitnessInput) -> ProofProtocolIdentity {
        ProofProtocolIdentity::of::<AgreementProtocol>()
    }

    fn prove_source(
        input: &Self::SourceInput,
        _reader: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Self::Error> {
        prove_agreement_role(input)
    }
    fn prove_witness(
        input: &Self::WitnessInput,
        _reader: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Self::Error> {
        prove_agreement_role(input)
    }
}

macro_rules! agreement_proof_domain {
    ($domain:ty) => {
        impl ProofDomain for $domain {
            type SourceInput = Role<[u8; 16]>;
            type WitnessInput = Role<[u8; 16]>;
            type Error = FixtureMutationError;

            fn source_protocol(_input: &Self::SourceInput) -> ProofProtocolIdentity {
                ProofProtocolIdentity::of::<AgreementProtocol>()
            }

            fn expected_source_correlation(input: &Self::SourceInput) -> ProofCorrelationBytes {
                ProofCorrelationBytes::new(input.correlation)
            }

            fn witness_protocol(_input: &Self::WitnessInput) -> ProofProtocolIdentity {
                ProofProtocolIdentity::of::<AgreementProtocol>()
            }

            fn prove_source(
                input: &Self::SourceInput,
                _reader: &DomainReader<'_, Self>,
            ) -> Result<ProofCorrelationBytes, Self::Error> {
                prove_agreement_role(input)
            }

            fn prove_witness(
                input: &Self::WitnessInput,
                _reader: &DomainReader<'_, Self>,
            ) -> Result<ProofCorrelationBytes, Self::Error> {
                prove_agreement_role(input)
            }
        }
    };
}

agreement_proof_domain!(BetaDomain);
agreement_proof_domain!(RoleDomain1);
agreement_proof_domain!(RoleDomain2);
agreement_proof_domain!(RoleDomain3);
agreement_proof_domain!(RoleDomain4);
agreement_proof_domain!(RoleDomain5);
agreement_proof_domain!(RoleDomain6);
agreement_proof_domain!(RoleDomain7);

impl ProofDomain for OversizedDomain {
    type SourceInput = Role<[u8; MAX_PROOF_CORRELATION_BYTES]>;
    type WitnessInput = Role<[u8; MAX_PROOF_CORRELATION_BYTES]>;
    type Error = FixtureMutationError;

    fn source_protocol(_input: &Self::SourceInput) -> ProofProtocolIdentity {
        ProofProtocolIdentity::of::<OversizedProtocol>()
    }

    fn expected_source_correlation(input: &Self::SourceInput) -> ProofCorrelationBytes {
        ProofCorrelationBytes::new(input.correlation)
    }

    fn witness_protocol(_input: &Self::WitnessInput) -> ProofProtocolIdentity {
        ProofProtocolIdentity::of::<OversizedProtocol>()
    }

    fn prove_source(
        input: &Self::SourceInput,
        _reader: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Self::Error> {
        Ok(ProofCorrelationBytes::new(input.correlation))
    }

    fn prove_witness(
        input: &Self::WitnessInput,
        _reader: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Self::Error> {
        Ok(ProofCorrelationBytes::new(input.correlation))
    }
}

impl ProofDomain for MalformedExpectationDomain {
    type SourceInput = Role<[u8; 16]>;
    type WitnessInput = Role<[u8; 16]>;
    type Error = FixtureMutationError;

    fn source_protocol(_input: &Self::SourceInput) -> ProofProtocolIdentity {
        ProofProtocolIdentity::of::<AgreementProtocol>()
    }

    fn expected_source_correlation(_input: &Self::SourceInput) -> ProofCorrelationBytes {
        ProofCorrelationBytes::new([0; 8])
    }

    fn witness_protocol(_input: &Self::WitnessInput) -> ProofProtocolIdentity {
        ProofProtocolIdentity::of::<AgreementProtocol>()
    }

    fn prove_source(
        input: &Self::SourceInput,
        _reader: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Self::Error> {
        prove_agreement_role(input)
    }

    fn prove_witness(
        input: &Self::WitnessInput,
        _reader: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Self::Error> {
        prove_agreement_role(input)
    }
}

fn generation(store: &HomeStore) -> beryl_home_store::HomeGeneration {
    store.health().generation().unwrap()
}

fn command(
    store: &HomeStore,
    alpha: &DomainHandle<AlphaDomain>,
    source: Role<[u8; 16]>,
) -> HomeProofCommand<AgreementProtocol> {
    HomeProofCommand::new(
        generation(store),
        store.home_revision().unwrap(),
        alpha.proof_source::<AgreementProtocol>(store.domain_revision(alpha).unwrap(), source),
    )
    .unwrap()
}

fn compose<P: HomeProofProtocol>(
    store: &HomeStore,
    command: HomeProofCommand<P>,
) -> Result<
    (
        beryl_home_store::HomeProofReceipt<P>,
        beryl_home_store::ProofReceiptConsumer<P>,
    ),
    ProofCompositionError,
> {
    let (command, consumer) = command.seal().unwrap();
    store
        .compose_proof(command)
        .map(|receipt| (receipt, consumer))
}

#[cfg(feature = "test-faults")]
fn open_with_faults(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

#[test]
fn source_only_and_multi_domain_agreement_leave_durable_and_reconciliation_state_unchanged() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();
    let role1 = store.register_domain::<RoleDomain1>().unwrap();
    let role2 = store.register_domain::<RoleDomain2>().unwrap();
    let home_before = store.home_revision().unwrap();
    let alpha_before = store.domain_revision(&alpha).unwrap();
    let beta_before = store.domain_revision(&beta).unwrap();
    let role1_before = store.domain_revision(&role1).unwrap();
    let role2_before = store.domain_revision(&role2).unwrap();
    let correlation = [7; 16];

    let source = alpha.proof_source::<AgreementProtocol>(alpha_before, Role::agreeing(correlation));
    let (receipt, source_consumer) = compose(
        &store,
        HomeProofCommand::new(generation(&store), home_before, source).unwrap(),
    )
    .unwrap();
    store
        .consume_proof_receipt(source_consumer, receipt)
        .unwrap();

    let source = alpha.proof_source::<AgreementProtocol>(alpha_before, Role::agreeing(correlation));
    let mut multi = HomeProofCommand::new(generation(&store), home_before, source).unwrap();
    multi
        .add_witness(beta.proof_witness(beta_before, Role::agreeing(correlation)))
        .unwrap()
        .add_witness(role1.proof_witness(role1_before, Role::agreeing(correlation)))
        .unwrap()
        .add_witness(role2.proof_witness(role2_before, Role::agreeing(correlation)))
        .unwrap();
    let (receipt, multi_consumer) = compose(&store, multi).unwrap();
    store
        .consume_proof_receipt(multi_consumer, receipt)
        .unwrap();
    assert_eq!(store.home_revision().unwrap(), home_before);
    assert_eq!(store.domain_revision(&alpha).unwrap(), alpha_before);
    assert_eq!(store.domain_revision(&beta).unwrap(), beta_before);
    assert_eq!(store.domain_revision(&role1).unwrap(), role1_before);
    assert_eq!(store.domain_revision(&role2).unwrap(), role2_before);
    assert!(store.pending_reconciliations().is_empty());
    let source = alpha.proof_source::<AgreementProtocol>(alpha_before, Role::agreeing(correlation));
    let (stale_receipt, stale_consumer) = compose(
        &store,
        HomeProofCommand::new(generation(&store), home_before, source).unwrap(),
    )
    .unwrap();
    let stale_source =
        alpha.proof_source::<AgreementProtocol>(alpha_before, Role::agreeing(correlation));
    let (stale_executable, _stale_consumer) =
        HomeProofCommand::new(generation(&store), home_before, stale_source)
            .unwrap()
            .seal()
            .unwrap();
    store.close().unwrap();
    let mut reopened = open_home(directory.path());
    reopened.register_domain::<AlphaDomain>().unwrap();
    assert!(matches!(
        reopened.consume_proof_receipt(stale_consumer, stale_receipt),
        Err(ProofReceiptError::StaleOrForeign)
    ));
    assert!(matches!(
        reopened.compose_proof(stale_executable),
        Err(ProofCompositionError::ForeignDomain { domain: "alpha" })
    ));
}

#[test]
fn source_consumer_rejects_same_fence_cross_page_receipts_and_consumes_the_exact_receipt() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();
    let alpha_revision = store.domain_revision(&alpha).unwrap();
    let beta_revision = store.domain_revision(&beta).unwrap();
    let home = store.home_revision().unwrap();
    let generation = generation(&store);

    let mut alpha_source = HomeProofCommand::new(
        generation,
        home,
        alpha.proof_source::<AgreementProtocol>(alpha_revision, Role::agreeing([1; 16])),
    )
    .unwrap();
    alpha_source
        .add_witness(beta.proof_witness(beta_revision, Role::agreeing([1; 16])))
        .unwrap();
    let (alpha_receipt, alpha_consumer) = compose(&store, alpha_source).unwrap();
    let mut beta_source = HomeProofCommand::new(
        generation,
        home,
        beta.proof_source::<AgreementProtocol>(beta_revision, Role::agreeing([1; 16])),
    )
    .unwrap();
    beta_source
        .add_witness(alpha.proof_witness(alpha_revision, Role::agreeing([1; 16])))
        .unwrap();
    let (beta_receipt, beta_consumer) = compose(&store, beta_source).unwrap();
    assert!(matches!(
        store.consume_proof_receipt(alpha_consumer, beta_receipt),
        Err(ProofReceiptError::SourceFenceMismatch)
    ));
    assert!(matches!(
        store.consume_proof_receipt(beta_consumer, alpha_receipt),
        Err(ProofReceiptError::SourceFenceMismatch)
    ));

    let first_source =
        alpha.proof_source::<AgreementProtocol>(alpha_revision, Role::agreeing([2; 16]));
    let (first_receipt, first_consumer) = compose(
        &store,
        HomeProofCommand::new(generation, home, first_source).unwrap(),
    )
    .unwrap();
    let mut advance = HomeCommand::new(store.home_revision().unwrap());
    advance
        .add(beta.contribution(
            beta_revision,
            PutBytes::<BetaDomain>::new(10, b"advance home fence".to_vec()),
        ))
        .unwrap();
    committed(store.execute(advance));
    let second_source =
        alpha.proof_source::<AgreementProtocol>(alpha_revision, Role::agreeing([2; 16]));
    let (second_receipt, second_consumer) = compose(
        &store,
        HomeProofCommand::new(generation, store.home_revision().unwrap(), second_source).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        store.consume_proof_receipt(first_consumer, second_receipt),
        Err(ProofReceiptError::SourceFenceMismatch)
    ));
    assert!(matches!(
        store.consume_proof_receipt(second_consumer, first_receipt),
        Err(ProofReceiptError::SourceFenceMismatch)
    ));
    let third_source =
        alpha.proof_source::<AgreementProtocol>(alpha_revision, Role::agreeing([2; 16]));
    let (third_receipt, third_consumer) = compose(
        &store,
        HomeProofCommand::new(generation, store.home_revision().unwrap(), third_source).unwrap(),
    )
    .unwrap();
    store
        .consume_proof_receipt(third_consumer, third_receipt)
        .unwrap();

    let mismatch_source =
        alpha.proof_source::<AgreementProtocol>(alpha_revision, Role::agreeing([3; 16]));
    let (_mismatch_receipt, mismatch_consumer) = compose(
        &store,
        HomeProofCommand::new(generation, store.home_revision().unwrap(), mismatch_source).unwrap(),
    )
    .unwrap();
    let other_source =
        alpha.proof_source::<AgreementProtocol>(alpha_revision, Role::agreeing([4; 16]));
    let (other_receipt, _other_consumer) = compose(
        &store,
        HomeProofCommand::new(generation, store.home_revision().unwrap(), other_source).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        store.consume_proof_receipt(mismatch_consumer, other_receipt),
        Err(ProofReceiptError::SourceFenceMismatch)
    ));

    let command_a = HomeProofCommand::new(
        generation,
        store.home_revision().unwrap(),
        alpha.proof_source::<AgreementProtocol>(alpha_revision, Role::agreeing([5; 16])),
    )
    .unwrap();
    let command_b = HomeProofCommand::new(
        generation,
        store.home_revision().unwrap(),
        alpha.proof_source::<AgreementProtocol>(alpha_revision, Role::agreeing([5; 16])),
    )
    .unwrap();
    let (_executable_a, consumer_a) = command_a.seal().unwrap();
    let (executable_b, _consumer_b) = command_b.seal().unwrap();
    let receipt_b = store.compose_proof(executable_b).unwrap();
    assert!(matches!(
        store.consume_proof_receipt(consumer_a, receipt_b),
        Err(ProofReceiptError::SourceFenceMismatch)
    ));
    let command_a = HomeProofCommand::new(
        generation,
        store.home_revision().unwrap(),
        alpha.proof_source::<AgreementProtocol>(alpha_revision, Role::agreeing([5; 16])),
    )
    .unwrap();
    let (executable_a, consumer_a) = command_a.seal().unwrap();
    let receipt_a = store.compose_proof(executable_a).unwrap();
    store.consume_proof_receipt(consumer_a, receipt_a).unwrap();
}

#[test]
fn disagreement_callback_failure_and_stale_fences_are_determinate_and_nonmutating() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();
    let home_before = store.home_revision().unwrap();
    let alpha_before = store.domain_revision(&alpha).unwrap();
    let beta_before = store.domain_revision(&beta).unwrap();
    let mut disagreement = command(&store, &alpha, Role::agreeing([1; 16]));
    disagreement
        .add_witness(beta.proof_witness(beta_before, Role::agreeing([2; 16])))
        .unwrap();
    assert!(matches!(
        compose(&store, disagreement),
        Err(ProofCompositionError::Disagreement { domain: "beta" })
    ));

    assert!(matches!(
        compose(&store, command(&store, &alpha, Role::rejecting([1; 16]))),
        Err(ProofCompositionError::Callback {
            domain: "alpha",
            ..
        })
    ));

    let stale = HomeProofCommand::<AgreementProtocol>::new(
        generation(&store),
        beryl_model::HomeRevision::new(home_before.get() + 1).unwrap(),
        alpha.proof_source::<AgreementProtocol>(alpha_before, Role::agreeing([1; 16])),
    )
    .unwrap();
    assert!(matches!(
        compose(&store, stale),
        Err(ProofCompositionError::Conflict { .. })
    ));

    let mut advance_beta = HomeCommand::new(store.home_revision().unwrap());
    advance_beta
        .add(beta.contribution(
            beta_before,
            PutBytes::<BetaDomain>::new(9, b"advance beta".to_vec()),
        ))
        .unwrap();
    committed(store.execute(advance_beta));
    let home_after_advance = store.home_revision().unwrap();
    let beta_after_advance = store.domain_revision(&beta).unwrap();
    let mut stale_domain = command(&store, &alpha, Role::agreeing([1; 16]));
    stale_domain
        .add_witness(beta.proof_witness(beta_before, Role::agreeing([1; 16])))
        .unwrap();
    assert!(matches!(
        compose(&store, stale_domain),
        Err(ProofCompositionError::Conflict { .. })
    ));
    assert_eq!(store.home_revision().unwrap(), home_after_advance);
    assert_eq!(store.domain_revision(&alpha).unwrap(), alpha_before);
    assert_eq!(store.domain_revision(&beta).unwrap(), beta_after_advance);
    assert!(store.pending_reconciliations().is_empty());
}

#[test]
fn nested_independent_proofs_complete_without_writer_reentry() {
    let directory = tempdir().unwrap();
    let mut opened = open_home(directory.path());
    let alpha = opened.register_domain::<AlphaDomain>().unwrap();
    let store = Arc::new(opened);
    let (nested, _nested_consumer) = command(&store, &alpha, Role::agreeing([6; 16]))
        .seal()
        .unwrap();
    let observed = Arc::new(Mutex::new(None));
    let outer = command(
        &store,
        &alpha,
        Role::agreeing([6; 16]).reentering(ProofReentry {
            store: Arc::clone(&store),
            command: Mutex::new(Some(nested)),
            result: Arc::clone(&observed),
        }),
    );
    let (outer, consumer) = outer.seal().unwrap();
    let receipt = store.compose_proof(outer).unwrap();
    store.consume_proof_receipt(consumer, receipt).unwrap();
    assert!(matches!(observed.lock().unwrap().as_ref(), Some(Ok(()))));
}

#[cfg(feature = "test-faults")]
#[test]
fn proof_and_receipt_publish_across_unrelated_maintenance_terminal() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open_with_faults(directory.path(), faults);
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let proof = command(&store, &alpha, Role::agreeing([7; 16]));

    store.inject_retained_maintenance_terminal();

    let (receipt, consumer) = compose(&store, proof).unwrap();
    store.consume_proof_receipt(consumer, receipt).unwrap();
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
}

#[cfg(feature = "test-faults")]
#[test]
fn proof_completes_while_a_writer_remains_blocked() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut opened = open_with_faults(directory.path(), faults.clone());
    let alpha = opened.register_domain::<AlphaDomain>().unwrap();
    let expected_home = opened.home_revision().unwrap();
    let expected_domain = opened.domain_revision(&alpha).unwrap();
    let blocked = faults.block_next(FaultPoint::BeforeCommit);
    let store = Arc::new(opened);
    let writing = Arc::clone(&store);
    let writer_alpha = alpha.clone();
    let writer = std::thread::spawn(move || {
        let mut command = HomeCommand::new(expected_home);
        command
            .add(writer_alpha.contribution(
                expected_domain,
                PutBytes::<AlphaDomain>::new(1, b"blocked writer".to_vec()),
            ))
            .unwrap();
        writing.execute(command)
    });
    assert!(blocked.wait_until_reached(Duration::from_secs(10)));

    let proof = command(&store, &alpha, Role::agreeing([8; 16]));
    let (receipt, consumer) = compose(&store, proof).unwrap();
    store.consume_proof_receipt(consumer, receipt).unwrap();

    blocked.release();
    committed(writer.join().unwrap());
}

#[cfg(feature = "test-faults")]
fn put_correlated_pair(
    store: &HomeStore,
    source: &DomainHandle<CorrelatedSourceDomain>,
    witness: &DomainHandle<CorrelatedWitnessDomain>,
    value: [u8; 16],
) -> beryl_home_store::CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(source.contribution(
            store.domain_revision(source).unwrap(),
            PutBytes::<CorrelatedSourceDomain>::new(1, value.to_vec()),
        ))
        .unwrap();
    command
        .add(witness.contribution(
            store.domain_revision(witness).unwrap(),
            PutBytes::<CorrelatedWitnessDomain>::new(1, value.to_vec()),
        ))
        .unwrap();
    store.execute(command)
}

#[cfg(feature = "test-faults")]
#[test]
fn proof_snapshot_is_one_complete_old_or_new_cross_domain_state() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut opened = open_with_faults(directory.path(), faults);
    let source = opened.register_domain::<CorrelatedSourceDomain>().unwrap();
    let witness = opened.register_domain::<CorrelatedWitnessDomain>().unwrap();
    let old = [1; 16];
    let new = [2; 16];
    committed(put_correlated_pair(&opened, &source, &witness, old));

    let callback_block = CallbackBlock::new();
    let mut command = HomeProofCommand::new(
        generation(&opened),
        opened.home_revision().unwrap(),
        source.proof_source::<StoredValueProtocol>(
            opened.domain_revision(&source).unwrap(),
            StoredValueRole::blocked(old, callback_block.clone()),
        ),
    )
    .unwrap();
    command
        .add_witness(witness.proof_witness(
            opened.domain_revision(&witness).unwrap(),
            StoredValueRole::plain(old),
        ))
        .unwrap();
    let (command, consumer) = command.seal().unwrap();
    let store = Arc::new(opened);
    let proving = Arc::clone(&store);
    let proof = std::thread::spawn(move || proving.compose_proof(command));

    let snapshot_exists = callback_block.wait_until_reached(Duration::from_secs(10));
    let mutation = snapshot_exists.then(|| put_correlated_pair(&store, &source, &witness, new));
    callback_block.release();
    let racing = proof.join().unwrap();

    assert!(snapshot_exists);
    committed(mutation.unwrap());
    let receipt = racing.unwrap();
    store.consume_proof_receipt(consumer, receipt).unwrap();

    let mut command = HomeProofCommand::new(
        generation(&store),
        store.home_revision().unwrap(),
        source.proof_source::<StoredValueProtocol>(
            store.domain_revision(&source).unwrap(),
            StoredValueRole::plain(new),
        ),
    )
    .unwrap();
    command
        .add_witness(witness.proof_witness(
            store.domain_revision(&witness).unwrap(),
            StoredValueRole::plain(new),
        ))
        .unwrap();
    let (receipt, consumer) = compose(&store, command).unwrap();
    store.consume_proof_receipt(consumer, receipt).unwrap();
}

#[cfg(feature = "test-faults")]
#[test]
fn cancellation_after_waiting_for_health_admission_skips_proof_callbacks() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut opened = open_with_faults(directory.path(), faults.clone());
    let alpha = opened.register_domain::<AlphaDomain>().unwrap();
    let cancellation = CommandCancellation::new();
    let callbacks = Arc::new(AtomicBool::new(false));
    let command = command(
        &opened,
        &alpha,
        Role::agreeing([9; 16]).tracking(Arc::clone(&callbacks)),
    )
    .with_cancellation(cancellation.clone());
    let (command, _consumer) = command.seal().unwrap();
    let blocks = (0..64)
        .map(|_| faults.block_next(FaultPoint::BeforeReadConfirmation))
        .collect::<Vec<_>>();
    let store = Arc::new(opened);
    let mut readers = Vec::with_capacity(64);
    for _ in 0..64 {
        let reading = Arc::clone(&store);
        readers.push(
            std::thread::Builder::new()
                .stack_size(256 * 1024)
                .spawn(move || reading.home_revision())
                .unwrap(),
        );
    }
    let admissions_full = blocks
        .iter()
        .all(|block| block.wait_until_reached(Duration::from_secs(10)));

    let (started_sender, started_receiver) = mpsc::sync_channel(1);
    let (result_sender, result_receiver) = mpsc::sync_channel(1);
    let proving = Arc::clone(&store);
    let proof = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            started_sender.send(()).unwrap();
            result_sender.send(proving.compose_proof(command)).unwrap();
        })
        .unwrap();
    let proof_started = started_receiver
        .recv_timeout(Duration::from_secs(10))
        .is_ok();
    let early_result = proof_started
        .then(|| {
            result_receiver
                .recv_timeout(Duration::from_millis(100))
                .ok()
        })
        .flatten();
    let completed_early = early_result.is_some();

    cancellation.cancel();
    if admissions_full && proof_started && early_result.is_none() {
        blocks[0].release();
    } else {
        for block in &blocks {
            block.release();
        }
    }
    let result =
        early_result.or_else(|| result_receiver.recv_timeout(Duration::from_secs(10)).ok());
    for block in &blocks {
        block.release();
    }
    for reader in readers {
        reader.join().unwrap().unwrap();
    }
    proof.join().unwrap();

    assert!(admissions_full);
    assert!(proof_started);
    assert!(!completed_early);
    assert!(matches!(
        result,
        Some(Err(ProofCompositionError::CancelledBeforeAdmission))
    ));
    assert!(!callbacks.load(Ordering::SeqCst));
}

#[test]
fn duplicate_foreign_cancellation_and_bounds_reject_without_callbacks_or_reconciliation() {
    let directory = tempdir().unwrap();
    let foreign_directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let mut foreign = open_home(foreign_directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();
    let role1 = store.register_domain::<RoleDomain1>().unwrap();
    let role2 = store.register_domain::<RoleDomain2>().unwrap();
    let role3 = store.register_domain::<RoleDomain3>().unwrap();
    let role4 = store.register_domain::<RoleDomain4>().unwrap();
    let role5 = store.register_domain::<RoleDomain5>().unwrap();
    let role6 = store.register_domain::<RoleDomain6>().unwrap();
    let role7 = store.register_domain::<RoleDomain7>().unwrap();
    let foreign_alpha = foreign.register_domain::<AlphaDomain>().unwrap();
    let revision = store.domain_revision(&alpha).unwrap();

    let mut duplicate = command(&store, &alpha, Role::agreeing([3; 16]));
    assert!(matches!(
        duplicate.add_witness(alpha.proof_witness(revision, Role::agreeing([3; 16]))),
        Err(ProofCommandBuildError::DuplicateDomain { domain: "alpha" })
    ));

    let foreign_command = HomeProofCommand::<AgreementProtocol>::new(
        generation(&store),
        store.home_revision().unwrap(),
        foreign_alpha.proof_source::<AgreementProtocol>(
            foreign.domain_revision(&foreign_alpha).unwrap(),
            Role::agreeing([3; 16]),
        ),
    )
    .unwrap();
    assert!(matches!(
        compose(&store, foreign_command),
        Err(ProofCompositionError::ForeignDomain { domain: "alpha" })
    ));

    let called = Arc::new(AtomicBool::new(false));
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let cancelled = command(
        &store,
        &alpha,
        Role::agreeing([3; 16]).tracking(Arc::clone(&called)),
    )
    .with_cancellation(cancellation);
    assert!(matches!(
        compose(&store, cancelled),
        Err(ProofCompositionError::CancelledBeforeAdmission)
    ));
    assert!(!called.load(Ordering::SeqCst));

    let admitted_cancellation = CommandCancellation::new();
    let mut admitted = command(&store, &alpha, Role::agreeing([3; 16]));
    admitted
        .add_witness(beta.proof_witness(
            store.domain_revision(&beta).unwrap(),
            Role::agreeing([3; 16]).cancelling(admitted_cancellation.clone()),
        ))
        .unwrap();
    admitted = admitted.with_cancellation(admitted_cancellation.clone());
    assert!(compose(&store, admitted).is_ok());
    assert!(admitted_cancellation.is_cancelled());

    let oversized_domain = store.register_domain::<OversizedDomain>().unwrap();
    let oversized = HomeProofCommand::<OversizedProtocol>::new(
        generation(&store),
        store.home_revision().unwrap(),
        oversized_domain.proof_source::<OversizedProtocol>(
            store.domain_revision(&oversized_domain).unwrap(),
            Role::<[u8; MAX_PROOF_CORRELATION_BYTES]>::agreeing([0; MAX_PROOF_CORRELATION_BYTES]),
        ),
    );
    assert!(matches!(
        oversized,
        Err(ProofCommandBuildError::CorrelationSize {
            limit: MAX_PROOF_CORRELATION_BYTES,
            ..
        })
    ));

    let malformed = store
        .register_domain::<MalformedExpectationDomain>()
        .unwrap();
    let malformed = HomeProofCommand::<AgreementProtocol>::new(
        generation(&store),
        store.home_revision().unwrap(),
        malformed.proof_source::<AgreementProtocol>(
            store.domain_revision(&malformed).unwrap(),
            Role::agreeing([0; 16]),
        ),
    );
    assert!(matches!(
        malformed,
        Err(ProofCommandBuildError::ExpectedCorrelationShape {
            actual: 8,
            expected: 16,
        })
    ));

    let mut bounded = command(&store, &alpha, Role::agreeing([4; 16]));
    bounded
        .add_witness(beta.proof_witness(
            store.domain_revision(&beta).unwrap(),
            Role::agreeing([4; 16]),
        ))
        .unwrap()
        .add_witness(role1.proof_witness(
            store.domain_revision(&role1).unwrap(),
            Role::agreeing([4; 16]),
        ))
        .unwrap()
        .add_witness(role2.proof_witness(
            store.domain_revision(&role2).unwrap(),
            Role::agreeing([4; 16]),
        ))
        .unwrap()
        .add_witness(role3.proof_witness(
            store.domain_revision(&role3).unwrap(),
            Role::agreeing([4; 16]),
        ))
        .unwrap()
        .add_witness(role4.proof_witness(
            store.domain_revision(&role4).unwrap(),
            Role::agreeing([4; 16]),
        ))
        .unwrap()
        .add_witness(role5.proof_witness(
            store.domain_revision(&role5).unwrap(),
            Role::agreeing([4; 16]),
        ))
        .unwrap()
        .add_witness(role6.proof_witness(
            store.domain_revision(&role6).unwrap(),
            Role::agreeing([4; 16]),
        ))
        .unwrap();
    assert!(matches!(
        bounded.add_witness(role7.proof_witness(
            store.domain_revision(&role7).unwrap(),
            Role::agreeing([4; 16])
        )),
        Err(ProofCommandBuildError::RoleLimit {
            limit: MAX_PROOF_ROLES
        })
    ));
    assert!(store.pending_reconciliations().is_empty());
}

#[test]
#[cfg(feature = "test-faults")]
fn registration_invariant_fault_rejects_before_proof_callbacks() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open_with_faults(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let called = Arc::new(AtomicBool::new(false));
    faults.fail_next(FaultPoint::BeforeVerification);

    assert!(matches!(
        compose(
            &store,
            command(
                &store,
                &alpha,
                Role::agreeing([9; 16]).tracking(Arc::clone(&called)),
            ),
        ),
        Err(ProofCompositionError::DomainRegistrationInvariant { domain: "alpha" })
    ));
    assert!(!called.load(Ordering::SeqCst));
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    assert!(store.pending_reconciliations().is_empty());
}

#[test]
#[cfg(feature = "test-faults")]
fn stale_executable_is_rejected_after_same_home_generation_recovery() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open_with_faults(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let (stale, _consumer) = command(&store, &alpha, Role::agreeing([11; 16]))
        .seal()
        .unwrap();
    faults.fail_next(FaultPoint::BeforeVerification);
    assert!(matches!(
        compose(&store, command(&store, &alpha, Role::agreeing([12; 16]))),
        Err(ProofCompositionError::DomainRegistrationInvariant { domain: "alpha" })
    ));
    let recovered = store.recover_same_home().unwrap().publish();
    assert!(matches!(
        recovered.compose_proof(stale),
        Err(ProofCompositionError::StaleGeneration)
    ));
}
