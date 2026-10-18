use std::{fmt, io};

use beryl_home_store::{
    CommandBuildError, CursorDirection, CursorRange, CursorReadLimits, DomainMutation,
    DomainReader, DomainReconciliation, DomainSchemaVersion, DomainValidator,
    HomeCandidateRecoveryAccess, HomeCommand, HomeStore, KeyspaceSchemaVersion, MutationBuilder,
    MutationContribution, PointReadLimit, ReadError, ReconciliationReader,
    ReconciliationReservation, RecordCodec, RecordFamily, RecordVersion, StorageDomain,
    ValidationContribution,
};
use beryl_model::{DomainRevision, WindowId};

use super::{
    MAX_RESTORABLE_WINDOWS, SESSION_HEADER_V1_BYTES, SESSION_WINDOW_V1_BYTES, SessionDomain,
    SessionExitIntent, SessionHeader, SessionMutationError, SessionState, SessionWindowRecord,
    codec::{HEADER_KEY, SessionHeaderCodec, SessionWindowCodec},
    error::SessionValidationError,
};
use crate::RecordRevision;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivityPanelHeight(u32);

impl ActivityPanelHeight {
    pub fn new(value: f32) -> Result<Self, ActivityPanelHeightError> {
        if value.is_finite() && value > 0.0 {
            Ok(Self(value.to_bits()))
        } else {
            Err(ActivityPanelHeightError)
        }
    }

    pub fn get(self) -> f32 {
        f32::from_bits(self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivityPanelHeightError;

impl fmt::Display for ActivityPanelHeightError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Activity panel height must be positive and finite")
    }
}

impl std::error::Error for ActivityPanelHeightError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowPresentationRecord {
    window: WindowId,
    height: ActivityPanelHeight,
    revision: RecordRevision,
}

impl WindowPresentationRecord {
    pub fn window_id(&self) -> WindowId {
        self.window
    }
    pub fn activity_panel_height(&self) -> ActivityPanelHeight {
        self.height
    }
    pub fn revision(&self) -> RecordRevision {
        self.revision
    }

    pub(super) fn recovered(&self) -> Result<Self, SessionMutationError> {
        Ok(Self {
            revision: self.revision.checked_next()?,
            ..self.clone()
        })
    }
}

pub(crate) struct WindowPresentationDomain;
pub(super) struct WindowPresentationCodec;

impl StorageDomain for WindowPresentationDomain {
    const NAME: &'static str = "beryl-window-presentation";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] =
        &[RecordFamily::new::<WindowPresentationCodec>(
            KeyspaceSchemaVersion::new(1),
        )];
    type ValidationError = SessionValidationError;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = std::convert::Infallible;

    fn create_runtime_attachment(
        _: &beryl_home_store::DomainRegistrationReader<'_, Self>,
    ) -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        let page = reader.cursor::<WindowPresentationCodec>(
            &CursorRange::closed(
                WindowId::from_bytes([0; 16]),
                WindowId::from_bytes([255; 16]),
            ),
            CursorDirection::Forward,
            CursorReadLimits::new(MAX_RESTORABLE_WINDOWS, 16_384)
                .expect("fixed presentation limits"),
        )?;
        if page.has_more()
            || page
                .records()
                .iter()
                .any(|record| *record.key() != record.value().window)
        {
            return Err(SessionValidationError::Invariant(
                "window presentation identities or count are invalid",
            ));
        }
        Ok(())
    }

    fn reconcile(
        reader: &ReconciliationReader<'_, Self>,
    ) -> Result<DomainReconciliation, Self::ValidationError> {
        let mut classification = crate::reconciliation::ReconciliationClassification::new();
        crate::reconciliation::classify_records::<Self, WindowPresentationCodec>(
            reader,
            &mut classification,
        )?;
        Ok(classification.finish())
    }
}

impl RecordCodec<WindowPresentationDomain> for WindowPresentationCodec {
    type Key = WindowId;
    type Value = WindowPresentationRecord;
    type Error = io::Error;
    const FAMILY: &'static str = "windows";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = 28;

