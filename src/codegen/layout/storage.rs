//! Compiler-owned physical storage. This validates the selected allocation;
//! it does not reproduce scalar liveness or grant independent checker authority.
use crate::error::{CompileError, Result};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::codegen) enum StorageBase {
    Frame,
    /// Call-entry SP, after its separately owned outgoing area is reserved.
    OutgoingCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::codegen) enum StorageOwnership {
    FrameOwned,
    CallArguments,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::codegen) enum StorageRegionKind {
    ScalarSlots,
    Cache,
    Collections,
    ExpressionTemporaries,
    Scratch,
    Linkage,
    OutgoingArguments,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::codegen) enum StoragePlaceKind {
    Scalar { local: usize },
    FixedBuffer { owner: usize },
    NamedVariable { name: String },
    HiddenResultPointer,
}

/// Alignment describes this frame place, not the referent of a stored pointer.
/// A zero extent permits no load or store by itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::codegen) struct StoragePlace {
    pub(in crate::codegen) offset: usize,
    pub(in crate::codegen) extent: usize,
    pub(in crate::codegen) actual_alignment: usize,
    pub(in crate::codegen) ownership: StorageOwnership,
    pub(in crate::codegen) kind: StoragePlaceKind,
}

impl StoragePlace {
    fn frame(offset: usize, extent: usize, kind: StoragePlaceKind) -> Result<Self> {
        checked_end(offset, extent)?;
        Ok(Self { offset, extent, actual_alignment: alignment_at(offset), ownership: StorageOwnership::FrameOwned, kind })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::codegen) struct StorageRegion {
    pub(in crate::codegen) base: StorageBase,
    pub(in crate::codegen) offset: usize,
    pub(in crate::codegen) extent: usize,
    pub(in crate::codegen) actual_alignment: usize,
    pub(in crate::codegen) ownership: StorageOwnership,
    pub(in crate::codegen) kind: StorageRegionKind,
}

impl StorageRegion {
    fn new(base: StorageBase, offset: usize, extent: usize, kind: StorageRegionKind) -> Result<Self> {
        checked_end(offset, extent)?;
        Ok(Self {
            base,
            offset,
            extent,
            actual_alignment: alignment_at(offset),
            ownership: match base {
                StorageBase::Frame => StorageOwnership::FrameOwned,
                StorageBase::OutgoingCall => StorageOwnership::CallArguments,
            },
            kind,
        })
    }
}

/// Actual boundaries selected by frame.rs, rather than a second allocator.
#[derive(Debug, Clone, Copy)]
pub(in crate::codegen) struct StorageBoundaries {
    pub(in crate::codegen) local_end: usize,
    pub(in crate::codegen) cache: Option<(usize, usize)>,
    pub(in crate::codegen) collection_bytes: usize,
    pub(in crate::codegen) expression_temporary_bytes: usize,
    pub(in crate::codegen) scratch_bytes: usize,
    pub(in crate::codegen) outgoing_bytes: usize,
}

#[derive(Debug, Clone)]
pub(in crate::codegen) struct FunctionStoragePlan {
    pub(in crate::codegen) frame_size: usize,
    pub(in crate::codegen) scalar_region_bytes: usize,
    pub(in crate::codegen) scalar_slot_offsets: BTreeMap<usize, usize>,
    pub(in crate::codegen) fixed_byte_local_offsets: HashMap<usize, usize>,
    pub(in crate::codegen) named_var_offsets: HashMap<String, usize>,
    pub(in crate::codegen) return_buffer_pointer_offset: Option<usize>,
    pub(in crate::codegen) collection_region_start: usize,
    pub(in crate::codegen) next_collection_slot: usize,
    pub(in crate::codegen) exact_read_cache_offset: Option<usize>,
    fixed_buffer_widths: HashMap<usize, usize>,
    fixed_buffer_owners: HashMap<usize, usize>,
    owned_value_buffers: HashMap<usize, (usize, usize)>,
    pub(in crate::codegen) places: Vec<StoragePlace>,
    pub(in crate::codegen) regions: Vec<StorageRegion>,
}

impl Default for FunctionStoragePlan {
    fn default() -> Self {
        Self {
            frame_size: 16,
            scalar_region_bytes: 0,
            scalar_slot_offsets: BTreeMap::new(),
            fixed_byte_local_offsets: HashMap::new(),
            named_var_offsets: HashMap::new(),
            return_buffer_pointer_offset: None,
            collection_region_start: 0,
            next_collection_slot: 0,
            exact_read_cache_offset: None,
            fixed_buffer_widths: HashMap::new(),
            fixed_buffer_owners: HashMap::new(),
            owned_value_buffers: HashMap::new(),
            places: Vec::new(),
            regions: Vec::new(),
        }
    }
}

impl FunctionStoragePlan {
    /// Compatibility accessor for existing emitter and hand-built runtime tests.
    /// Typed preparation validates every used IR ID through validate_scalar_ids.
    /// The identity fallback is lazy and is not a physical-region size inference.
    pub(in crate::codegen) fn scalar_offset(&self, id: usize) -> usize {
        self.scalar_slot_offsets.get(&id).copied().unwrap_or_else(|| id * 8)
    }

