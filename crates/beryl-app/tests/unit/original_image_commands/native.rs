use super::*;

#[path = "support.rs"]
mod support;
use support::{wait_prepared, with_window};

#[test]
#[ignore = "requires an announced native GPUI qualification run"]
fn native_pending_picker_owns_slot_through_retirement_and_receipt() {
    with_window(|core, attachment, window, demand| {
        let command = window
            .request_original(demand, OriginalImageRequest::Save)
            .unwrap();
        assert!(matches!(
            window.request_original(demand, OriginalImageRequest::Availability),
            Err(ImageMediaError(ImageMediaFailure::Capacity))
        ));
        assert!(!has_work(&lock(&core.state)));
        assert_eq!(lock(&core.state).active_workers, 0);
        assert_eq!(attachment.retire(), ImageResourceDrain::Pending);
        assert_eq!(
            command.state(),
            OriginalImageState::Settled(Err(OriginalImageFailure::Cancelled))
        );
        assert!(lock(&core.state).original.is_some());
        command.picker_finished();
        assert!(lock(&core.state).original.is_none());
        assert_eq!(attachment.poll_drain(), ImageResourceDrain::Drained);
    });
}

#[test]
#[ignore = "requires an announced native GPUI qualification run"]
fn native_elected_copy_survives_retirement_until_exact_acknowledgement() {
    with_window(|core, attachment, window, demand| {
        let command = window
            .request_original(demand, OriginalImageRequest::Copy)
            .unwrap();
        let bytes = b"exact encoded original".to_vec();
        {
            let mut data = lock(&command.inner.data);
            data.encoded_bytes = bytes.capacity();
            data.encoded_length = bytes.len();
            data.copy = Some(ClipboardItem::from(gpui::Image::from_bytes(
                gpui::ImageFormat::Png,
                bytes,
            )));
            data.state = OriginalImageState::PreparedCopy;
        }
        assert!(command.take_copy().is_none());
        assert!(command.elect_copy());
        assert!(!command.elect_copy());
        assert_eq!(attachment.retire(), ImageResourceDrain::Pending);
        assert_eq!(command.state(), OriginalImageState::Publishing);
        let effect = command.take_copy().unwrap();
        assert_eq!(
            effect.publish(|item, _| {
                let [gpui::ClipboardEntry::Image(image)] = item.entries() else {
                    panic!("one original image")
                };
                assert_eq!(image.bytes(), b"exact encoded original");
                Err(gpui::ClipboardError::Ownership)
            }),
            Err(OriginalImageFailure::Clipboard)
        );
        assert_eq!(
            command.state(),
            OriginalImageState::Settled(Err(OriginalImageFailure::Clipboard))
        );
        assert!(lock(&core.state).original.is_none());
        assert_eq!(attachment.poll_drain(), ImageResourceDrain::Drained);
    });
}

#[test]
#[ignore = "requires an announced native GPUI qualification run"]
fn native_cancelled_preparation_cannot_elect_and_fresh_work_authenticates_again() {
    with_window(|core, attachment, window, demand| {
        let command = window
            .request_original(demand, OriginalImageRequest::Copy)
            .unwrap();
        command.cancel();
        assert!(!command.elect_copy());
        assert!(command.take_copy().is_none());
        let command = window
            .request_original(demand, OriginalImageRequest::Availability)
            .unwrap();
        let (operation, owner) = claim(&core).unwrap();
        let worker_core = core.clone();
        std::thread::spawn(move || run(&worker_core, operation, owner))
            .join()
            .unwrap();
        assert_eq!(
            command.state(),
            OriginalImageState::Available(OriginalImageAvailability {
                copy: Ok(()),
                save: true
            })
        );
        assert!(lock(&core.state).original.is_none());
        assert_eq!(lock(&core.state).path_slots, 0);
        assert_eq!(lock(&core.state).verification_slots, 0);
        assert_eq!(attachment.inner.active_workers.load(Ordering::Acquire), 0);
    });
}

#[test]
#[ignore = "requires an announced native GPUI qualification run"]
fn native_worker_moves_exact_encoded_original_without_rendition_or_clipboard_access() {
    with_window(|core, attachment, window, demand| {
        let command = window
            .request_original(demand, OriginalImageRequest::Copy)
            .unwrap();
        let (operation, owner) = claim(&core).unwrap();
        let worker_core = core.clone();
        std::thread::spawn(move || run(&worker_core, operation, owner))
            .join()
            .unwrap();
        assert_eq!(command.state(), OriginalImageState::PreparedCopy);
        assert_eq!(lock(&core.state).path_slots, 0);
        assert_eq!(lock(&core.state).verification_slots, 0);
        assert_eq!(lock(&core.state).active_workers, 0);
        assert!(lock(&core.state).original.is_some());
        assert!(command.elect_copy());
        let effect = command.take_copy().unwrap();
        assert_eq!(
            effect.publish(|item, _| {
                let [gpui::ClipboardEntry::Image(image)] = item.entries() else {
                    panic!("one image")
                };
                assert_eq!(image.format(), gpui::ImageFormat::Gif);
                assert_eq!(
                    image.bytes(),
                    b"original GIF bytes without a supported rendition decoder"
                );
                Ok(())
            }),
            Ok(())
        );
        assert_eq!(command.state(), OriginalImageState::Settled(Ok(())));
        assert!(lock(&core.state).original.is_none());
        assert_eq!(attachment.inner.active_workers.load(Ordering::Acquire), 0);
    });
}

