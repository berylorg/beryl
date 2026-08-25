use crate::command::MaterializedDomainDescriptor;

use super::{
    digest,
    erased::{
        ErasedObservation, ErasedWitnessObservation, SuccessorDescriptor, SuccessorExecution,
        SuccessorProtocolIdentity, SuccessorRoleDescriptor, SuccessorRoleFact, SuccessorRoleKind,
        SuccessorRoleReservation, SuccessorRoleResult,
    },
};

impl SuccessorDescriptor {
    pub(crate) fn unrun_collision(&self) -> SuccessorExecution {
        missing_execution(self.identity, &self.roles)
    }

    pub(crate) fn execute(
        &self,
        snapshot: &fjall::Snapshot,
        registry: &crate::domain::DomainRegistry,
        domains: &[MaterializedDomainDescriptor],
    ) -> Result<SuccessorExecution, (&'static str, crate::domain::callback::ErasedCallbackError)>
    {
        let source = self.roles.iter().find_map(|role| match &role.role {
            SuccessorRoleReservation::Source(source) => Some((role.domain_slot, source)),
            SuccessorRoleReservation::Witness(_) => None,
        });
        let Some((source_slot, source)) = source else {
            return Ok(SuccessorExecution {
                identity: self.identity,
                resolved: false,
                roles: Vec::new(),
                correlation_digest: None,
            });
        };
        let Some(source_domain) = registry.get(source_slot) else {
            return Ok(missing_execution(self.identity, &self.roles));
        };
        if source_domain.name != source.domain || source_domain.owner != source.owner {
            return Ok(missing_execution(self.identity, &self.roles));
        }
        let Some(source_descriptor) = domains
            .iter()
            .find(|descriptor| descriptor.domain_slot == source_slot)
        else {
            return Ok(missing_execution(self.identity, &self.roles));
        };
        let source_observation = (source.authenticate)(
            source.state.as_ref(),
            snapshot,
            source_domain,
            source_descriptor,
        )
        .map_err(|error| (source.domain, error))?;
        let mut facts = Vec::with_capacity(self.roles.len());
        let (correlation, encoded) = match source_observation {
            ErasedObservation::Authenticated {
                correlation,
                encoded,
            } => {
                facts.push(SuccessorRoleFact {
                    domain_slot: source_slot,
                    kind: SuccessorRoleKind::Source,
                    result: SuccessorRoleResult::Authenticated,
                    correlation_digest: Some(digest(&encoded)),
                    derived: Vec::new(),
                });
                (correlation, encoded)
            }
            ErasedObservation::Unresolved => {
                facts.push(simple_role_fact(
                    source_slot,
                    SuccessorRoleKind::Source,
                    SuccessorRoleResult::Unresolved,
                ));
                append_unrun_witnesses(&mut facts, &self.roles);
                return Ok(SuccessorExecution {
                    identity: self.identity,
                    resolved: false,
                    roles: facts,
                    correlation_digest: None,
                });
            }
            ErasedObservation::Collision => {
                facts.push(simple_role_fact(
                    source_slot,
                    SuccessorRoleKind::Source,
                    SuccessorRoleResult::Collision,
                ));
                append_unrun_witnesses(&mut facts, &self.roles);
                return Ok(SuccessorExecution {
                    identity: self.identity,
                    resolved: false,
                    roles: facts,
                    correlation_digest: None,
                });
            }
        };
        let expected_digest = digest(&encoded);
        let mut resolved = true;
        for role in &self.roles {
            let SuccessorRoleReservation::Witness(witness) = &role.role else {
                continue;
            };
            let Some(domain) = registry.get(role.domain_slot) else {
                resolved = false;
                facts.push(simple_role_fact(
                    role.domain_slot,
                    SuccessorRoleKind::Witness,
                    SuccessorRoleResult::Missing,
                ));
                continue;
            };
            if domain.name != witness.domain || domain.owner != witness.owner {
                resolved = false;
                facts.push(simple_role_fact(
                    role.domain_slot,
                    SuccessorRoleKind::Witness,
                    SuccessorRoleResult::Missing,
                ));
                continue;
            }
            let execution = (witness.authenticate)(
                witness.state.as_ref(),
                correlation.as_ref(),
                snapshot,
                domain,
                &witness.reads,
            )
            .map_err(|error| (witness.domain, error))?;
            let (result, observed_digest) = match execution.observation {
                ErasedWitnessObservation::Authenticated {
                    agrees,
                    encoded: observed,
                } if !execution.rejected => {
                    let observed_digest = digest(&observed);
                    if agrees {
                        (SuccessorRoleResult::Authenticated, Some(observed_digest))
                    } else {
                        resolved = false;
                        (SuccessorRoleResult::Mismatch, Some(observed_digest))
                    }
                }
                ErasedWitnessObservation::Unresolved if !execution.rejected => {
                    resolved = false;
                    (SuccessorRoleResult::Unresolved, None)
                }
                _ => {
                    resolved = false;
                    (SuccessorRoleResult::Collision, None)
                }
            };
            facts.push(SuccessorRoleFact {
                domain_slot: role.domain_slot,
                kind: SuccessorRoleKind::Witness,
                result,
                correlation_digest: observed_digest,
                derived: execution.facts,
            });
        }
        Ok(SuccessorExecution {
            identity: self.identity,
            resolved,
            roles: facts,
            correlation_digest: Some(expected_digest),
        })
    }
}

fn missing_execution(
    identity: SuccessorProtocolIdentity,
    roles: &[SuccessorRoleDescriptor],
) -> SuccessorExecution {
    SuccessorExecution {
        identity,
        resolved: false,
        roles: roles
            .iter()
            .map(|role| {
                simple_role_fact(
                    role.domain_slot,
                    if role.role.is_source() {
                        SuccessorRoleKind::Source
                    } else {
                        SuccessorRoleKind::Witness
                    },
                    SuccessorRoleResult::Missing,
                )
            })
            .collect(),
        correlation_digest: None,
    }
}

fn append_unrun_witnesses(facts: &mut Vec<SuccessorRoleFact>, roles: &[SuccessorRoleDescriptor]) {
    facts.extend(roles.iter().filter_map(|role| match role.role {
        SuccessorRoleReservation::Witness(_) => Some(simple_role_fact(
            role.domain_slot,
            SuccessorRoleKind::Witness,
            SuccessorRoleResult::Missing,
        )),
        SuccessorRoleReservation::Source(_) => None,
    }));
}

fn simple_role_fact(
    domain_slot: usize,
    kind: SuccessorRoleKind,
    result: SuccessorRoleResult,
) -> SuccessorRoleFact {
    SuccessorRoleFact {
        domain_slot,
        kind,
        result,
        correlation_digest: None,
        derived: Vec::new(),
    }
}