    pub(in crate::codegen) fn reset_layout_records(&mut self) {
        self.fixed_byte_local_offsets.clear();
        self.fixed_buffer_widths.clear();
        self.fixed_buffer_owners.clear();
        self.owned_value_buffers.clear();
        self.places.clear();
        self.regions.clear();
    }

    pub(in crate::codegen) fn checked_scalar_place(&self, local: usize) -> Result<StoragePlace> {
        let offset = match self.scalar_slot_offsets.get(&local) {
            Some(offset) => *offset,
            None => local.checked_mul(8).ok_or_else(|| invalid("scalar slot offset overflows"))?,
        };
        if !offset.is_multiple_of(8) || checked_end(offset, 8)? > self.scalar_region_bytes {
            return Err(invalid("scalar slot lies outside its actual allocated region"));
        }
        StoragePlace::frame(offset, 8, StoragePlaceKind::Scalar { local })
    }

    pub(in crate::codegen) fn validate_scalar_ids(&self, ids: impl IntoIterator<Item = usize>) -> Result<()> {
        for id in ids {
            self.checked_scalar_place(id)?;
        }
        Ok(())
    }

    pub(in crate::codegen) fn record_fixed_buffer(&mut self, local: usize, offset: usize, width: usize) -> Result<()> {
        checked_end(offset, width)?;
        if !offset.is_multiple_of(8) {
            return Err(invalid("fixed-value buffer must retain its allocated eight-byte alignment"));
        }
        if self.fixed_byte_local_offsets.get(&local).is_some_and(|old| *old != offset)
            || self.fixed_buffer_widths.get(&local).is_some_and(|old| *old != width)
        {
            return Err(invalid("one fixed-value local has conflicting physical storage"));
        }
        if self.fixed_byte_local_offsets.contains_key(&local) {
            return self.record_fixed_buffer_alias(local, local);
        }
        self.fixed_byte_local_offsets.insert(local, offset);
        self.fixed_buffer_widths.insert(local, width);
        self.fixed_buffer_owners.insert(local, local);
        Ok(())
    }

    /// Record an independently allocated owned value without changing any
    /// legacy source-dispatch map (for example the separate u128 buffer bank).
    pub(in crate::codegen) fn record_owned_value_buffer(&mut self, local: usize, offset: usize, width: usize) -> Result<()> {
        checked_end(offset, width)?;
        if !offset.is_multiple_of(8) {
            return Err(invalid("owned value buffer is misaligned"));
        }
        if self.owned_value_buffers.get(&local).is_some_and(|old| *old != (offset, width)) {
            return Err(invalid("one owned value has conflicting auxiliary storage"));
        }
        self.owned_value_buffers.insert(local, (offset, width));
        Ok(())
    }

