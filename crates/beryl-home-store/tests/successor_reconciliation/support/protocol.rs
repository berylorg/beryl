#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Correlation(u64);

impl SuccessorCorrelation for Correlation {
    const ENCODED_BYTES: usize = 8;
    fn encode(&self, output: &mut [u8]) {
        output.copy_from_slice(&self.0.to_be_bytes());
    }
}

struct Protocol;

impl SuccessorProtocol for Protocol {
    const NAME: &'static str = "test-successor";
    type Correlation = Correlation;
}

#[derive(Clone, Copy)]
struct SourceHook;

impl SuccessorSource<SourceDomain, Protocol> for SourceHook {
    const MAX_RETAINED_BYTES: usize = 1;
    fn authenticate(
        &self,
        reader: &ReconciliationReader<'_, SourceDomain>,
    ) -> Result<SuccessorObservation<Correlation>, TestError> {
        SOURCE_CALLS.fetch_add(1, Ordering::SeqCst);
        while BLOCK_SOURCE.load(Ordering::SeqCst) && !RELEASE_SOURCE.load(Ordering::SeqCst) {
            thread::yield_now();
        }
        if FAIL_SOURCE.load(Ordering::SeqCst) {
            return Err(TestError::Read(ReadError::Storage {
                stage: ReadStage::PointValue,
                source: Box::new(std::io::Error::other("successor source failure")),
            }));
        }
        let records = reader.records::<SourceRecord>()?;
        let Some(value) = records.first().and_then(|record| record.current()) else {
            return Ok(SuccessorObservation::Unresolved);
        };
        let Ok(bytes) = <[u8; 8]>::try_from(value.as_slice()) else {
            return Ok(SuccessorObservation::Collision);
        };
        Ok(SuccessorObservation::Authenticated(Correlation(
            u64::from_be_bytes(bytes),
        )))
    }
}

struct DerivedWitnessRead;

impl SuccessorPointRead<WitnessDomain, Protocol> for DerivedWitnessRead {
    type Record = WitnessRecord;
    const MAX_DECODED_BYTES: usize = 64;
    fn derive_key(correlation: &Correlation, _ordinal: usize) -> u64 {
        correlation.0
    }
    fn expected_value(correlation: &Correlation, _ordinal: usize) -> Vec<u8> {
        correlation.0.to_be_bytes().to_vec()
    }
}

#[derive(Clone, Copy)]
struct WitnessHook {
    reads: usize,
}

impl SuccessorWitness<WitnessDomain, Protocol> for WitnessHook {
    const MAX_RETAINED_BYTES: usize = 8;
    fn reserve_reads(
        &self,
        reservation: &mut SuccessorReadReservation<'_, WitnessDomain, Protocol>,
    ) -> Result<(), beryl_home_store::MutationBuildError> {
        reservation.reserve::<DerivedWitnessRead>(1)
    }
    fn authenticate(
        &self,
        reader: &mut SuccessorPointReader<'_, WitnessDomain, Protocol>,
    ) -> Result<SuccessorObservation<Correlation>, TestError> {
        let mut observed = None;
        for _ in 0..self.reads {
            match reader.read::<DerivedWitnessRead>()? {
                SuccessorPointRecord::Present(value) => {
                    let Ok(bytes) = <[u8; 8]>::try_from(value.as_slice()) else {
                        return Ok(SuccessorObservation::Collision);
                    };
                    observed = Some(Correlation(u64::from_be_bytes(bytes)));
                }
                SuccessorPointRecord::Absent => return Ok(SuccessorObservation::Unresolved),
                SuccessorPointRecord::Rejected(_) => {
                    return Ok(SuccessorObservation::Collision);
                }
            }
        }
        Ok(observed
            .map(SuccessorObservation::Authenticated)
            .unwrap_or(SuccessorObservation::Unresolved))
    }
}

struct OversizedExpectedRead;

impl SuccessorPointRead<WitnessDomain, Protocol> for OversizedExpectedRead {
    type Record = WitnessRecord;
    const MAX_DECODED_BYTES: usize = 8;
    fn derive_key(correlation: &Correlation, _ordinal: usize) -> u64 {
        correlation.0
    }
    fn expected_value(correlation: &Correlation, _ordinal: usize) -> Vec<u8> {
        correlation.0.to_be_bytes().to_vec()
    }
}

#[derive(Clone, Copy)]
struct OversizedExpectedWitness;

