struct Put<D, R> {
    key: u64,
    value: Vec<u8>,
    _typed: PhantomData<fn(D, R)>,
}

impl<D, R> Put<D, R> {
    fn new(key: u64, value: u64) -> Self {
        Self {
            key,
            value: value.to_be_bytes().to_vec(),
            _typed: PhantomData,
        }
    }
}

impl<D, R> DomainMutation<D> for Put<D, R>
where
    D: StorageDomain<ValidationError = TestError>,
    R: RecordCodec<D, Key = u64, Value = Vec<u8>>,
{
    type Error = TestError;
    fn validate(&self, _reader: &DomainReader<'_, D>) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, D>,
    ) -> Result<(), Self::Error> {
        reservation
            .reserve_records::<R>(1)
            .map_err(|error| TestError::Build(Box::new(error)))
    }
    fn contribute(
        &self,
        _reader: &DomainReader<'_, D>,
        mutations: &mut MutationBuilder<'_, D>,
    ) -> Result<(), Self::Error> {
        mutations
            .put::<R>(&self.key, &self.value)
            .map_err(|error| TestError::Build(Box::new(error)))
    }
}
struct SourcePut<S> {
    key: u64,
    value: u64,
    source: S,
}

struct NearLimitPut;

impl DomainMutation<SourceDomain> for NearLimitPut {
    type Error = TestError;
    fn validate(&self, _reader: &DomainReader<'_, SourceDomain>) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SourceDomain>,
    ) -> Result<(), Self::Error> {
        reservation
            .reserve_records::<SourceRecord>(1)
            .and_then(|()| {
                reservation
                    .reserve_successor_source::<NearLimitProtocol, NearLimitSource>(NearLimitSource)
            })
            .map_err(|error| TestError::Build(Box::new(error)))
    }
    fn contribute(
        &self,
        _reader: &DomainReader<'_, SourceDomain>,
        mutations: &mut MutationBuilder<'_, SourceDomain>,
    ) -> Result<(), Self::Error> {
        mutations
            .put::<SourceRecord>(&1, &1u64.to_be_bytes().to_vec())
            .map_err(|error| TestError::Build(Box::new(error)))
    }
}

struct AliasedSourcePut;

impl DomainMutation<SourceDomain> for AliasedSourcePut {
    type Error = TestError;
    fn validate(&self, _reader: &DomainReader<'_, SourceDomain>) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SourceDomain>,
    ) -> Result<(), Self::Error> {
        reservation
            .reserve_records::<SourceRecord>(1)
            .and_then(|()| {
                reservation
                    .reserve_successor_source::<AliasedProtocol, AliasedSource>(AliasedSource)
            })
            .map_err(|error| TestError::Build(Box::new(error)))
    }
    fn contribute(
        &self,
        _reader: &DomainReader<'_, SourceDomain>,
        mutations: &mut MutationBuilder<'_, SourceDomain>,
    ) -> Result<(), Self::Error> {
        mutations
            .put::<SourceRecord>(&1, &2u64.to_be_bytes().to_vec())
            .map_err(|error| TestError::Build(Box::new(error)))
    }
}

struct AliasedWitnessPut;

impl DomainMutation<WitnessDomain> for AliasedWitnessPut {
    type Error = TestError;
    fn validate(&self, _reader: &DomainReader<'_, WitnessDomain>) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, WitnessDomain>,
    ) -> Result<(), Self::Error> {
        reservation
            .reserve_records::<WitnessRecord>(1)
            .and_then(|()| {
                reservation
                    .reserve_successor_witness::<AliasedProtocol, AliasedWitness>(AliasedWitness)
            })
            .map_err(|error| TestError::Build(Box::new(error)))
    }
    fn contribute(
        &self,
        _reader: &DomainReader<'_, WitnessDomain>,
        mutations: &mut MutationBuilder<'_, WitnessDomain>,
    ) -> Result<(), Self::Error> {
        mutations
            .put::<WitnessRecord>(&7, &2u64.to_be_bytes().to_vec())
            .map_err(|error| TestError::Build(Box::new(error)))
    }
}

impl<S> DomainMutation<SourceDomain> for SourcePut<S>
where
    S: SuccessorSource<SourceDomain, Protocol>,
{
    type Error = TestError;
    fn validate(&self, _reader: &DomainReader<'_, SourceDomain>) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SourceDomain>,
    ) -> Result<(), Self::Error> {
        reservation
            .reserve_records::<SourceRecord>(1)
            .and_then(|()| reservation.reserve_successor_source::<Protocol, S>(self.source))
            .map_err(|error| TestError::Build(Box::new(error)))
    }
    fn contribute(
        &self,
        _reader: &DomainReader<'_, SourceDomain>,
        mutations: &mut MutationBuilder<'_, SourceDomain>,
    ) -> Result<(), Self::Error> {
        mutations
            .put::<SourceRecord>(&self.key, &self.value.to_be_bytes().to_vec())
            .map_err(|error| TestError::Build(Box::new(error)))
    }
}

struct WitnessPut<W> {
    key: u64,
    value: u64,
    witness: W,
}

impl<W> DomainMutation<WitnessDomain> for WitnessPut<W>
where
    W: SuccessorWitness<WitnessDomain, Protocol>,
{
    type Error = TestError;
    fn validate(&self, _reader: &DomainReader<'_, WitnessDomain>) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, WitnessDomain>,
    ) -> Result<(), Self::Error> {
        reservation
            .reserve_records::<WitnessRecord>(1)
            .and_then(|()| reservation.reserve_successor_witness::<Protocol, W>(self.witness))
            .map_err(|error| TestError::Build(Box::new(error)))
    }
    fn contribute(
        &self,
        _reader: &DomainReader<'_, WitnessDomain>,
        mutations: &mut MutationBuilder<'_, WitnessDomain>,
    ) -> Result<(), Self::Error> {
        mutations
            .put::<WitnessRecord>(&self.key, &self.value.to_be_bytes().to_vec())
            .map_err(|error| TestError::Build(Box::new(error)))
    }
}