    /// An explicit whole-object alias is compatible; arbitrary overlapping
    /// independently owned buffers are not. This does not establish lifetime.
    pub(in crate::codegen) fn record_fixed_buffer_alias(&mut self, alias: usize, owner: usize) -> Result<()> {
        let offset = *self.fixed_byte_local_offsets.get(&owner).ok_or_else(|| invalid("fixed-buffer alias has no owner"))?;
        let width = *self.fixed_buffer_widths.get(&owner).ok_or_else(|| invalid("fixed-buffer alias has no recorded extent"))?;
        let root = *self.fixed_buffer_owners.get(&owner).ok_or_else(|| invalid("fixed-buffer alias has no object owner"))?;
        if self.fixed_byte_local_offsets.get(&alias).is_some_and(|old| *old != offset)
            || self.fixed_buffer_widths.get(&alias).is_some_and(|old| *old != width)
        {
            return Err(invalid("fixed-buffer alias conflicts with its existing storage"));
        }
        self.fixed_byte_local_offsets.insert(alias, offset);
        self.fixed_buffer_widths.insert(alias, width);
        self.fixed_buffer_owners.insert(alias, root);
        Ok(())
    }

    /// Call only after typed frame preparation. Raw runtime helpers retain their
    /// separately owned frames and do not implicitly validate Default's 16B.
    pub(in crate::codegen) fn finish(&mut self, bounds: StorageBoundaries) -> Result<()> {
        if self.frame_size < 16 || !self.frame_size.is_multiple_of(16) {
            return Err(invalid("frame must include aligned linkage and preserve sixteen-byte SP alignment"));
        }
        checked_end(0, self.frame_size)?;
        if !self.scalar_region_bytes.is_multiple_of(8) || self.scalar_region_bytes > bounds.local_end {
            return Err(invalid("scalar region exceeds actual local storage or is misaligned"));
        }
        if self.exact_read_cache_offset != bounds.cache.map(|(offset, _)| offset) {
            return Err(invalid("cache boundary differs from its selected physical owner"));
        }
        if !bounds.outgoing_bytes.is_multiple_of(16) {
            return Err(invalid("outgoing arguments must preserve sixteen-byte call alignment"));
        }
        let linkage = self.frame_size - 16;
        let scratch = linkage.checked_sub(bounds.scratch_bytes).ok_or_else(|| invalid("scratch exceeds frame"))?;
        let temporaries =
            scratch.checked_sub(bounds.expression_temporary_bytes).ok_or_else(|| invalid("expression temporaries exceed frame"))?;
        if bounds.local_end > temporaries {
            return Err(invalid("local storage overlaps expression/scratch/linkage storage"));
        }
        if self.fixed_byte_local_offsets.len() != self.fixed_buffer_widths.len() {
            return Err(invalid("fixed-value storage is missing its actual extent"));
        }
        let collections_end = checked_end(self.collection_region_start, bounds.collection_bytes)?;
        if self.collection_region_start > bounds.local_end || collections_end > bounds.local_end {
            return Err(invalid("collection reservation exceeds local storage"));
        }
        let collection_slot_bytes = 8 + crate::codegen::RUNTIME_COLLECTION_BUFFER_SIZE;
        if !bounds.collection_bytes.is_multiple_of(collection_slot_bytes)
            || self.next_collection_slot > bounds.collection_bytes / collection_slot_bytes
        {
            return Err(invalid("collection cursor exceeds its actual reserved slots"));
        }
        let mut regions = vec![
            StorageRegion::new(StorageBase::Frame, 0, self.scalar_region_bytes, StorageRegionKind::ScalarSlots)?,
            StorageRegion::new(
                StorageBase::Frame,
                self.collection_region_start,
                bounds.collection_bytes,
                StorageRegionKind::Collections,
            )?,
            StorageRegion::new(
                StorageBase::Frame,
                temporaries,
                bounds.expression_temporary_bytes,
                StorageRegionKind::ExpressionTemporaries,
            )?,
            StorageRegion::new(StorageBase::Frame, scratch, bounds.scratch_bytes, StorageRegionKind::Scratch)?,
            StorageRegion::new(StorageBase::Frame, linkage, 16, StorageRegionKind::Linkage)?,
            StorageRegion::new(StorageBase::OutgoingCall, 0, bounds.outgoing_bytes, StorageRegionKind::OutgoingArguments)?,
        ];
        if let Some((offset, bytes)) = bounds.cache {
            if !offset.is_multiple_of(8) || bytes == 0 || checked_end(offset, bytes)? > bounds.local_end {
                return Err(invalid("cache region is empty, misaligned or outside local storage"));
            }
            regions.push(StorageRegion::new(StorageBase::Frame, offset, bytes, StorageRegionKind::Cache)?);
        }
        for (index, region) in regions.iter().enumerate() {
            let ownership = match region.base {
                StorageBase::Frame => StorageOwnership::FrameOwned,
                StorageBase::OutgoingCall => StorageOwnership::CallArguments,
            };
            if region.actual_alignment != alignment_at(region.offset) || region.ownership != ownership {
                return Err(invalid("region alignment or ownership differs from its physical base"));
            }
            for other in &regions[..index] {
                if region.base == other.base && overlaps(region.offset, region.extent, other.offset, other.extent) {
                    return Err(invalid("separately reserved physical regions overlap"));
                }
            }
        }
        let mut places = self.scalar_slot_offsets.keys().map(|id| self.checked_scalar_place(*id)).collect::<Result<Vec<_>>>()?;
        for (&local, &offset) in &self.fixed_byte_local_offsets {
            let width = *self.fixed_buffer_widths.get(&local).ok_or_else(|| invalid("fixed buffer has no extent"))?;
            let owner = *self.fixed_buffer_owners.get(&local).ok_or_else(|| invalid("fixed buffer has no object owner"))?;
            places.push(StoragePlace::frame(offset, width, StoragePlaceKind::FixedBuffer { owner })?);
        }
        for (&local, &(offset, width)) in &self.owned_value_buffers {
            let owner = self.fixed_buffer_owners.get(&local).copied().unwrap_or(local);
            places.push(StoragePlace::frame(offset, width, StoragePlaceKind::FixedBuffer { owner })?);
        }
        for (name, &offset) in &self.named_var_offsets {
            places.push(StoragePlace::frame(offset, 8, StoragePlaceKind::NamedVariable { name: name.clone() })?);
        }
        if let Some(offset) = self.return_buffer_pointer_offset {
            places.push(StoragePlace::frame(offset, 8, StoragePlaceKind::HiddenResultPointer)?);
        }
        places.sort_by(|a, b| a.offset.cmp(&b.offset).then(a.extent.cmp(&b.extent)).then(a.kind.cmp(&b.kind)));
        let mut previous: Option<&StoragePlace> = None;
        for place in &places {
            if place.actual_alignment < 8
                || place.ownership != StorageOwnership::FrameOwned
                || checked_end(place.offset, place.extent)? > bounds.local_end
            {
                return Err(invalid("value storage is misaligned or exceeds local storage"));
            }
            for region in &regions {
                if region.base != StorageBase::Frame || region.kind == StorageRegionKind::ScalarSlots {
                    continue;
                }
                if overlaps(place.offset, place.extent, region.offset, region.extent) {
                    return Err(invalid("value storage overlaps cache, collection or reserved tail storage"));
                }
            }
            if !matches!(place.kind, StoragePlaceKind::Scalar { .. })
                && overlaps(place.offset, place.extent, 0, self.scalar_region_bytes)
            {
                return Err(invalid("owned value storage overlaps scalar slots"));
            }
            if let Some(other) = previous
                && overlaps(place.offset, place.extent, other.offset, other.extent)
            {
                let compatible = place.offset == other.offset
                    && place.extent == other.extent
                    && match (&place.kind, &other.kind) {
                        (StoragePlaceKind::Scalar { .. }, StoragePlaceKind::Scalar { .. }) => true,
                        (StoragePlaceKind::FixedBuffer { owner: left }, StoragePlaceKind::FixedBuffer { owner: right }) => {
                            left == right
                        }
                        _ => false,
                    };
                if !compatible {
                    return Err(invalid("distinct value storage overlaps without an explicit compatible alias"));
                }
            }
            if place.extent != 0 {
                previous = Some(place);
            }
        }
        self.places = places;
        self.regions = regions;
        Ok(())
    }
}

fn invalid(message: &str) -> CompileError {
    CompileError::without_span(format!("invalid compiler storage plan: {message}"))
}

fn checked_end(offset: usize, extent: usize) -> Result<usize> {
    let end = offset.checked_add(extent).ok_or_else(|| invalid("physical storage extent overflows"))?;
    crate::ir::checked_fixed_size(Some(end))?;
    Ok(end)
}

fn alignment_at(offset: usize) -> usize {
    if offset == 0 {
        16
    } else {
        1usize << offset.trailing_zeros().min(4)
    }
}

fn overlaps(left: usize, left_width: usize, right: usize, right_width: usize) -> bool {
    left_width != 0 && right_width != 0 && left < right + right_width && right < left + left_width
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (FunctionStoragePlan, StorageBoundaries) {
        let mut plan =
            FunctionStoragePlan { frame_size: 192, scalar_region_bytes: 24, collection_region_start: 80, ..Default::default() };
        plan.scalar_slot_offsets.extend([(0, 0), (1, 8), (2, 16), (100, 16)]);
        plan.record_fixed_buffer(3, 24, 16).unwrap();
        plan.named_var_offsets.insert("saved".into(), 40);
        plan.return_buffer_pointer_offset = Some(48);
        plan.exact_read_cache_offset = Some(56);
        let bounds = StorageBoundaries {
            local_end: 80,
            cache: Some((56, 24)),
            collection_bytes: 0,
            expression_temporary_bytes: 32,
            scratch_bytes: 64,
            outgoing_bytes: 32,
        };
        (plan, bounds)
    }

    #[test]
    fn checked_regions_preserve_dead_scalar_reuse_and_actual_alignment() {
        let (mut plan, bounds) = fixture();
        plan.validate_scalar_ids([0, 1, 2, 100]).unwrap();
        plan.finish(bounds).unwrap();
        assert_eq!(plan.scalar_offset(100), 16);
        assert_eq!(plan.places.iter().find(|p| p.kind == StoragePlaceKind::FixedBuffer { owner: 3 }).unwrap().actual_alignment, 8);
        let outgoing = plan.regions.iter().find(|r| r.kind == StorageRegionKind::OutgoingArguments).unwrap();
        assert_eq!(
            (outgoing.base, outgoing.extent, outgoing.ownership),
            (StorageBase::OutgoingCall, 32, StorageOwnership::CallArguments)
        );
    }

    #[test]
    fn fallback_is_lazy_but_checked_preparation_rejects_overflow_and_unknown_ids() {
        let mut plan = FunctionStoragePlan { scalar_region_bytes: 8, ..Default::default() };
        plan.scalar_slot_offsets.insert(usize::MAX, 0);
        assert_eq!(plan.scalar_offset(usize::MAX), 0);
        plan.validate_scalar_ids([usize::MAX]).unwrap();
        plan.scalar_slot_offsets.clear();
        assert!(plan.checked_scalar_place(usize::MAX).is_err());
        assert!(plan.validate_scalar_ids([1]).is_err());
    }

    #[test]
    fn explicit_buffer_alias_and_zero_width_do_not_create_false_overlap() {
        let (mut plan, bounds) = fixture();
        plan.record_fixed_buffer_alias(9, 3).unwrap();
        plan.record_fixed_buffer(10, 80, 0).unwrap();
        plan.finish(bounds).unwrap();
        assert!(plan.places.iter().any(|p| p.extent == 0));
        let (mut plan, bounds) = fixture();
        plan.record_fixed_buffer(9, 24, 16).unwrap();
        assert!(plan.finish(bounds).is_err());
    }

    #[test]
    fn protected_regions_reject_cache_overlap_tail_overlap_and_bad_linkage() {
        let (mut plan, mut bounds) = fixture();
        plan.record_fixed_buffer(4, 72, 16).unwrap();
        assert!(plan.finish(bounds).is_err());
        let (mut plan, _) = fixture();
        bounds.local_end = 81;
        assert!(plan.finish(bounds).is_err());
        let (mut plan, bounds) = fixture();
        plan.frame_size = 191;
        assert!(plan.finish(bounds).is_err());
        let (mut plan, mut bounds) = fixture();
        bounds.scratch_bytes = usize::MAX;
        assert!(plan.finish(bounds).is_err());
    }

    #[test]
    fn named_hidden_and_scalar_storage_cannot_partially_alias() {
        for offset in [0, 8, 24, 48, 56] {
            let (mut plan, bounds) = fixture();
            plan.named_var_offsets.insert("other".into(), offset);
            assert!(plan.finish(bounds).is_err(), "offset {offset}");
        }
        let (mut plan, bounds) = fixture();
        plan.scalar_slot_offsets.insert(5, 12);
        assert!(plan.finish(bounds).is_err());
    }

    #[test]
    fn cache_identity_fixed_extents_and_outgoing_alignment_are_checked() {
        let (mut plan, mut bounds) = fixture();
        bounds.cache = Some((64, 16));
        assert!(plan.finish(bounds).is_err());
        let (mut plan, bounds) = fixture();
        plan.fixed_byte_local_offsets.insert(7, 80);
        assert!(plan.finish(bounds).is_err());
        let (mut plan, mut bounds) = fixture();
        bounds.outgoing_bytes = 24;
        assert!(plan.finish(bounds).is_err());
        let (mut plan, bounds) = fixture();
        plan.record_fixed_buffer(7, usize::MAX - 7, 16).unwrap_err();
        plan.finish(bounds).unwrap();
    }

    #[test]
    fn collection_reservation_does_not_swallow_unrelated_tail_allocations() {
        let (mut plan, mut bounds) = fixture();
        plan.frame_size = 544;
        bounds.local_end = 432;
        bounds.collection_bytes = 8 + crate::codegen::RUNTIME_COLLECTION_BUFFER_SIZE;
        plan.record_fixed_buffer(20, 344, 32).unwrap();
        plan.return_buffer_pointer_offset = Some(376);
        plan.next_collection_slot = 1;
        plan.finish(bounds).unwrap();
        plan.next_collection_slot = 2;
        assert!(plan.finish(bounds).is_err());
    }

    #[test]
    fn auxiliary_wide_buffers_preserve_legacy_source_maps_and_protected_regions() {
        let (mut plan, mut bounds) = fixture();
        plan.record_owned_value_buffer(3, 24, 16).unwrap();
        plan.frame_size = 208;
        bounds.local_end = 96;
        plan.record_owned_value_buffer(44, 80, 16).unwrap();
        assert!(!plan.fixed_byte_local_offsets.contains_key(&44));
        plan.finish(bounds).unwrap();
        assert!(plan.record_owned_value_buffer(44, 80, 24).is_err());
        let (mut plan, bounds) = fixture();
        plan.record_owned_value_buffer(44, 72, 8).unwrap();
        assert!(plan.finish(bounds).is_err());
    }

    #[test]
    fn runtime_default_is_not_mistaken_for_a_prepared_typed_frame() {
        let plan = FunctionStoragePlan::default();
        assert_eq!(plan.frame_size, 16);
        assert_eq!(plan.scalar_offset(2), 16);
        assert!(plan.regions.is_empty());
    }
}