    fn encode_key(key: &WindowId) -> Result<Vec<u8>, io::Error> {
        Ok(key.as_bytes().to_vec())
    }
    fn decode_key(encoded: &[u8]) -> Result<WindowId, io::Error> {
        Ok(WindowId::from_bytes(
            encoded.try_into().map_err(|_| codec_error())?,
        ))
    }
    fn encode_value(value: &WindowPresentationRecord) -> Result<Vec<u8>, io::Error> {
        let mut bytes = value.window.as_bytes().to_vec();
        bytes.extend_from_slice(&value.height.0.to_le_bytes());
        bytes.extend_from_slice(&value.revision.get().to_le_bytes());
        Ok(bytes)
    }
    fn decode_value(encoded: &[u8]) -> Result<WindowPresentationRecord, io::Error> {
        if encoded.len() != 28 {
            return Err(codec_error());
        }
        Ok(WindowPresentationRecord {
            window: WindowId::from_bytes(encoded[..16].try_into().map_err(|_| codec_error())?),
            height: ActivityPanelHeight::new(f32::from_bits(u32::from_le_bytes(
                encoded[16..20].try_into().map_err(|_| codec_error())?,
            )))
            .map_err(|_| codec_error())?,
            revision: RecordRevision::new(u64::from_le_bytes(
                encoded[20..28].try_into().map_err(|_| codec_error())?,
            ))
            .map_err(|_| codec_error())?,
        })
    }
}

fn codec_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid window presentation record",
    )
}
pub(super) fn point_limit() -> PointReadLimit {
    PointReadLimit::new(32).expect("fixed presentation point limit")
}

#[derive(Debug)]
enum Participant {
    Mutation(MutationContribution),
    Validation(ValidationContribution),
}

#[derive(Debug)]
pub struct PreparedWindowMutation {
    session: Participant,
    presentation: Participant,
}

pub type PreparedWindowPresentationUpdate = PreparedWindowMutation;

