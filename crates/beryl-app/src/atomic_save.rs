use std::io::Write;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum AtomicSaveError {
    #[error("destination refused")]
    Destination,
    #[error("staging file could not be created")]
    Create,
    #[error("writing failed")]
    Write,
    #[error("staging synchronization failed")]
    Prepare,
    #[error("publication failed")]
    Publish,
    #[error("temporary output cleanup failed; a temporary file may remain")]
    Cleanup,
}

pub(crate) struct AtomicSaveStage {
    stage: Option<tempfile::NamedTempFile>,
    destination: gpui::SavePathChoice,
    prepared: bool,
}

impl AtomicSaveStage {
    pub(crate) fn create(
        destination: gpui::SavePathChoice,
        prefix: &str,
    ) -> Result<Self, AtomicSaveError> {
        let path = &destination.path;
        if path.as_os_str().len() > 32768 || !path.is_absolute() {
            return Err(AtomicSaveError::Destination);
        }
        let parent = path.parent().ok_or(AtomicSaveError::Destination)?;
        let stage = tempfile::Builder::new()
            .prefix(prefix)
            .tempfile_in(parent)
            .map_err(|_| AtomicSaveError::Create)?;
        Ok(Self {
            stage: Some(stage),
            destination,
            prepared: false,
        })
    }

    pub(crate) fn write(&mut self, bytes: &[u8]) -> Result<(), AtomicSaveError> {
        self.prepared = false;
        self.stage
            .as_mut()
            .ok_or(AtomicSaveError::Write)?
            .write_all(bytes)
            .map_err(|_| AtomicSaveError::Write)
    }

    pub(crate) fn prepare(&mut self) -> Result<(), AtomicSaveError> {
        self.prepared = false;
        let stage = self.stage.as_mut().ok_or(AtomicSaveError::Prepare)?;
        stage.flush().map_err(|_| AtomicSaveError::Prepare)?;
        stage
            .as_file()
            .sync_all()
            .map_err(|_| AtomicSaveError::Prepare)?;
        self.prepared = true;
        Ok(())
    }

    pub(crate) fn publish(&mut self) -> Result<(), AtomicSaveError> {
        if !self.prepared {
            return Err(AtomicSaveError::Publish);
        }
        let stage = self.stage.take().ok_or(AtomicSaveError::Publish)?;
        let result = if self.destination.replace_existing {
            stage.persist(&self.destination.path)
        } else {
            stage.persist_noclobber(&self.destination.path)
        };
        match result {
            Ok(file) => {
                drop(file);
                Ok(())
            }
            Err(error) => {
                self.stage = Some(error.file);
                Err(AtomicSaveError::Publish)
            }
        }
    }

    pub(crate) fn cleanup(mut self) -> Result<(), AtomicSaveError> {
        if let Some(stage) = self.stage.take() {
            stage.close().map_err(|_| AtomicSaveError::Cleanup)?;
        }
        Ok(())
    }
}
