use super::*;

impl PreparedMainWindowRestoreSet {
    pub fn prepare_native(
        mut self,
    ) -> Result<PreparedNativeMainWindowRestoreSet, MainWindowNativePreparationFailure> {
        let prepared = self.native_facts();
        let (placements, validation) = match prepared {
            Ok(facts) => facts,
            Err(error) => {
                return Err(MainWindowNativePreparationFailure {
                    prepared: self,
                    error,
                });
            }
        };
        let members = std::mem::take(&mut self.owner.members)
            .into_iter()
            .zip(placements)
            .map(|(member, placement)| {
                let window_id = member.window_id();
                let prepared = match member {
                    PreparedRestoreSetMember::Restored(prepared) => {
                        MainWindowStartupShellPrepared::Restored(*prepared)
                    }
                    PreparedRestoreSetMember::Threadless(prepared) => {
                        MainWindowStartupShellPrepared::Threadless(prepared)
                    }
                    PreparedRestoreSetMember::Replacement(prepared) => {
                        MainWindowStartupShellPrepared::Acquired(*prepared)
                    }
                };
                PreparedNativeMember {
                    window_id,
                    prepared: Box::new(prepared),
                    placement,
                }
            })
            .collect();
        Ok(PreparedNativeMainWindowRestoreSet {
            owner: Box::new(self.owner),
            members,
            validation,
            #[cfg(feature = "test-faults")]
            faults: test_faults::NativeRestoreSetFaults::default(),
        })
    }

    fn native_facts(
        &self,
    ) -> Result<(Vec<PreparedWindowsWindowPlacement>, Vec<NativeValidation>), String> {
        self.revalidate()?;
        if self.owner.expected_revision.is_none() {
            return Err("native startup has no exact session revision".to_owned());
        }
        let mut placements = Vec::with_capacity(self.owner.members.len());
        let mut validation = Vec::with_capacity(self.owner.members.len());
        for member in &self.owner.members {
            let (saved, check) = match member {
                PreparedRestoreSetMember::Restored(prepared) => {
                    (prepared.placement(), prepared.native_validation())
                }
                PreparedRestoreSetMember::Threadless(prepared) => {
                    (prepared.placement(), prepared.native_validation())
                }
                PreparedRestoreSetMember::Replacement(prepared) => {
                    (prepared.startup_placement(), prepared.native_validation())
                }
            };
            placements.push(
                prepare_windows_window_placement(member.window_id(), saved.clone())
                    .map_err(|e| e.to_string())?,
            );
            validation.push(NativeValidation {
                window_id: member.window_id(),
                check,
            });
        }
        self.revalidate()?;
        Ok((placements, validation))
    }
}
