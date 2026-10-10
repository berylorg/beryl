use super::*;
use crate::model_selection::{ModelReaderLimits, PublishedModelReader, PublishedModelSelection};

impl ProcessServiceOwner {
    pub(crate) fn model_selection_reader(&self) -> Option<PublishedModelSelection> {
        let graph = self.graph()?;
        if graph.shutdown.is_some() {
            return None;
        }
        let lifetime = Arc::downgrade(graph.restore_lifetime.as_ref()?);
        Some(
            graph
                .model_selection
                .get_or_init(|| {
                    let limits = ModelReaderLimits::new(
                        std::num::NonZeroUsize::new(16).unwrap(),
                        std::num::NonZeroUsize::new(32).unwrap(),
                        64,
                        std::time::Duration::from_secs(10),
                    )
                    .expect("fixed bounded model reader configuration");
                    PublishedModelSelection {
                        reader: PublishedModelReader::new(
                            graph.cas().model_read_source(),
                            graph.state().session(),
                            lifetime.clone(),
                            limits,
                        ),
                        sessions: graph.sessions.downgrade(),
                        lifetime,
                    }
                })
                .clone(),
        )
    }
}