impl SuccessorWitness<WitnessDomain, Protocol> for OversizedExpectedWitness {
    const MAX_RETAINED_BYTES: usize = 1;
    fn reserve_reads(
        &self,
        reservation: &mut SuccessorReadReservation<'_, WitnessDomain, Protocol>,
    ) -> Result<(), beryl_home_store::MutationBuildError> {
        reservation.reserve::<OversizedExpectedRead>(1)
    }
    fn authenticate(
        &self,
        reader: &mut SuccessorPointReader<'_, WitnessDomain, Protocol>,
    ) -> Result<SuccessorObservation<Correlation>, TestError> {
        match reader.read::<OversizedExpectedRead>()? {
            SuccessorPointRecord::Rejected(_) => {
                OVERSIZED_EXPECTED_REJECTIONS.fetch_add(1, Ordering::SeqCst);
                Ok(SuccessorObservation::Collision)
            }
            _ => Ok(SuccessorObservation::Authenticated(*reader.correlation())),
        }
    }
}

struct InvalidDerivedKeyRead;

impl SuccessorPointRead<WitnessDomain, Protocol> for InvalidDerivedKeyRead {
    type Record = WitnessRecord;
    const MAX_DECODED_BYTES: usize = 64;
    fn derive_key(_correlation: &Correlation, _ordinal: usize) -> u64 {
        INVALID_DERIVED_KEY
    }
    fn expected_value(correlation: &Correlation, _ordinal: usize) -> Vec<u8> {
        correlation.0.to_be_bytes().to_vec()
    }
}

struct OversizedDerivedKeyRead;

impl SuccessorPointRead<WitnessDomain, Protocol> for OversizedDerivedKeyRead {
    type Record = WitnessRecord;
    const MAX_DECODED_BYTES: usize = 64;
    fn derive_key(_correlation: &Correlation, _ordinal: usize) -> u64 {
        OVERSIZED_DERIVED_KEY
    }
    fn expected_value(correlation: &Correlation, _ordinal: usize) -> Vec<u8> {
        correlation.0.to_be_bytes().to_vec()
    }
}

struct InvalidExpectedRead;

impl SuccessorPointRead<WitnessDomain, Protocol> for InvalidExpectedRead {
    type Record = WitnessRecord;
    const MAX_DECODED_BYTES: usize = 64;
    fn derive_key(correlation: &Correlation, _ordinal: usize) -> u64 {
        correlation.0
    }
    fn expected_value(_correlation: &Correlation, _ordinal: usize) -> Vec<u8> {
        vec![0xfe]
    }
}

struct OversizedExpectedEncodingRead;

impl SuccessorPointRead<WitnessDomain, Protocol> for OversizedExpectedEncodingRead {
    type Record = WitnessRecord;
    const MAX_DECODED_BYTES: usize = 64;
    fn derive_key(correlation: &Correlation, _ordinal: usize) -> u64 {
        correlation.0
    }
    fn expected_value(_correlation: &Correlation, _ordinal: usize) -> Vec<u8> {
        vec![0xfd; WitnessRecord::MAX_VALUE_BYTES + 1]
    }
}

struct RejectionWitness<Q>(PhantomData<fn() -> Q>);

impl<Q> Copy for RejectionWitness<Q> {}

impl<Q> Clone for RejectionWitness<Q> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Q> SuccessorWitness<WitnessDomain, Protocol> for RejectionWitness<Q>
where
    Q: SuccessorPointRead<WitnessDomain, Protocol>,
{
    const MAX_RETAINED_BYTES: usize = 1;
    fn reserve_reads(
        &self,
        reservation: &mut SuccessorReadReservation<'_, WitnessDomain, Protocol>,
    ) -> Result<(), MutationBuildError> {
        reservation.reserve::<Q>(1)
    }
    fn authenticate(
        &self,
        reader: &mut SuccessorPointReader<'_, WitnessDomain, Protocol>,
    ) -> Result<SuccessorObservation<Correlation>, TestError> {
        match reader.read::<Q>()? {
            SuccessorPointRecord::Rejected(_) => Ok(SuccessorObservation::Collision),
            _ => Ok(SuccessorObservation::Authenticated(*reader.correlation())),
        }
    }
}

#[derive(Clone, Copy)]
struct NoReadReservationWitness;

impl SuccessorWitness<WitnessDomain, Protocol> for NoReadReservationWitness {
    const MAX_RETAINED_BYTES: usize = 1;
    fn reserve_reads(
        &self,
        _reservation: &mut SuccessorReadReservation<'_, WitnessDomain, Protocol>,
    ) -> Result<(), MutationBuildError> {
        Ok(())
    }
    fn authenticate(
        &self,
        reader: &mut SuccessorPointReader<'_, WitnessDomain, Protocol>,
    ) -> Result<SuccessorObservation<Correlation>, TestError> {
        Ok(SuccessorObservation::Authenticated(*reader.correlation()))
    }
}

