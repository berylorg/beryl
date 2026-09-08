use super::{MappingContext, model::*};

struct Frame {
    children: Vec<Descriptor>,
    index: usize,
    height: u8,
}

impl MappingContext<'_, '_> {
    fn descend(
        &mut self,
        root: MapRoot,
        target: u128,
        insertion: bool,
    ) -> MappingResult<(Vec<Frame>, Vec<Run>, u128)> {
        if !root.valid()
            || target > root.measure().target
            || (!insertion && target == root.measure().target)
        {
            return Err(invalid());
        }
        let mut frames = Vec::with_capacity(22);
        let mut descriptor = match root {
            MapRoot::Empty => return Ok((frames, Vec::new(), 0)),
            MapRoot::Identity(n) => return Ok((frames, vec![Run::Copy(n)], 0)),
            MapRoot::Stored(d) => d,
        };
        let mut start = 0u128;
        let mut selected_root = true;
        loop {
            let node = self.load(descriptor, selected_root)?;
            selected_root = false;
            match node.entries {
                Entries::Runs(runs) => return Ok((frames, runs, start)),
                Entries::Children(children) => {
                    let mut chosen = None;
                    for (index, child) in children.iter().enumerate() {
                        let end = start
                            .checked_add(child.measure.target)
                            .ok_or_else(invalid)?;
                        if (insertion && end >= target) || (!insertion && end > target) {
                            chosen = Some((index, *child));
                            break;
                        }
                        start = end;
                    }
                    let (index, child) = chosen.ok_or_else(invalid)?;
                    frames.push(Frame {
                        children,
                        index,
                        height: descriptor.height,
                    });
                    descriptor = child;
                }
            }
        }
    }
    fn emit_region(
        &mut self,
        height: u8,
        mut entries: Entries,
        root: bool,
    ) -> MappingResult<Vec<Descriptor>> {
        if entries.len() <= 16 {
            return Ok(vec![self.emit(height, entries, root)?]);
        }
        if entries.len() > 32 {
            return Err(invalid());
        }
        let right = entries.split_off(entries.len() / 2);
        Ok(vec![
            self.emit(height, entries, false)?,
            self.emit(height, right, false)?,
        ])
    }
    fn normalize(
        &mut self,
        mut frames: Vec<Frame>,
        mut entries: Entries,
    ) -> MappingResult<MapRoot> {
        let mut height = 1;
        while let Some(mut frame) = frames.pop() {
            let mut begin = frame.index;
            let mut count = 1;
            if entries.len() < 8 {
                let sibling_index = if begin > 0 { begin - 1 } else { begin + 1 };
                let sibling = self.load(
                    *frame.children.get(sibling_index).ok_or_else(invalid)?,
                    false,
                )?;
                if sibling_index < begin {
                    let mut left = sibling.entries;
                    left.append(entries)?;
                    entries = left;
                    begin -= 1;
                } else {
                    entries.append(sibling.entries)?;
                }
                count = 2;
            }
            let replacement = self.emit_region(height, entries, false)?;
            frame.children.splice(begin..begin + count, replacement);
            entries = Entries::Children(frame.children);
            height = frame.height;
        }
        if entries.len() == 0 {
            return Ok(MapRoot::Empty);
        }
        if let Entries::Children(children) = &entries {
            if children.len() == 1 {
                return Ok(MapRoot::Stored(children[0]));
            }
        }
        let roots = self.emit_region(height, entries, true)?;
        if roots.len() == 1 {
            Ok(MapRoot::Stored(roots[0]))
        } else {
            Ok(MapRoot::Stored(self.emit(
                height.checked_add(1).ok_or_else(invalid)?,
                Entries::Children(roots),
                true,
            )?))
        }
    }
    pub(crate) fn insert(
        &mut self,
        root: MapRoot,
        at: u128,
        length: u128,
    ) -> MappingResult<MapRoot> {
        if length == 0
            || root
                .measure()
                .target
                .checked_add(length)
                .is_none_or(|n| n > MAX_UNITS)
        {
            return Err(invalid());
        }
        let (frames, runs, start) = self.descend(root, at, true)?;
        let mut remaining = at.checked_sub(start).ok_or_else(invalid)?;
        let mut output = Vec::with_capacity(18);
        let mut inserted = false;
        for run in runs {
            if !inserted && remaining <= run.measure().target {
                if remaining > 0 {
                    output.push(run.with_length(remaining));
                }
                output.push(Run::Inserted(length));
                if remaining < run.length() {
                    output.push(run.with_length(run.length() - remaining));
                }
                inserted = true;
            } else {
                if !inserted {
                    remaining -= run.measure().target;
                }
                output.push(run);
            }
        }
        if !inserted {
            if remaining != 0 {
                return Err(invalid());
            }
            output.push(Run::Inserted(length));
        }
        let result = self.normalize(frames, Entries::Runs(output))?;
        if result.measure()
            != (Measure {
                source: root.measure().source,
                target: root.measure().target + length,
            })
        {
            return Err(invalid());
        }
        Ok(result)
    }
    pub(crate) fn delete_leaf(
        &mut self,
        root: MapRoot,
        a: u128,
        end: u128,
    ) -> MappingResult<(MapRoot, u128)> {
        if a >= end || end > root.measure().target {
            return Err(invalid());
        }
        let (frames, runs, start) = self.descend(root, end - 1, false)?;
        let next_end = a.max(start);
        if next_end >= end {
            return Err(invalid());
        }
        let mut cursor = start;
        let mut output = Vec::with_capacity(18);
        for run in runs {
            let run_end = cursor
                .checked_add(run.measure().target)
                .ok_or_else(invalid)?;
            let lo = cursor.max(a);
            let hi = run_end.min(end);
            if lo >= hi || matches!(run, Run::Deleted(_)) {
                output.push(run);
            } else {
                if lo > cursor {
                    output.push(run.with_length(lo - cursor));
                }
                if matches!(run, Run::Copy(_)) {
                    output.push(Run::Deleted(hi - lo));
                }
                if hi < run_end {
                    output.push(run.with_length(run_end - hi));
                }
            }
            cursor = run_end;
        }
        if output.len() > 18 {
            return Err(invalid());
        }
        let result = self.normalize(frames, Entries::Runs(output))?;
        if result.measure()
            != (Measure {
                source: root.measure().source,
                target: root.measure().target - (end - next_end),
            })
        {
            return Err(invalid());
        }
        Ok((result, next_end))
    }
}
