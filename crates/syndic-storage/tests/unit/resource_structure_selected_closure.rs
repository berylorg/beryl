use super::*;
use crate::{SyndicStorage, indexed_resource as api};
use beryl_home_store::{
    DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation, RecordCodec,
};

struct Replace<R: RecordCodec<SyndicDomain>> {
    key: R::Key,
    value: Option<R::Value>,
}

impl<R: RecordCodec<SyndicDomain>> DomainMutation<SyndicDomain> for Replace<R>
where
    R::Error: Into<crate::codec::CodecError>,
{
    type Error = ConstructionError;
    type Prepared = Self;

    fn prepare(self, _: &DomainReader<'_, SyndicDomain>) -> Result<Self, Self::Error> {
        if let Some(value) = &self.value {
            let encoded = R::encode_value(value).map_err(Into::into)?;
            let decoded = R::decode_value(&encoded).map_err(Into::into)?;
            assert_eq!(encoded, R::encode_value(&decoded).map_err(Into::into)?);
        }
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<R>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match prepared.value {
            Some(value) => mutations.put::<R>(&prepared.key, &value)?,
            None => mutations.delete::<R>(&prepared.key)?,
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
enum Member {
    Head,
    Root,
    Build,
    Receipt,
}

fn replace_member(
    store: &HomeStore,
    handle: &DomainHandle<SyndicDomain>,
    member: Member,
    missing: bool,
) {
    match member {
        Member::Head => {
            let mut value = read::<HeadsCodec>(store, handle, &key().resource).unwrap();
            value.root[0] ^= 1;
            value.digest = head_digest(&value).unwrap();
            committed(
                store.execute_current(handle.current_command(Replace::<HeadsCodec> {
                    key: key().resource,
                    value: (!missing).then_some(value),
                })),
            );
        }
        Member::Root => {
            let mut value = read::<RootsCodec>(store, handle, &key()).unwrap();
            value.chain[0] ^= 1;
            value.digest = root_digest(&value).unwrap();
            committed(
                store.execute_current(handle.current_command(Replace::<RootsCodec> {
                    key: key(),
                    value: (!missing).then_some(value),
                })),
            );
        }
        Member::Build => {
            let mut value = read::<BuildsCodec>(store, handle, &key()).unwrap();
            value.chain[0] ^= 1;
            committed(
                store.execute_current(handle.current_command(Replace::<BuildsCodec> {
                    key: key(),
                    value: (!missing).then_some(value),
                })),
            );
        }
        Member::Receipt => {
            let progress = read::<BuildsCodec>(store, handle, &key())
                .unwrap()
                .progress
                .unwrap();
            let mut value = read::<ProgressCodec>(store, handle, &progress.key).unwrap();
            value.after_digest[0] ^= 1;
            value.digest = progress_digest(&value).unwrap();
            committed(
                store.execute_current(handle.current_command(Replace::<ProgressCodec> {
                    key: progress.key,
                    value: (!missing).then_some(value),
                })),
            );
        }
    }
}

fn refused<T>(result: Result<api::IndexedResourceResult<T>, api::IndexedResourceError>) {
    assert!(matches!(
        result,
        Err(api::IndexedResourceError::Corrupt
            | api::IndexedResourceError::Collision
            | api::IndexedResourceError::Unavailable)
    ));
}

fn exercise(binding: Binding, bytes: &[u8], range: api::Request, display: api::DisplayRequest) {
    for member in [Member::Head, Member::Root, Member::Build, Member::Receipt] {
        for missing in [false, true] {
            let home = TestHome::new();
            let faults = FaultController::new();
            let (store, handle) = open(&home, &faults);
            let storage = SyndicStorage::reacquire(&store).unwrap();
            committed(store.execute_current(handle.current_command(Seed {
                binding: binding.clone(),
                bytes: bytes.to_vec(),
            })));
            complete(&store, &handle, &binding);
            let old = store
                .capture_frozen_read(&CommandCancellation::new())
                .unwrap();
            let api::IndexedResourceResult::Ready { root, .. } = storage
                .read_frozen_indexed_resource_display(
                    &store, &old, &binding, None, display, None, 65_536,
                )
                .unwrap()
            else {
                panic!("completed selection is pending");
            };
            replace_member(&store, &handle, member, missing);
            // A retained snapshot still owns the original complete selection.
            assert!(matches!(
                storage
                    .read_frozen_indexed_resource_display(
                        &store,
                        &old,
                        &binding,
                        Some(root.digest),
                        display,
                        None,
                        65_536,
                    )
                    .unwrap(),
                api::IndexedResourceResult::Ready { .. }
            ));
            store.release_frozen_read(&old).unwrap();
            store.close().unwrap();

            let (store, _) = open(&home, &faults);
            let storage = SyndicStorage::reacquire(&store).unwrap();
            let fresh = store
                .capture_frozen_read(&CommandCancellation::new())
                .unwrap();
            refused(storage.read_frozen_indexed_resource_range(
                &store,
                &fresh,
                &binding,
                Some(root.digest),
                range,
                None,
                api::ResponseLimits {
                    descriptors: 8,
                    bytes: 65_536,
                },
            ));
            refused(storage.read_frozen_indexed_resource_display(
                &store,
                &fresh,
                &binding,
                Some(root.digest),
                display,
                None,
                65_536,
            ));
            refused(storage.read_frozen_indexed_resource_anchor(
                &store,
                &fresh,
                &binding,
                Some(root.digest),
                api::SourceAnchor {
                    source: binding.source,
                    block_start: binding.range.start,
                    kind: binding.metadata.kind(),
                    position: binding.range.start,
                    visual_offset: 7,
                },
                65_536,
            ));
            assert_eq!(store.retained_frozen_read_count().unwrap(), 1);
            store.release_frozen_read(&fresh).unwrap();
            assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
            assert_eq!(
                read::<crate::codec::ResourcesCodec>(&store, &storage.handle, &binding.resource.id,),
                Some(binding.metadata.clone())
            );
            store.close().unwrap();
        }
    }
}

#[test]
fn public_code_reads_reject_canonical_mixed_and_missing_closure_after_reopen() {
    let bytes = b"one\nthree\n";
    exercise(
        binding(),
        bytes,
        api::Request::Code {
            lines: api::OrdinalRange { start: 0, end: 2 },
        },
        api::DisplayRequest {
            target: api::Target::Code(0),
            offset: 0,
        },
    );
}

#[test]
fn public_table_reads_reject_canonical_mixed_and_missing_closure_after_reopen() {
    let bytes = b"a|b\n---|---:\nc||\nx|y|z\n";
    exercise(
        fixture_support::table_build(bytes, 2, 2).binding,
        bytes,
        api::Request::Table {
            role: RowRole::Body,
            rows: api::OrdinalRange { start: 0, end: 2 },
            columns: api::OrdinalRange { start: 0, end: 2 },
        },
        api::DisplayRequest {
            target: api::Target::Cell {
                role: RowRole::Body,
                row: 0,
                column: 0,
            },
            offset: 0,
        },
    );
}
