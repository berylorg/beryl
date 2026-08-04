use crate::{HomeHealthState, HomeStore};

impl HomeStore {
    /// Installs an unobserved Fjall maintenance terminal for publication-boundary tests.
    ///
    /// The raw database remains package-private, and this seam deliberately leaves
    /// the Beryl health gate healthy so the next state-dependent result must detect
    /// the retained dependency terminal during publication confirmation.
    pub fn inject_retained_maintenance_terminal(&self) {
        let generation = self
            .generation
            .read()
            .expect("maintenance-terminal fixture generation lock is poisoned");
        let generation = generation
            .as_ref()
            .expect("maintenance-terminal fixture requires a current generation");
        generation
            .database
            .health()
            .expect("maintenance-terminal fixture requires initially healthy Fjall state");
        assert_eq!(self.health.snapshot().state(), HomeHealthState::Healthy);

        fjall::test_faults::retain_maintenance_terminal(&generation.database);

        generation
            .database
            .health()
            .expect_err("maintenance-terminal fixture did not retain a Fjall terminal");
        assert_eq!(self.health.snapshot().state(), HomeHealthState::Healthy);
    }
}