#[derive(Clone, Copy)]
struct NoConsumptionWitness;

impl SuccessorWitness<WitnessDomain, Protocol> for NoConsumptionWitness {
    const MAX_RETAINED_BYTES: usize = 1;
    fn reserve_reads(
        &self,
        reservation: &mut SuccessorReadReservation<'_, WitnessDomain, Protocol>,
    ) -> Result<(), MutationBuildError> {
        reservation.reserve::<DerivedWitnessRead>(1)
    }
    fn authenticate(
        &self,
        reader: &mut SuccessorPointReader<'_, WitnessDomain, Protocol>,
    ) -> Result<SuccessorObservation<Correlation>, TestError> {
        Ok(SuccessorObservation::Authenticated(*reader.correlation()))
    }
}

#[derive(Clone, Copy)]
struct HugeSource;

impl SuccessorSource<SourceDomain, Protocol> for HugeSource {
    const MAX_RETAINED_BYTES: usize = 65 * 1024 * 1024;
    fn authenticate(
        &self,
        _reader: &ReconciliationReader<'_, SourceDomain>,
    ) -> Result<SuccessorObservation<Correlation>, TestError> {
        Ok(SuccessorObservation::Unresolved)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NearLimitCorrelation(u8);

impl SuccessorCorrelation for NearLimitCorrelation {
    const ENCODED_BYTES: usize = 16 * 1024 * 1024;
    fn encode(&self, output: &mut [u8]) {
        output.fill(self.0);
    }
}

struct NearLimitProtocol;

impl SuccessorProtocol for NearLimitProtocol {
    const NAME: &'static str = "near-limit-successor";
    type Correlation = NearLimitCorrelation;
}

#[derive(Clone, Copy)]
struct NearLimitSource;

impl SuccessorSource<SourceDomain, NearLimitProtocol> for NearLimitSource {
    const MAX_RETAINED_BYTES: usize = 1;
    fn authenticate(
        &self,
        _reader: &ReconciliationReader<'_, SourceDomain>,
    ) -> Result<SuccessorObservation<NearLimitCorrelation>, TestError> {
        Ok(SuccessorObservation::Unresolved)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AliasedCorrelation(u64);

impl SuccessorCorrelation for AliasedCorrelation {
    const ENCODED_BYTES: usize = 8;
    fn encode(&self, output: &mut [u8]) {
        output.fill(0);
    }
}

struct AliasedProtocol;

impl SuccessorProtocol for AliasedProtocol {
    const NAME: &'static str = "aliased-successor";
    type Correlation = AliasedCorrelation;
}

#[derive(Clone, Copy)]
struct AliasedSource;

impl SuccessorSource<SourceDomain, AliasedProtocol> for AliasedSource {
    const MAX_RETAINED_BYTES: usize = 1;
    fn authenticate(
        &self,
        _reader: &ReconciliationReader<'_, SourceDomain>,
    ) -> Result<SuccessorObservation<AliasedCorrelation>, TestError> {
        Ok(SuccessorObservation::Authenticated(AliasedCorrelation(42)))
    }
}

#[derive(Clone, Copy)]
struct AliasedWitness;

struct AliasedRead;

impl SuccessorPointRead<WitnessDomain, AliasedProtocol> for AliasedRead {
    type Record = WitnessRecord;
    const MAX_DECODED_BYTES: usize = 64;
    fn derive_key(_correlation: &AliasedCorrelation, _ordinal: usize) -> u64 {
        7
    }
    fn expected_value(_correlation: &AliasedCorrelation, _ordinal: usize) -> Vec<u8> {
        1_u64.to_be_bytes().to_vec()
    }
}

impl SuccessorWitness<WitnessDomain, AliasedProtocol> for AliasedWitness {
    const MAX_RETAINED_BYTES: usize = 1;
    fn reserve_reads(
        &self,
        reservation: &mut SuccessorReadReservation<'_, WitnessDomain, AliasedProtocol>,
    ) -> Result<(), beryl_home_store::MutationBuildError> {
        reservation.reserve::<AliasedRead>(1)
    }
    fn authenticate(
        &self,
        reader: &mut SuccessorPointReader<'_, WitnessDomain, AliasedProtocol>,
    ) -> Result<SuccessorObservation<AliasedCorrelation>, TestError> {
        let _ = reader.read::<AliasedRead>()?;
        Ok(SuccessorObservation::Authenticated(AliasedCorrelation(43)))
    }
}
