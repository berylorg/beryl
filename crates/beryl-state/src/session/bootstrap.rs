use beryl_home_store::{DomainHandle, HomeStore, PointReadLimit};

use super::{
    MinimalSessionBootstrap, SESSION_HEADER_V1_BYTES, SESSION_WINDOW_V1_BYTES, SessionDomain,
    SessionHeader, SessionReadError,
    codec::{HEADER_KEY, SessionHeaderCodec, SessionWindowCodec},
};

pub(super) fn read(
    handle: &DomainHandle<SessionDomain>,
    store: &HomeStore,
) -> Result<Option<MinimalSessionBootstrap>, SessionReadError> {
    let first = read_header(handle, store)?;
    let Some(header) = first else {
        return if read_header(handle, store)?.is_none() {
            Ok(None)
        } else {
            Err(SessionReadError::ConcurrentPublication)
        };
    };

    let mut windows = Vec::with_capacity(header.windows.len());
    let mut record_error = None;
    for reference in &header.windows {
        match store.read_point::<SessionDomain, SessionWindowCodec>(
            handle,
            &reference.window_id,
            point_limit(SESSION_WINDOW_V1_BYTES),
        )? {
            None => {
                record_error = Some(SessionReadError::MissingWindow {
                    window_id: reference.window_id,
                });
                break;
            }
            Some(window) if window.window_id != reference.window_id => {
                record_error = Some(SessionReadError::WindowIdentityMismatch {
                    window_id: reference.window_id,
                });
                break;
            }
            Some(window) if window.revision != reference.record_revision => {
                record_error = Some(SessionReadError::WindowRevisionConflict {
                    window_id: reference.window_id,
                    expected: reference.record_revision,
                    current: window.revision,
                });
                break;
            }
            Some(window) => windows.push(window),
        }
    }

    if read_header(handle, store)?.as_ref() != Some(&header) {
        return Err(SessionReadError::ConcurrentPublication);
    }
    if let Some(error) = record_error {
        return Err(error);
    }
    Ok(Some(MinimalSessionBootstrap { header, windows }))
}

fn read_header(
    handle: &DomainHandle<SessionDomain>,
    store: &HomeStore,
) -> Result<Option<SessionHeader>, SessionReadError> {
    store
        .read_point::<SessionDomain, SessionHeaderCodec>(
            handle,
            &HEADER_KEY,
            point_limit(SESSION_HEADER_V1_BYTES),
        )
        .map_err(Into::into)
}

fn point_limit(payload: usize) -> PointReadLimit {
    PointReadLimit::new(payload + 4).expect("fixed session point limit is nonzero")
}
