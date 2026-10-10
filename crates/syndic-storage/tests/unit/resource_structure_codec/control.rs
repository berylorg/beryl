use super::*;

#[test]
fn encoded_build_restores_every_row_pass_utf8_and_grammar_frontier() {
    let source = "|α|🦀|\r\n|:---|---:|\r\n|a\\|b|`c|d`| \u{a0}\r\n";
    let header = range(0, "|α|🦀|\r\n".len() as u64);
    let mut value = build();
    value.binding.range = range(5, 5 + source.len() as u64);
    value.binding.metadata = ResourceMetadataRecord::new(
        key().resource.id,
        key().resource.revision,
        SyndicProjectionId::from_bytes([0x55; 16]),
        SyndicItemId::from_bytes([0x66; 16]),
        ResourceOrdinal::FIRST,
        ResourceKind::Table,
        "text/plain",
        ResourceBacking::TextRange {
            source: value.binding.source,
            range: ProjectionSourceRange::new(value.binding.range.start, value.binding.range.end)
                .unwrap(),
        },
        [0x77; 32],
        None,
        ResourceStructure::table(
            1,
            2,
            ProjectionSourceRange::new(header.start, header.end).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    value.binding_digest = binding_digest(&value.binding).unwrap();
    value.chain = empty_chain(value.key, &value.binding_digest);
    value.checkpoint.pass = Pass::TableRows;
    let mut descriptors = Vec::new();
    while value.checkpoint.phase != ParsePhase::Eof {
        let start = value.checkpoint.next_byte as usize;
        let step = advance_rows(
            &value.checkpoint,
            &source.as_bytes()[start..start + 1],
            source.len() as u64,
            header,
            2,
        )
        .unwrap();
        value.checkpoint = step.checkpoint;
        value.counts[1] += step.descriptors.len() as u64;
        descriptors.extend(step.descriptors);
        value.revision += 1;
        value.transition += 1;
        value.progress = Some(Reference {
            key: SequenceKey {
                build: value.key,
                ordinal: value.transition,
            },
            digest: [0xAA; 32],
        });
        let encoded = encode_build(&value).unwrap();
        let restored = decode_build(&encoded).unwrap();
        assert_eq!(value, restored);
        value = restored;
    }
    assert_eq!(descriptors.len(), 2);
    assert_eq!(value.counts[1], 2);
    assert_eq!(value.checkpoint.next_row, 1);
}

#[test]
fn build_digest_omits_only_selected_progress_digest_and_genesis_is_exact() {
    let mut value = build();
    assert_eq!(decode_build(&encode_build(&value).unwrap()).unwrap(), value);
    value.revision = 2;
    value.transition = 1;
    value.progress = Some(Reference {
        key: SequenceKey {
            build: key(),
            ordinal: 1,
        },
        digest: [0xAA; 32],
    });
    value.checkpoint.next_byte = 3;
    let first = build_digest(&value).unwrap();
    let first_bytes = encode_build(&value).unwrap();
    value.progress.as_mut().unwrap().digest = [0xBB; 32];
    assert_eq!(build_digest(&value).unwrap(), first);
    assert_ne!(encode_build(&value).unwrap(), first_bytes);
    value.checkpoint.next_byte = 4;
    assert_ne!(build_digest(&value).unwrap(), first);
    value = build();
    value.checkpoint.next_byte = 1;
    assert!(encode_build(&value).is_err());
    value = build();
    value.revision = u64::MAX;
    value.transition = u64::MAX;
    assert!(encode_build(&value).is_err());
}

#[test]
fn checkpoint_carry_accepts_only_incomplete_scalar_and_rejects_overflow() {
    let mut value = build().checkpoint;
    value.next_byte = 4;
    for carry in [vec![], vec![0xF0], vec![0xF0, 0x9F], vec![0xF0, 0x9F, 0x92]] {
        value.utf8_carry = carry;
        assert!(validate_checkpoint(&value, 10).is_ok());
    }
    for carry in [
        vec![b'a'],
        vec![0x80],
        vec![0xFF],
        vec![0xF0, 0x9F, 0x92, 0xA9],
        vec![0xE0, 0x80],
    ] {
        value.utf8_carry = carry;
        assert!(validate_checkpoint(&value, 10).is_err());
    }
    value.utf8_carry.clear();
    value.pass = Pass::TableRows;
    value.next_byte = u64::MAX;
    value.syntax_start = u64::MAX;
    value.syntax = vec![b'x'];
    assert!(validate_checkpoint(&value, u64::MAX).is_err());
}

#[test]
fn completion_progress_root_and_head_are_acyclic_canonical_records() {
    let root = root();
    let mut progress = Progress {
        key: root.progress,
        command: Command::Complete,
        prior: Some(Reference {
            key: SequenceKey {
                build: key(),
                ordinal: 2,
            },
            digest: [0xAA; 32],
        }),
        before_revision: 3,
        after_revision: 4,
        before_digest: [0xBB; 32],
        after_digest: [0xCC; 32],
        page: None,
        root: Some(root.digest),
        digest: [0; 32],
    };
    progress.digest = progress_digest(&progress).unwrap();
    let mut head = Head {
        key: key().resource,
        binding_digest: root.binding_digest,
        build: key(),
        root: root.digest,
        digest: [0; 32],
    };
    head.digest = head_digest(&head).unwrap();
    let bytes = encode_head(&head).unwrap();
    assert_eq!(bytes.len(), 168);
    assert_eq!(&bytes[24..32], &1_u64.to_be_bytes());
    assert_eq!(decode_head(&bytes).unwrap(), head);
    assert_eq!(decode_root(&encode_root(&root).unwrap()).unwrap(), root);
    assert_eq!(
        decode_progress(&encode_progress(&progress).unwrap()).unwrap(),
        progress
    );
    let mut altered = bytes;
    altered[31] = 2;
    assert!(decode_head(&altered).is_err());
    progress.root = None;
    assert!(progress_digest(&progress).is_err());
}

#[test]
fn terminal_commands_cannot_carry_output_or_inconsistent_prior_frontier() {
    let mut progress = Progress {
        key: SequenceKey {
            build: key(),
            ordinal: 1,
        },
        command: Command::Cancel,
        prior: None,
        before_revision: 1,
        after_revision: 2,
        before_digest: [1; 32],
        after_digest: [2; 32],
        page: None,
        root: None,
        digest: [0; 32],
    };
    progress.digest = progress_digest(&progress).unwrap();
    assert_eq!(
        decode_progress(&encode_progress(&progress).unwrap()).unwrap(),
        progress
    );
    progress.page = Some(Reference {
        key: progress.key,
        digest: [3; 32],
    });
    assert!(progress_digest(&progress).is_err());
    progress.page = None;
    progress.key.ordinal = 2;
    progress.before_revision = 2;
    progress.after_revision = 3;
    assert!(progress_digest(&progress).is_err());
}

#[test]
fn control_records_reject_each_truncation_trailing_bytes_and_digest_changes() {
    let build_bytes = encode_build(&build()).unwrap();
    let root = root();
    let root_bytes = encode_root(&root).unwrap();
    let mut progress = Progress {
        key: SequenceKey {
            build: key(),
            ordinal: 2,
        },
        command: Command::Complete,
        prior: Some(Reference {
            key: SequenceKey {
                build: key(),
                ordinal: 1,
            },
            digest: [0xAA; 32],
        }),
        before_revision: 2,
        after_revision: 3,
        before_digest: [0xBB; 32],
        after_digest: [0xCC; 32],
        page: Some(Reference {
            key: SequenceKey {
                build: key(),
                ordinal: 1,
            },
            digest: [0xDD; 32],
        }),
        root: Some(root.digest),
        digest: [0; 32],
    };
    progress.digest = progress_digest(&progress).unwrap();
    let progress_bytes = encode_progress(&progress).unwrap();
    assert_eq!(progress_bytes.len(), 356);
    for (bytes, decode) in [(
        build_bytes,
        decode_build as fn(&[u8]) -> Result<Build, crate::codec::CodecError>,
    )] {
        for end in 0..bytes.len() {
            assert!(decode(&bytes[..end]).is_err());
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert!(decode(&trailing).is_err());
    }
    for end in 0..root_bytes.len() {
        assert!(decode_root(&root_bytes[..end]).is_err());
    }
    for end in 0..progress_bytes.len() {
        assert!(decode_progress(&progress_bytes[..end]).is_err());
    }
    let mut trailing_root = root_bytes.clone();
    trailing_root.push(0);
    assert!(decode_root(&trailing_root).is_err());
    let mut trailing_progress = progress_bytes.clone();
    trailing_progress.push(0);
    assert!(decode_progress(&trailing_progress).is_err());
    for position in [0, 48, 64, root_bytes.len() - 1] {
        let mut altered = root_bytes.clone();
        altered[position] ^= 0x80;
        assert!(decode_root(&altered).is_err());
    }
    for position in [0, 48, 49, 130, progress_bytes.len() - 1] {
        let mut altered = progress_bytes.clone();
        altered[position] ^= 0x80;
        assert!(decode_progress(&altered).is_err());
    }
}

#[test]
fn roots_reject_mixed_variants_count_overflow_and_impossible_page_totals() {
    let mut value = root();
    value.counts[0] = 257;
    value.code_lines = 257;
    value.pages = 1;
    assert!(root_digest(&value).is_err());
    value = root();
    value.pages = 0;
    assert!(root_digest(&value).is_err());
    value = root();
    value.columns = 1;
    assert!(root_digest(&value).is_err());
    value = root();
    value.header = Some(range(0, 8));
    value.columns = 2;
    assert!(root_digest(&value).is_err());
    value.language = None;
    value.code_lines = 0;
    value.body_rows = u64::MAX;
    value.counts = [0, u64::MAX, 1, 2];
    assert!(root_digest(&value).is_err());
}