#[test]
#[ignore = "requires an announced native GPUI qualification run"]
fn native_transferred_copy_effect_keeps_charge_after_command_drop_and_retirement() {
    with_window(|core, attachment, window, demand| {
        let command = window
            .request_original(demand, OriginalImageRequest::Copy)
            .unwrap();
        let (operation, owner) = claim(&core).unwrap();
        let worker_core = core.clone();
        std::thread::spawn(move || run(&worker_core, operation, owner))
            .join()
            .unwrap();
        assert!(command.elect_copy());
        let effect = command.take_copy().unwrap();
        let operation = command.inner.clone();
        drop(command);
        assert!(lock(&core.state).original.is_some());
        assert!(lock(&operation.data).encoded_bytes > 0);
        assert_eq!(attachment.retire(), ImageResourceDrain::Pending);
        assert_eq!(attachment.poll_drain(), ImageResourceDrain::Pending);
        let calls = AtomicUsize::new(0);
        assert_eq!(
            effect.publish(|_, _| {
                calls.fetch_add(1, Ordering::AcqRel);
                Ok(())
            }),
            Ok(())
        );
        assert_eq!(calls.load(Ordering::Acquire), 1);
        assert_eq!(
            lock(&operation.data).state,
            OriginalImageState::Settled(Ok(()))
        );
        assert_eq!(lock(&operation.data).encoded_bytes, 0);
        assert!(lock(&core.state).original.is_none());
        assert_eq!(attachment.poll_drain(), ImageResourceDrain::Drained);
    });
}

#[test]
#[ignore = "requires an announced native GPUI qualification run"]
fn native_dropping_uninvoked_copy_owner_or_untransferred_command_reclaims_output() {
    with_window(|core, _attachment, window, demand| {
        for transfer in [false, true] {
            let command = window
                .request_original(demand, OriginalImageRequest::Copy)
                .unwrap();
            let (operation, owner) = claim(&core).unwrap();
            let worker_core = core.clone();
            std::thread::spawn(move || run(&worker_core, operation, owner))
                .join()
                .unwrap();
            let operation = command.inner.clone();
            assert!(command.elect_copy());
            let effect = if transfer { command.take_copy() } else { None };
            drop(command);
            if transfer {
                assert!(lock(&core.state).original.is_some());
                assert!(lock(&operation.data).encoded_bytes > 0);
                drop(effect);
            }
            assert_eq!(
                lock(&operation.data).state,
                OriginalImageState::Settled(Err(OriginalImageFailure::Cancelled))
            );
            assert_eq!(lock(&operation.data).encoded_bytes, 0);
            assert!(lock(&core.state).original.is_none());
        }
    });
}

#[test]
#[ignore = "requires an announced native GPUI qualification run"]
fn native_save_streams_exact_original_after_election_and_cleans_cancelled_stage() {
    with_window(|core, attachment, window, demand| {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("original.gif");
        std::fs::write(&destination, b"existing destination").unwrap();
        for publish in [false, true] {
            let command = window
                .request_original(demand, OriginalImageRequest::Save)
                .unwrap();
            assert!(command.submit_save_choice(SavePathChoice {
                path: destination.clone(),
                replace_existing: true
            }));
            let (operation, owner) = claim(&core).unwrap();
            let worker_core = core.clone();
            let worker = std::thread::spawn(move || run(&worker_core, operation, owner));
            wait_prepared(&command);
            assert_eq!(lock(&core.state).path_slots, 0);
            assert_eq!(lock(&core.state).verification_slots, 0);
            assert_eq!(lock(&core.state).active_workers, 1);
            assert_eq!(
                std::fs::read(&destination).unwrap(),
                b"existing destination"
            );
            if publish {
                assert!(command.elect_save());
            } else {
                command.cancel();
            }
            worker.join().unwrap();
            assert_eq!(
                command.state(),
                OriginalImageState::Settled(if publish {
                    Ok(())
                } else {
                    Err(OriginalImageFailure::Cancelled)
                })
            );
            assert!(lock(&core.state).original.is_none());
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
            assert_eq!(
                std::fs::read(&destination).unwrap(),
                if publish {
                    b"original GIF bytes without a supported rendition decoder".as_slice()
                } else {
                    b"existing destination".as_slice()
                }
            );
        }
        assert_eq!(attachment.inner.active_workers.load(Ordering::Acquire), 0);
    });
}

#[test]
#[ignore = "requires an announced native GPUI qualification run"]
fn native_save_without_overwrite_authority_preserves_existing_destination_on_failure() {
    with_window(|core, _attachment, window, demand| {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("existing.gif");
        std::fs::write(&destination, b"existing destination").unwrap();
        let command = window
            .request_original(demand, OriginalImageRequest::Save)
            .unwrap();
        assert!(command.submit_save_choice(SavePathChoice {
            path: destination.clone(),
            replace_existing: false
        }));
        let (operation, owner) = claim(&core).unwrap();
        let worker_core = core.clone();
        let worker = std::thread::spawn(move || run(&worker_core, operation, owner));
        wait_prepared(&command);
        assert!(command.elect_save());
        worker.join().unwrap();
        assert_eq!(
            command.state(),
            OriginalImageState::Settled(Err(OriginalImageFailure::Publish))
        );
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"existing destination"
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        assert!(lock(&core.state).original.is_none());
    });
}
