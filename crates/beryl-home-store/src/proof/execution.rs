use super::*;

impl crate::HomeStore {
    pub fn compose_proof<P: HomeProofProtocol>(
        &self,
        command: ExecutableHomeProofCommand<P>,
    ) -> Result<HomeProofReceipt<P>, ProofCompositionError> {
        let command_id = command.command_id;
        let command = command.command;
        let cancellation = command.cancellation.clone();
        if cancellation.is_cancelled() {
            return Err(ProofCompositionError::CancelledBeforeAdmission);
        }
        if ActiveWriter::already_active(self.writer_id) {
            return Err(ProofCompositionError::ReentrantWriter);
        }
        let admission = self.health.admit()?;
        let generation_guard = match self.generation.read() {
            Ok(generation) => generation,
            Err(_) => {
                admission.fail(FailureSeverity::Structural);
                return Err(ProofCompositionError::GenerationPoisoned);
            }
        };
        let generation = match generation_guard.as_ref() {
            Some(generation) => generation,
            None => {
                admission.fail(FailureSeverity::Structural);
                return Err(ProofCompositionError::GenerationPoisoned);
            }
        };
        let result =
            self.compose_proof_admitted(generation, admission.generation(), command_id, command);
        if let Err(error) = &result {
            if let Some(severity) = failure_severity(error) {
                admission.fail(severity);
                return result;
            }
        }
        admission.confirm()?;
        result
    }

    fn compose_proof_admitted<P: HomeProofProtocol>(
        &self,
        generation: &StoreGeneration,
        health_generation: HomeGeneration,
        command_id: u64,
        command: HomeProofCommand<P>,
    ) -> Result<HomeProofReceipt<P>, ProofCompositionError> {
        if command.expected_generation != health_generation {
            return Err(ProofCompositionError::StaleGeneration);
        }
        let snapshot = generation.database.snapshot().map_err(|source| {
            ProofCompositionError::RevisionRead {
                source: crate::ReadError::Storage {
                    stage: crate::ReadStage::HomeRevision,
                    source: Box::new(crate::health::ClassifiedFjallError::direct(source)),
                },
            }
        })?;
        let current_home = read_home_revision(&snapshot, generation.header_keyspace())
            .map_err(|source| ProofCompositionError::RevisionRead { source })?;
        self.faults
            .check(crate::fault::FaultPoint::BeforeVerification)
            .map_err(|_| ProofCompositionError::DomainRegistrationInvariant {
                domain: command.source.plan.domain,
            })?;
        let mut conflicts = Vec::with_capacity(MAX_PROOF_ROLES);
        if current_home != command.expected_home_revision {
            conflicts.push(RevisionConflict::Home {
                expected: command.expected_home_revision,
                current: current_home,
            });
        }
        let source = prepare_role(
            generation,
            &snapshot,
            &command.source.plan,
            command.source.expected_revision,
            &mut conflicts,
        )?;
        let mut witnesses = Vec::with_capacity(MAX_PROOF_ROLES - 1);
        for witness in &command.witnesses {
            witnesses.push(prepare_role(
                generation,
                &snapshot,
                &witness.plan,
                witness.expected_revision,
                &mut conflicts,
            )?);
        }
        if !conflicts.is_empty() {
            conflicts.sort_by(|left, right| conflict_name(left).cmp(conflict_name(right)));
            return Err(ProofCompositionError::Conflict {
                conflicts_len: conflicts.len(),
                conflicts,
            });
        }
        let source_domain = source.plan.domain;
        let correlation = prove(source.plan, &snapshot, source.domain)
            .map_err(|error| callback_error(source_domain, error))?;
        for witness in &witnesses {
            let observed = prove(witness.plan, &snapshot, witness.domain)
                .map_err(|source| callback_error(witness.plan.domain, source))?;
            if !observed.agrees_with(correlation) {
                return Err(ProofCompositionError::Disagreement {
                    domain: witness.plan.domain,
                });
            }
        }
        Ok(HomeProofReceipt::new(
            generation.instance_id,
            health_generation,
            command_id,
            current_home,
            &source,
            &witnesses,
            correlation,
        ))
    }
}

fn prepare_role<'a, P: HomeProofProtocol>(
    generation: &'a StoreGeneration,
    snapshot: &fjall::Snapshot,
    plan: &'a ProofRolePlan<P>,
    expected_revision: DomainRevision,
    conflicts: &mut Vec<RevisionConflict>,
) -> Result<PreparedProofRole<'a, P>, ProofCompositionError> {
    if plan.store != generation.instance_id {
        return Err(ProofCompositionError::ForeignDomain {
            domain: plan.domain,
        });
    }
    let domain = generation
        .registry
        .get(plan.slot)
        .filter(|domain| domain.name == plan.domain && domain.owner == plan.owner)
        .ok_or(ProofCompositionError::ForeignDomain {
            domain: plan.domain,
        })?;
    let metadata = read_domain_metadata(snapshot, generation.domains_keyspace(), plan.domain)
        .map_err(|source| ProofCompositionError::RevisionRead { source })?;
    if metadata != domain.metadata(metadata.revision) {
        return Err(ProofCompositionError::DomainRegistrationInvariant {
            domain: plan.domain,
        });
    }
    if metadata.revision != expected_revision {
        conflicts.push(RevisionConflict::Domain {
            domain: plan.domain,
            expected: expected_revision,
            current: metadata.revision,
        });
    }
    Ok(PreparedProofRole {
        plan,
        domain,
        revision: metadata.revision,
    })
}

fn conflict_name(conflict: &RevisionConflict) -> &'static str {
    match conflict {
        RevisionConflict::Home { .. } => "",
        RevisionConflict::Domain { domain, .. } => domain,
    }
}
