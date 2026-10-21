use super::*;
use gpui::ImageResourceDrain;

impl ProcessServiceOwner {
    pub(crate) fn media_process(&self) -> Option<Arc<ImageMediaProcess>> {
        self.media_process
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub(crate) fn retire_media_attachment(&self) -> ImageResourceDrain {
        self.graph.as_ref().or(self.closing_graph.as_ref()).map_or(
            ImageResourceDrain::Drained,
            PublishedAppServices::retire_media,
        )
    }

    pub(crate) fn media_failure(&self) -> Option<crate::image_processing::ImageMediaFailure> {
        self.media_process()
            .and_then(|process| process.domain().failure())
            .map(crate::image_processing::ImageMediaFailure::Native)
    }

    pub(super) fn prepare_media_process(
        &self,
        configuration: &AppServiceConfiguration,
    ) -> Result<Option<Arc<ImageMediaProcess>>, AppServiceOpenError> {
        let Some(domain) = configuration.media_domain.as_ref() else {
            #[cfg(test)]
            if configuration.allow_absent_media_domain_for_tests {
                return Ok(None);
            }
            return Err(AppServiceOpenError::MediaDomainUnavailable);
        };
        let mut media = self
            .media_process
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(process) = media.as_ref() {
            if !process.domain().same_domain(domain)
                || process.limits() != configuration.media_limits
            {
                return Err(AppServiceOpenError::MediaConfigurationChanged);
            }
            return Ok(Some(process.clone()));
        }
        let process = ImageMediaProcess::start(domain.clone(), configuration.media_limits)?;
        *media = Some(process.clone());
        Ok(Some(process))
    }

    pub(crate) fn begin_media_process_shutdown(&self) -> ImageResourceDrain {
        self.retire_media_attachment();
        self.media_process()
            .map_or(ImageResourceDrain::Drained, |process| {
                process.begin_shutdown()
            })
    }

    pub(crate) fn poll_media_process_shutdown(&self) -> ImageResourceDrain {
        self.media_process()
            .map_or(ImageResourceDrain::Drained, |process| {
                process.poll_shutdown()
            })
    }
}

impl PublishedAppServices {
    pub(crate) fn media_attachment(&self) -> Option<Arc<ImageMediaAttachment>> {
        self.media.clone()
    }

    pub(super) fn retire_media(&self) -> ImageResourceDrain {
        self.media
            .as_ref()
            .map_or(ImageResourceDrain::Drained, |attachment| {
                attachment.retire()
            })
    }
}
