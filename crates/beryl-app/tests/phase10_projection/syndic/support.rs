use beryl_home_store::{HomeCommand, HomeStore};
use beryl_model::SyndicItemId;
use syndic_storage::*;

use super::point_limit;

pub(super) fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).unwrap();
}

pub(super) fn stage_prepared_content(
    store: &HomeStore,
    storage: SyndicStorage,
    content: &PreparedContent,
) {
    execute(
        store,
        storage.begin_content(
            storage.revision(store).unwrap(),
            ContentBuild::from_prepared(content),
        ),
    );
    let mut manifest = content.building_manifest();
    while let Some(append) = ContentAppend::prepare(&manifest, content).unwrap() {
        manifest = append.next_manifest().clone();
        execute(
            store,
            storage.append_content(storage.revision(store).unwrap(), append),
        );
    }
}

pub(super) fn project_item(store: &HomeStore, storage: SyndicStorage, item: SyndicItemId) {
    let canonical = storage
        .canonical_item(store, item, point_limit())
        .unwrap()
        .unwrap();
    let generation = ItemProjectionGeneration::FIRST;
    execute(
        store,
        storage.start_item_projection_build(
            storage.revision(store).unwrap(),
            StartItemProjectionBuild::new(item, canonical.record().revision(), generation),
        ),
    );
    for _ in 0..4_096 {
        if storage
            .item_projection_set(store, item, generation, point_limit())
            .unwrap()
            .is_some()
        {
            return;
        }
        let build = storage
            .item_projection_build(store, item, generation, point_limit())
            .unwrap()
            .unwrap();
        execute(
            store,
            storage.advance_item_projection_build(
                storage.revision(store).unwrap(),
                AdvanceItemProjectionBuild::new(item, generation, build.record().revision()),
            ),
        );
    }
    panic!("fixture item projection did not converge")
}