impl PreparedWindowMutation {
    pub fn add_to(self, command: &mut HomeCommand) -> Result<(), CommandBuildError> {
        for participant in [self.session, self.presentation] {
            match participant {
                Participant::Mutation(value) => {
                    command.add(value)?;
                }
                Participant::Validation(value) => {
                    command.add_validation(value)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
struct PresentationChange {
    window: WindowId,
    expected: Option<WindowPresentationRecord>,
    next: Option<WindowPresentationRecord>,
}

impl DomainValidator<WindowPresentationDomain> for PresentationChange {
    type Error = SessionMutationError;
    fn validate(
        &self,
        reader: &DomainReader<'_, WindowPresentationDomain>,
    ) -> Result<(), Self::Error> {
        if reader.point::<WindowPresentationCodec>(&self.window, point_limit())? != self.expected {
            return Err(SessionMutationError::InvalidCurrentState(
                "window presentation source changed",
            ));
        }
        Ok(())
    }
}

impl DomainMutation<WindowPresentationDomain> for PresentationChange {
    type Error = SessionMutationError;
    type Prepared = Self;
    fn prepare(
        self,
        reader: &DomainReader<'_, WindowPresentationDomain>,
    ) -> Result<Self, Self::Error> {
        self.validate(reader)?;
        Ok(self)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, WindowPresentationDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<WindowPresentationCodec>(1)?;
        Ok(())
    }
    fn contribute(
        prepared: Self,
        mutations: &mut MutationBuilder<'_, WindowPresentationDomain>,
    ) -> Result<(), Self::Error> {
        match prepared.next {
            Some(value) => mutations.put::<WindowPresentationCodec>(&prepared.window, &value)?,
            None => mutations.delete::<WindowPresentationCodec>(&prepared.window)?,
        }
        Ok(())
    }
}

#[derive(Clone)]
struct WindowGuard {
    header: SessionHeader,
    window: SessionWindowRecord,
}

impl DomainValidator<SessionDomain> for WindowGuard {
    type Error = SessionMutationError;
    fn validate(&self, reader: &DomainReader<'_, SessionDomain>) -> Result<(), Self::Error> {
        if reader
            .point::<SessionHeaderCodec>(
                &HEADER_KEY,
                PointReadLimit::new(SESSION_HEADER_V1_BYTES + 4).unwrap(),
            )?
            .as_ref()
            != Some(&self.header)
            || reader
                .point::<SessionWindowCodec>(
                    &self.window.window_id,
                    PointReadLimit::new(SESSION_WINDOW_V1_BYTES + 4).unwrap(),
                )?
                .as_ref()
                != Some(&self.window)
        {
            return Err(SessionMutationError::InvalidCurrentState(
                "window presentation session source changed",
            ));
        }
        Ok(())
    }
}

impl SessionState {
    pub fn window_presentation(
        &self,
        home: &HomeStore,
        window: WindowId,
    ) -> Result<Option<WindowPresentationRecord>, ReadError> {
        checked_record(
            window,
            home.read_point::<WindowPresentationDomain, WindowPresentationCodec>(
                &self.presentation,
                &window,
                point_limit(),
            )?,
        )
    }

    pub fn window_presentation_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        window: WindowId,
    ) -> Result<Option<WindowPresentationRecord>, ReadError> {
        checked_record(
            window,
            access.read_point::<WindowPresentationDomain, WindowPresentationCodec>(
                &self.presentation,
                &window,
                point_limit(),
            )?,
        )
    }

    pub fn prepare_activity_panel_height(
        &self,
        home: &HomeStore,
        window: WindowId,
        height: ActivityPanelHeight,
    ) -> Result<PreparedWindowPresentationUpdate, SessionMutationError> {
        let before = home.home_revision()?;
        let domain = self.revision(home)?;
        let presentation_domain = home.domain_revision(&self.presentation)?;
        let header = home
            .read_point::<SessionDomain, SessionHeaderCodec>(
                &self.handle,
                &HEADER_KEY,
                PointReadLimit::new(SESSION_HEADER_V1_BYTES + 4).unwrap(),
            )?
            .ok_or(SessionMutationError::NotInitialized)?;
        if header.exit_intent != SessionExitIntent::Running {
            return Err(SessionMutationError::OrderlyExitInProgress);
        }
        let reference = header
            .windows
            .iter()
            .find(|reference| reference.window_id == window)
            .ok_or(SessionMutationError::WindowMissing { window_id: window })?;
        let record = home
            .read_point::<SessionDomain, SessionWindowCodec>(
                &self.handle,
                &window,
                PointReadLimit::new(SESSION_WINDOW_V1_BYTES + 4).unwrap(),
            )?
            .ok_or(SessionMutationError::WindowMissing { window_id: window })?;
        if record.window_id != window || record.revision != reference.record_revision {
            return Err(SessionMutationError::InvalidCurrentState(
                "window presentation membership is inconsistent",
            ));
        }
        let expected = self.window_presentation(home, window)?;
        if expected
            .as_ref()
            .is_some_and(|value| value.window != window)
        {
            return Err(SessionMutationError::InvalidCurrentState(
                "window presentation identity is inconsistent",
            ));
        }
        let revision = expected
            .as_ref()
            .map(|value| value.revision.checked_next())
            .transpose()?
            .unwrap_or(RecordRevision::INITIAL);
        let next = Some(WindowPresentationRecord {
            window,
            height,
            revision,
        });
        if home.home_revision()? != before {
            return Err(SessionMutationError::InvalidCurrentState(
                "window presentation source changed during preparation",
            ));
        }
        Ok(PreparedWindowMutation {
            session: Participant::Validation(self.handle.validation(
                domain,
                WindowGuard {
                    header,
                    window: record,
                },
            )),
            presentation: Participant::Mutation(self.presentation.contribution(
                presentation_domain,
                PresentationChange {
                    window,
                    expected,
                    next,
                },
            )),
        })
    }

    pub(super) fn presentation_bundle(
        &self,
        session: MutationContribution,
        revision: DomainRevision,
        window: WindowId,
        expected: Option<WindowPresentationRecord>,
        next: Option<WindowPresentationRecord>,
    ) -> PreparedWindowMutation {
        let change = PresentationChange {
            window,
            expected,
            next,
        };
        let presentation = if change.expected == change.next {
            Participant::Validation(self.presentation.validation(revision, change))
        } else {
            Participant::Mutation(self.presentation.contribution(revision, change))
        };
        PreparedWindowMutation {
            session: Participant::Mutation(session),
            presentation,
        }
    }
}

fn checked_record(
    window: WindowId,
    value: Option<WindowPresentationRecord>,
) -> Result<Option<WindowPresentationRecord>, ReadError> {
    if value.as_ref().is_some_and(|record| record.window != window) {
        Err(ReadError::Codec {
            domain: WindowPresentationDomain::NAME,
            family: WindowPresentationCodec::FAMILY,
            operation: beryl_home_store::CodecOperation::DecodeValue,
            source: Box::new(codec_error()),
        })
    } else {
        Ok(value)
    }
}
