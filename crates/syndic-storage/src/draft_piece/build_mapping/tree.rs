use super::super::mutation::advance_budget::BuildAcquisition;
use super::{DraftPieceBuildMappingFamily, codec, model::*};
use crate::codec::Family;

pub(crate) struct MappingContext<'a, 'store> {
    pub(super) acquisition: &'a BuildAcquisition<'store>,
    pub(super) owner: [u8; 48],
    ordinal: &'a mut u64,
    pub(crate) emitted: Vec<Node>,
    pub(crate) acquired: usize,
}

impl<'a, 'store> MappingContext<'a, 'store> {
    pub(in crate::draft_piece) fn new(
        acquisition: &'a BuildAcquisition<'store>,
        owner: [u8; 48],
        ordinal: &'a mut u64,
    ) -> Self {
        Self {
            acquisition,
            owner,
            ordinal,
            emitted: Vec::new(),
            acquired: 0,
        }
    }
    pub(super) fn load(&mut self, descriptor: Descriptor, root: bool) -> MappingResult<Node> {
        if !descriptor.valid() || self.acquired >= 43 {
            return Err(invalid());
        }
        let mut key = [0; 64];
        key[..48].copy_from_slice(&self.owner);
        key[48..].copy_from_slice(&descriptor.id);
        let node = self
            .acquisition
            .point::<DraftPieceBuildMappingFamily>(key)?
            .ok_or_else(invalid)?;
        self.acquired += 1;
        if node.key != key || !node.shape(root) || node.descriptor()? != descriptor {
            return Err(invalid());
        }
        Ok(node)
    }
    pub(crate) fn authenticate(&mut self, root: MapRoot, expected: Measure) -> MappingResult<()> {
        if !root.valid() || root.measure() != expected {
            return Err(invalid());
        }
        if let MapRoot::Stored(d) = root {
            self.load(d, true)?;
        }
        Ok(())
    }
    pub(super) fn emit(
        &mut self,
        height: u8,
        entries: Entries,
        root: bool,
    ) -> MappingResult<Descriptor> {
        if self.emitted.len() >= 45 || *self.ordinal == 0 {
            return Err(invalid());
        }
        let next = self.ordinal.checked_add(1).ok_or_else(invalid)?;
        let digest = codec::node_digest(&self.owner, height, &entries);
        let id = codec::record_id(&self.owner, *self.ordinal, &digest);
        let mut key = [0; 64];
        key[..48].copy_from_slice(&self.owner);
        key[48..].copy_from_slice(&id);
        let node = Node {
            key,
            height,
            entries,
            digest,
        };
        if !node.shape(root) {
            return Err(invalid());
        }
        let descriptor = node.descriptor()?;
        if !descriptor.valid() {
            return Err(invalid());
        }
        DraftPieceBuildMappingFamily::encode_value(&node).map_err(|_| invalid())?;
        if self
            .acquisition
            .point::<DraftPieceBuildMappingFamily>(key)?
            .is_some()
        {
            return Err(invalid());
        }
        self.acquisition
            .budget
            .emission::<DraftPieceBuildMappingFamily>(&key, &node)?;
        *self.ordinal = next;
        self.emitted.push(node);
        Ok(descriptor)
    }
    pub(crate) fn source_cut(
        &mut self,
        root: MapRoot,
        source: u128,
        require_copy: bool,
    ) -> MappingResult<u128> {
        if !root.valid()
            || source > root.measure().source
            || (require_copy && source == root.measure().source)
        {
            return Err(invalid());
        }
        let mut descriptor = match root {
            MapRoot::Empty => return Ok(0),
            MapRoot::Identity(_) => return Ok(source),
            MapRoot::Stored(d) => d,
        };
        let mut x = source;
        let mut target = 0u128;
        let mut selected_root = true;
        loop {
            let node = self.load(descriptor, selected_root)?;
            selected_root = false;
            match node.entries {
                Entries::Runs(runs) => {
                    for run in runs {
                        let m = run.measure();
                        if m.source > x {
                            return match run {
                                Run::Copy(_) => target.checked_add(x).ok_or_else(invalid),
                                Run::Deleted(_) if !require_copy => Ok(target),
                                _ => Err(invalid()),
                            };
                        }
                        x -= m.source;
                        target = target.checked_add(m.target).ok_or_else(invalid)?;
                    }
                    return if x == 0 && !require_copy {
                        Ok(target)
                    } else {
                        Err(invalid())
                    };
                }
                Entries::Children(children) => {
                    let mut next = None;
                    for child in children {
                        if child.measure.source > x {
                            next = Some(child);
                            break;
                        }
                        x -= child.measure.source;
                        target = target
                            .checked_add(child.measure.target)
                            .ok_or_else(invalid)?;
                    }
                    match next {
                        Some(child) => descriptor = child,
                        None if x == 0 && !require_copy => return Ok(target),
                        None => return Err(invalid()),
                    }
                }
            }
        }
    }
}
