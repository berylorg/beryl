use super::{
    OutageAssembly, OutageAssemblyError, OutageAssemblyLimits, OutageBuffer,
    OutageConnectionIdentity,
};
use beryl_backend::{
    ProviderObservationBegin, ProviderObservationControl, ProviderObservationRoute,
    ProviderValueContext,
};
use beryl_model::ProviderObservationId;

enum Observation {
    Empty,
    Open(OutageAssembly),
    Sealed(OutageAssembly, ProviderObservationRoute),
}

pub struct OutageObservationSlot {
    connection: OutageConnectionIdentity,
    limits: OutageAssemblyLimits,
    observation: Observation,
    gap: bool,
    accepting: bool,
}

impl OutageObservationSlot {
    pub fn new(connection: OutageConnectionIdentity, limits: OutageAssemblyLimits) -> Self {
        Self {
            connection,
            limits,
            observation: Observation::Empty,
            gap: false,
            accepting: true,
        }
    }

    pub fn begin(
        &mut self,
        identity: ProviderObservationId,
        begin: ProviderObservationBegin,
        ready: Option<&mut OutageBuffer>,
    ) -> Result<(), OutageAssemblyError> {
        if !self.accepting {
            return Err(OutageAssemblyError::RetentionLoss);
        }
        if let Some(buffer) = ready {
            self.flush(buffer);
        }
        match std::mem::replace(&mut self.observation, Observation::Empty) {
            Observation::Empty => {}
            Observation::Sealed(..) => self.gap = true,
            Observation::Open(_) => {
                self.gap = true;
                return Err(OutageAssemblyError::Malformed);
            }
        }
        self.observation = Observation::Open(OutageAssembly::new(
            self.connection,
            identity,
            begin,
            self.limits,
        ));
        Ok(())
    }

    pub fn control(
        &mut self,
        control: ProviderObservationControl,
    ) -> Result<(), OutageAssemblyError> {
        let Observation::Open(assembly) = &mut self.observation else {
            self.abandon();
            return Err(OutageAssemblyError::Malformed);
        };
        let result = assembly.control(control);
        if result == Err(OutageAssemblyError::Malformed) {
            self.abandon();
        }
        result
    }

    pub fn fragment(
        &mut self,
        context: ProviderValueContext,
        offset: usize,
        text: &str,
    ) -> Result<(), OutageAssemblyError> {
        let Observation::Open(assembly) = &mut self.observation else {
            self.abandon();
            return Err(OutageAssemblyError::Malformed);
        };
        let result = assembly.fragment(context, offset, text);
        if result == Err(OutageAssemblyError::Malformed) {
            self.abandon();
        }
        result
    }

    pub fn seal(
        &mut self,
        route: ProviderObservationRoute,
        ready: Option<&mut OutageBuffer>,
    ) -> Result<(), OutageAssemblyError> {
        let observation = std::mem::replace(&mut self.observation, Observation::Empty);
        let result = match observation {
            Observation::Open(mut assembly) => {
                let validation = assembly.validate_seal(&route);
                if let Some(buffer) = ready {
                    if validation.is_err() {
                        assembly.discard();
                    }
                    let publication = assembly.seal(buffer, Some(&route));
                    if self.gap {
                        self.connection.record_gap(buffer);
                    }
                    return validation.and(publication);
                }
                match validation {
                    Ok(()) => {
                        self.observation = Observation::Sealed(assembly, route);
                        Ok(())
                    }
                    Err(error) => {
                        self.gap = true;
                        Err(error)
                    }
                }
            }
            _ => {
                self.gap = true;
                Err(OutageAssemblyError::Malformed)
            }
        };
        if let Some(buffer) = ready {
            self.flush(buffer);
        }
        result
    }

    pub fn record_loss(&mut self) {
        self.gap = true;
    }

    pub fn abandon(&mut self) {
        self.observation = Observation::Empty;
        self.gap = true;
    }

    pub fn disable(&mut self) {
        self.abandon();
        self.accepting = false;
    }

    pub(in crate::cas_projection) fn discard_for_retirement(&mut self) -> bool {
        let lost = self.gap || !matches!(self.observation, Observation::Empty);
        self.disable();
        lost
    }

    pub fn flush(&mut self, buffer: &mut OutageBuffer) {
        if matches!(self.observation, Observation::Sealed(..)) {
            let Observation::Sealed(assembly, route) =
                std::mem::replace(&mut self.observation, Observation::Empty)
            else {
                unreachable!("sealed observation checked above")
            };
            let _ = assembly.seal(buffer, Some(&route));
        }
        if self.gap {
            self.connection.record_gap(buffer);
        }
    }

    pub fn retained_bytes(&self) -> usize {
        match &self.observation {
            Observation::Empty => 0,
            Observation::Open(assembly) => assembly.retained_bytes(),
            Observation::Sealed(assembly, route) => {
                assembly.retained_bytes()
                    + std::mem::size_of::<ProviderObservationRoute>()
                    + route.thread_id().as_str().len()
                    + route.turn_id().as_str().len()
            }
        }
    }

    pub fn retained_entries(&self) -> usize {
        match &self.observation {
            Observation::Empty => 0,
            Observation::Open(assembly) => assembly.retained_entries(),
            Observation::Sealed(assembly, _) => assembly.retained_entries() + 1,
        }
    }
}
