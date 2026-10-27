//! Bounded allocation primitive, including every mutation and zeroing byte.
//! Resize/release, descriptors, element copies and sorted construction compose
//! checked lookup/copying and conditional mutation.
//! Whole-operation source equivalence remains separate. A per-typed-block
//! inventory prevents erasing all marked sites.
use super::*;
use crate::allocated_values::Contract;
use crate::cell_selection::machine::{cursor::Cursor, flow::Markers};

mod arguments;
mod borrowed;
mod descriptor;
mod elements;
mod lifecycle;
mod ordered;
mod paths;
mod queries;
mod reads;
mod sorted;
mod transport;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Primitive {
    Allocate,
    Resize,
    Release,
    Reserve,
    Drop,
    Push,
    Insert,
    Set,
    Remove,
    Find,
    Put,
    Erase,
    Read,
    Lookup,
    Take,
    Pop,
    Required,
    Sorted(u32),
    Extent,
    Len,
    Capacity,
    Empty,
}

impl Primitive {
    fn tier(self) -> u8 {
        match self {
            Self::Allocate | Self::Extent => 0,
            Self::Resize | Self::Release => 1,
            Self::Reserve | Self::Drop => 2,
            Self::Push
            | Self::Insert
            | Self::Set
            | Self::Remove
            | Self::Find
            | Self::Read
            | Self::Sorted(_)
            | Self::Len
            | Self::Capacity
            | Self::Empty => 3,
            Self::Put | Self::Erase | Self::Lookup | Self::Take | Self::Pop => 4,
            Self::Required => 5,
        }
    }
}

struct Site<'a> {
    start: u64,
    end: u64,
    owner: &'a str,
    block: u32,
    offset: u32,
    descriptor: Option<u32>,
    kind: Primitive,
    contract: &'a Contract,
    borrowed: Option<Borrowed>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Borrowed {
    pointer: u32,
    scratch: u32,
}

impl Site<'_> {
    fn scratch(&self) -> u32 {
        self.borrowed.map_or(self.offset + 32 + self.contract.bytes + self.contract.members.len() as u32 * 32, |borrow| borrow.scratch)
    }

    fn region(&self) -> (u32, u32) {
        (self.borrowed.map_or(self.offset, |borrow| borrow.scratch), self.scratch() + 128)
    }
}

fn invalid(message: impl Into<String>) -> CheckerError {
    CheckerError::new(
        CheckerRejectionCode::V2420TypedMachineBindingInvalid,
        format!("allocated allocation machine contract: {}", message.into()),
    )
}

struct Machine<'a> {
    words: &'a BTreeMap<u64, u32>,
    targets: &'a BTreeMap<u64, u64>,
    abort: u64,
}

impl Machine<'_> {
    fn member_child(
        &self,
        c: &mut Cursor<'_>,
        parent: &Site<'_>,
        kind: Primitive,
        children: &BTreeMap<u64, &Site<'_>>,
        consumed: &mut BTreeSet<u64>,
    ) -> Result<(), CheckerError> {
        let child = children.get(&c.pc).ok_or_else(|| invalid("member operation has no checked child"))?;
        if child.kind != kind
            || child.owner != parent.owner
            || child.offset != parent.offset
            || child.descriptor != parent.descriptor
            || child.contract != parent.contract
            || child.borrowed != parent.borrowed
            || child.start < parent.start
            || child.end > parent.end
            || !consumed.insert(child.start)
        {
            return Err(invalid("member child is mismatched, detached or reused"));
        }
        c.pc = child.end;
        Ok(())
    }

    fn failure(&self, pc: u64) -> Result<(), CheckerError> {
        let mut cursor = Cursor::new(pc, pc + 8, self.words, self.targets);
        cursor.constant(10, 21)?;
        if cursor.jump_target()? != self.abort {
            return Err(invalid("allocation failure does not terminate with error 21"));
        }
        cursor.finish()
    }

    /// Prove both private bounds and preservation of all temporary registers.
    /// The terminal error path may abandon the temporary 32-byte spill frame.
    fn store(
        &self,
        cursor: &mut Cursor<'_>,
        source: u32,
        base: u32,
        displacement: i32,
        byte: bool,
        region: (u32, u32),
    ) -> Result<(), CheckerError> {
        let scratch = [16, 17, 31, 30, 29].into_iter().filter(|r| *r != source && *r != base).take(3).collect::<Vec<_>>();
        cursor.addi(2, 2, -32)?;
        for (index, register) in scratch.iter().enumerate() {
            cursor.store(*register, 2, index as u32 * 8, false)?;
        }
        let (pointer, lower, upper) = (scratch[0], scratch[1], scratch[2]);
        cursor.addi(pointer, base, displacement)?;
        cursor.stack_address(lower, region.0 + 32)?;
        cursor.stack_address(upper, region.1 + 32 - if byte { 1 } else { 8 })?;
        let target = *self.targets.get(&cursor.pc).ok_or_else(|| invalid("missing write failure edge"))?;
        self.failure(target)?;
        let failure = cursor.target(target);
        cursor.branch(6, pointer, lower, failure)?;
        cursor.branch(6, upper, pointer, failure)?;
        cursor.store(source, pointer, 0, byte)?;
        for (index, register) in scratch.iter().enumerate() {
            cursor.load(*register, 2, index as u32 * 8, false)?;
        }
        cursor.addi(2, 2, 32)
    }

    fn allocation(&self, start: u64, end: u64, offset: u32, contract: &Contract) -> Result<(), CheckerError> {
        if start >= end || end - start > 4096 {
            return Err(invalid("invalid or excessive allocation extent"));
        }
        let mut c = Cursor::new(start, end, self.words, self.targets);
        let done = c.target(end);
        let bad = c.label();
        let zero = c.label();
        c.branch(0, 10, 0, done)?;
        c.constant(7, u64::from(contract.bytes - 32))?;
        c.branch(6, 7, 10, bad)?;
        c.addi(7, 10, 15)?;
        c.immediate(0x13, 7, 7, 7, -16)?;
        c.stack_address(29, offset)?;
        c.load(5, 29, 0, false)?;
        c.load(6, 29, 8, false)?;
        c.immediate(0x13, 7, 28, 5, 15)?;
        c.branch(1, 28, 0, bad)?;
        c.constant(28, u64::from(contract.bytes))?;
        c.branch(6, 28, 5, bad)?;
        c.sub(28, 28, 5)?;
        c.addi(30, 7, 32)?;
        c.branch(6, 28, 30, bad)?;
        c.constant(28, u64::from(contract.allocations))?;
        c.branch(7, 6, 28, bad)?;
        c.addi(31, 29, 32)?;
        c.add(31, 31, 5)?;
        c.add(5, 5, 30)?;
        c.addi(6, 6, 1)?;
        let region = (offset, offset + 32 + contract.bytes + contract.members.len() as u32 * 32 + 128);
        self.store(&mut c, 5, 29, 0, false, region)?;
        self.store(&mut c, 6, 29, 8, false, region)?;
        self.store(&mut c, 7, 31, 0, false, region)?;
        self.store(&mut c, 10, 31, 8, false, region)?;
        c.constant(28, 1)?;
        self.store(&mut c, 28, 31, 16, false, region)?;
        self.store(&mut c, 6, 31, 24, false, region)?;
        c.addi(10, 31, 32)?;
        c.addi(5, 10, 0)?;
        c.bind(zero)?;
        self.store(&mut c, 0, 5, 0, true, region)?;
        c.addi(5, 5, 1)?;
        c.addi(7, 7, -1)?;
        c.branch(1, 7, 0, zero)?;
        c.jump(done)?;
        c.bind(bad)?;
        self.failure(c.jump_target()?)?;
        c.finish()
    }
}

pub(super) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    constructors: &Markers<'_>,
    abort: u64,
    checked_constructors: &BTreeMap<String, BTreeMap<u64, Constructor>>,
) -> Result<Checked, CheckerError> {
    let mut expected = BTreeMap::<(&str, u32, u32, Option<u32>, Primitive), (usize, &Contract)>::new();
    let owner_types = crate::allocated_values::owner_types(&record.typed_semantics);
    let mut work = 0usize;
    for entry in &record.typed_semantics.entries {
        let parameters = entry.params.iter().map(|param| (param.binding_id, param)).collect::<BTreeMap<_, _>>();
        let mut roots = BTreeMap::new();
        let aliases = super::owner_aliases(entry, &owner_types)?;
        for block in &entry.blocks {
            for op in &block.operations {
                if let TypedSemanticOperationDetail::AllocatedValue { operation: Operation::New, contract, .. } = &op.detail {
                    let offset =
                        constructors.get(&(entry.id.as_str(), block.id, op.index)).ok_or_else(|| invalid("unchecked constructor"))?.2;
                    roots.insert(op.destinations[0], (offset, contract));
                }
            }
        }
        for block in &entry.blocks {
            for op in &block.operations {
                let TypedSemanticOperationDetail::AllocatedValue { operation, contract, member } = &op.detail else { continue };
                let ordered = member
                    .and_then(|index| contract.members.get(index as usize))
                    .is_some_and(|member| !matches!(member.shape, crate::allocated_values::MemberKind::Vec { .. }));
                let searches = ordered
                    && matches!(
                        operation,
                        Operation::Insert | Operation::Remove | Operation::Get | Operation::GetOr | Operation::Contains
                    );
                let sorted_count = if *operation == Operation::LoadSorted {
                    Some(
                        op.operands
                            .get(1)
                            .and_then(|arg| arg.ty.strip_suffix(']'))
                            .and_then(|ty| ty.rsplit_once("; "))
                            .and_then(|(_, count)| count.parse::<u32>().ok())
                            .filter(|count| *count <= 1024)
                            .ok_or_else(|| invalid("sorted input has no bounded fixed count"))?,
                    )
                } else {
                    None
                };
                let allocates = matches!(operation, Operation::Reserve | Operation::Push | Operation::Insert)
                    || sorted_count.is_some_and(|count| count != 0);
                if !allocates
                    && !searches
                    && !matches!(
                        operation,
                        Operation::Drop
                            | Operation::Set
                            | Operation::Remove
                            | Operation::Pop
                            | Operation::Get
                            | Operation::GetOr
                            | Operation::EntryKey
                            | Operation::EntryValue
                            | Operation::LoadSorted
                            | Operation::Len
                            | Operation::Capacity
                            | Operation::IsEmpty
                    )
                {
                    continue;
                }
                let mut root =
                    op.operands.first().and_then(|arg| arg.local).ok_or_else(|| invalid("allocation has no local owner"))?;
                let mut visited = BTreeSet::new();
                while !roots.contains_key(&root) && !parameters.contains_key(&root) {
                    work += 1;
                    if work > 4_000_000 || !visited.insert(root) {
                        return Err(invalid("cyclic or excessive owner alias chain"));
                    }
                    let sources = aliases
                        .get(&root)
                        .filter(|sources| sources.len() == 1)
                        .ok_or_else(|| invalid("ambiguous allocation owner"))?;
                    root = *sources.first().ok_or_else(|| invalid("empty owner alias"))?;
                }
                if !roots.contains_key(&root) {
                    // This slice checks invocation-owned storage. Read-only
                    // helpers use caller-relative descriptors and remain a
                    // separate machine proof; typed lifetime checking is mandatory.
                    if !matches!(
                        operation,
                        Operation::Get
                            | Operation::GetOr
                            | Operation::Contains
                            | Operation::EntryKey
                            | Operation::EntryValue
                            | Operation::Len
                            | Operation::Capacity
                            | Operation::IsEmpty
                    ) || entry.kind != "helper"
                        || !parameters
                            .get(&root)
                            .is_some_and(|param| !param.mutable && param.ty.strip_prefix('&') == Some(contract.owner.as_str()))
                    {
                        return Err(invalid("operation has no checked owned constructor"));
                    }
                    continue;
                }
                let (offset, declared) = roots[&root];
                if contract != declared {
                    return Err(invalid("allocation differs from constructor contract"));
                }
                if searches {
                    let descriptor = offset + 32 + contract.bytes + member.ok_or_else(|| invalid("search has no member"))? * 32;
                    expected
                        .entry((entry.id.as_str(), block.id, offset, Some(descriptor), Primitive::Find))
                        .or_insert((0, declared))
                        .0 += 1;
                    let parent = match operation {
                        Operation::Insert => Some(Primitive::Put),
                        Operation::Remove => Some(Primitive::Erase),
                        _ => None,
                    };
                    if let Some(parent) = parent {
                        expected.entry((entry.id.as_str(), block.id, offset, Some(descriptor), parent)).or_insert((0, declared)).0 +=
                            1;
                    }
                }
                // Each bounded reserve has one existing-buffer path and one
                // fresh-buffer path. Zero-input load_sorted has neither.
                if allocates {
                    expected.entry((entry.id.as_str(), block.id, offset, None, Primitive::Allocate)).or_insert((0, declared)).0 += 2;
                    expected.entry((entry.id.as_str(), block.id, offset, None, Primitive::Resize)).or_insert((0, declared)).0 += 1;
                    let member = member
                        .filter(|index| (*index as usize) < contract.members.len())
                        .ok_or_else(|| invalid("reserve has no declared member"))?;
                    let descriptor = offset + 32 + contract.bytes + member * 32;
                    expected
                        .entry((entry.id.as_str(), block.id, offset, Some(descriptor), Primitive::Reserve))
                        .or_insert((0, declared))
                        .0 += 1;
                } else if *operation == Operation::Drop {
                    expected.entry((entry.id.as_str(), block.id, offset, None, Primitive::Release)).or_insert((0, declared)).0 +=
                        contract.members.len();
                    for member in 0..contract.members.len() as u32 {
                        let descriptor = offset + 32 + contract.bytes + member * 32;
                        expected
                            .entry((entry.id.as_str(), block.id, offset, Some(descriptor), Primitive::Drop))
                            .or_insert((0, declared))
                            .0 += 1;
                    }
                }
                let element_kinds = match operation {
                    Operation::Len => vec![Primitive::Len],
                    Operation::Capacity => vec![Primitive::Capacity],
                    Operation::IsEmpty => vec![Primitive::Empty],
                    Operation::Push => vec![Primitive::Push],
                    Operation::Insert => {
                        let member = member
                            .and_then(|index| contract.members.get(index as usize))
                            .ok_or_else(|| invalid("insert has no declared member"))?;
                        if matches!(member.shape, crate::allocated_values::MemberKind::OrderedMap { .. }) {
                            vec![Primitive::Insert, Primitive::Set]
                        } else {
                            vec![Primitive::Insert]
                        }
                    }
                    Operation::Set => vec![Primitive::Set],
                    Operation::Remove if ordered => vec![Primitive::Remove],
                    Operation::Remove => vec![Primitive::Read, Primitive::Remove, Primitive::Take],
                    Operation::Pop => vec![Primitive::Read, Primitive::Remove, Primitive::Pop],
                    Operation::Get if ordered => vec![Primitive::Read, Primitive::Lookup, Primitive::Required],
                    Operation::GetOr => vec![Primitive::Read, Primitive::Lookup],
                    Operation::Get | Operation::EntryKey | Operation::EntryValue => vec![Primitive::Read],
                    Operation::LoadSorted => vec![Primitive::Sorted(sorted_count.ok_or_else(|| invalid("missing sorted count"))?)],
                    _ => Vec::new(),
                };
                for kind in element_kinds {
                    let member = member
                        .filter(|index| (*index as usize) < contract.members.len())
                        .ok_or_else(|| invalid("element operation has no declared member"))?;
                    let descriptor = offset + 32 + contract.bytes + member * 32;
                    expected.entry((entry.id.as_str(), block.id, offset, Some(descriptor), kind)).or_insert((0, declared)).0 += 1;
                }
            }
        }
    }
    let words = elf.instructions.iter().map(|i| (i.address, i.word)).collect::<BTreeMap<_, _>>();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect::<BTreeMap<_, _>>();
    let machine = Machine { words: &words, targets: &targets, abort };
    let mut boundaries = BTreeMap::new();
    let mut current = BTreeMap::new();
    let mut blocks = record.blocks.iter().collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    for block in blocks {
        if let Some(id) = block.lowering_block_id {
            current.insert(block.owner_entry.as_str(), id);
        }
        for label in block.allocated_allocation_labels.iter().chain(&block.allocated_lifecycle_labels) {
            if label.starts_with(borrowed::PREFIX)
                || label.starts_with(arguments::PREFIX)
                || label.starts_with(crate::scalar_machine::PREFIX)
                || label.starts_with(".Lallocated_call_")
                || label.starts_with(".Lallocated_branch_")
                || label.starts_with(".Lallocated_return_")
                || transport::is_marker(label)
            {
                continue;
            }
            let (primitive, suffix) = if let Some(suffix) = label.strip_prefix(".Lallocated_allocation_") {
                (Primitive::Allocate, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_resize_") {
                (Primitive::Resize, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_release_") {
                (Primitive::Release, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_reserve_") {
                (Primitive::Reserve, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_drop_") {
                (Primitive::Drop, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_push_") {
                (Primitive::Push, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_insert_") {
                (Primitive::Insert, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_set_") {
                (Primitive::Set, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_remove_") {
                (Primitive::Remove, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_find_") {
                (Primitive::Find, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_put_") {
                (Primitive::Put, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_erase_") {
                (Primitive::Erase, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_read_") {
                (Primitive::Read, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_lookup_") {
                (Primitive::Lookup, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_take_") {
                (Primitive::Take, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_pop_") {
                (Primitive::Pop, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_required_") {
                (Primitive::Required, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_len_") {
                (Primitive::Len, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_capacity_") {
                (Primitive::Capacity, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_empty_") {
                (Primitive::Empty, suffix)
            } else if let Some(suffix) = label.strip_prefix(".Lallocated_sorted_") {
                let (count, suffix) = suffix.split_once('_').ok_or_else(|| invalid("sorted marker has no count"))?;
                (Primitive::Sorted(count.parse().map_err(|_| invalid("invalid sorted count"))?), suffix)
            } else {
                return Err(invalid("unknown marker"));
            };
            let parts = suffix.split('_').collect::<Vec<_>>();
            let descriptor_count = usize::from(primitive.tier() >= 2);
            if parts.len() != 3 + descriptor_count || !matches!(parts[2 + descriptor_count], "begin" | "end") {
                return Err(invalid("invalid allocation marker"));
            }
            let offset = parts[0].parse::<u32>().map_err(|_| invalid("invalid owner offset"))?;
            let descriptor = if descriptor_count == 1 {
                Some(parts[1].parse::<u32>().map_err(|_| invalid("invalid descriptor offset"))?)
            } else {
                None
            };
            let id = parts[1 + descriptor_count].parse::<u64>().map_err(|_| invalid("invalid allocation identity"))?;
            let owner = block.owner_entry.as_str();
            let typed = *current.get(owner).ok_or_else(|| invalid("allocation outside typed code"))?;
            if boundaries
                .insert((owner, offset, descriptor, primitive, id, parts[2 + descriptor_count]), (block.range.start, typed))
                .is_some()
            {
                return Err(invalid("duplicate allocation boundary"));
            }
        }
    }
    let mut checked = BTreeSet::new();
    let mut sites = Vec::new();
    for (&(owner, offset, descriptor, primitive, id, kind), &(start, typed)) in &boundaries {
        if kind != "begin" {
            continue;
        }
        let end_key = (owner, offset, descriptor, primitive, id, "end");
        let &(end, _) = boundaries.get(&end_key).ok_or_else(|| invalid("missing allocation end"))?;
        let (remaining, contract) = expected
            .get_mut(&(owner, typed, offset, descriptor, primitive))
            .filter(|(count, _)| *count != 0)
            .ok_or_else(|| invalid("extra or unbound allocation site"))?;
        *remaining -= 1;
        checked.insert((owner, offset, descriptor, primitive, id, kind));
        checked.insert(end_key);
        sites.push(Site { start, end, owner, block: typed, offset, descriptor, kind: primitive, contract, borrowed: None });
    }
    if checked.len() != boundaries.len() || expected.values().any(|(count, _)| *count != 0) {
        return Err(invalid("missing or orphan allocation site"));
    }
    let mut allocations = BTreeMap::new();
    for site in sites.iter().filter(|site| site.kind == Primitive::Allocate) {
        machine.allocation(site.start, site.end, site.offset, site.contract).map_err(|error| invalid(error.message))?;
        if allocations.insert(site.start, site).is_some() {
            return Err(invalid("duplicate allocation site"));
        }
    }
    let mut consumed = BTreeSet::new();
    for site in sites.iter().filter(|site| site.kind.tier() == 1) {
        machine.lifecycle(site, &allocations, &mut consumed).map_err(|error| invalid(error.message))?;
    }
    if consumed.len() != allocations.len() {
        return Err(invalid("allocation detached from its resize"));
    }
    let primitives = sites.iter().filter(|site| site.kind.tier() == 1).map(|site| (site.start, site)).collect::<BTreeMap<_, _>>();
    let mut consumed_primitives = BTreeSet::new();
    let types = record.typed_semantics.types.iter().map(|ty| (ty.name.as_str(), ty)).collect();
    for site in sites.iter().filter(|site| site.kind.tier() == 2) {
        machine.descriptor(site, &primitives, &mut consumed_primitives, &types).map_err(|error| invalid(error.message))?;
    }
    if consumed_primitives.len() != primitives.len() {
        return Err(invalid("arena lifecycle detached from its descriptor update"));
    }
    let reserves = sites.iter().filter(|site| site.kind == Primitive::Reserve).map(|site| (site.start, site)).collect();
    let mut consumed_reserves = BTreeSet::new();
    for site in sites.iter().filter(|site| site.kind.tier() == 3) {
        if site.kind == Primitive::Find {
            machine.find(site, &types).map_err(|error| invalid(error.message))?;
        } else if matches!(site.kind, Primitive::Sorted(_)) {
            machine.sorted(site, &reserves, &mut consumed_reserves, &types).map_err(|error| invalid(error.message))?;
        } else if matches!(site.kind, Primitive::Len | Primitive::Capacity | Primitive::Empty) {
            machine.query(site, &types)?;
        } else {
            machine.elements(site, &reserves, &mut consumed_reserves, &types).map_err(|error| invalid(error.message))?;
        }
    }
    let members = sites.iter().filter(|site| site.kind.tier() == 3).map(|site| (site.start, site)).collect();
    let mut consumed_members = BTreeSet::new();
    for site in sites.iter().filter(|site| site.kind.tier() == 4) {
        if matches!(site.kind, Primitive::Put | Primitive::Erase) {
            machine.ordered_update(site, &members, &mut consumed_members).map_err(|error| invalid(error.message))?;
        } else {
            machine.read_operation(site, &members, &mut consumed_members, &types).map_err(|error| invalid(error.message))?;
        }
    }
    let reads = sites.iter().filter(|site| site.kind.tier() == 4).map(|site| (site.start, site)).collect();
    let mut consumed_reads = BTreeSet::new();
    for site in sites.iter().filter(|site| site.kind.tier() == 5) {
        machine.read_operation(site, &reads, &mut consumed_reads, &types).map_err(|error| invalid(error.message))?;
    }
    // Ordered updates contain find and element writes; push/insert contain
    // reserve; reserve/drop contain resize/release; resize contains allocations.
    // Explicit reserve, search and sorted batches may be roots.
    // Each tier is disjoint. No incoming edge may enter a partial primitive,
    // including any child start from outside its enclosing parent.
    for tier in 0..=5 {
        let mut ranges = sites.iter().filter(|site| site.kind.tier() == tier).map(|site| (site.start, site.end)).collect::<Vec<_>>();
        ranges.sort_unstable();
        if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err(invalid("overlapping arena primitive sites"));
        }
        for edge in &elf.control_flow {
            let index = ranges.partition_point(|(start, _)| *start < edge.target);
            if let Some(&(start, end)) = index.checked_sub(1).and_then(|index| ranges.get(index))
                && edge.target < end
                && !(start..end).contains(&edge.address)
            {
                return Err(invalid("control flow enters a partial arena primitive"));
            }
        }
    }
    let borrowed = borrowed::validate(record, elf, &machine, checked_constructors)?;
    sites.extend(borrowed.sites);
    let copied_results = transport::validate(record, elf, &machine, checked_constructors, &sites)?;
    arguments::validate(record, elf, &machine, checked_constructors, &sites)?;
    queries::validate(record, elf, &machine, checked_constructors, &sites)?;
    // Export only complete independently recognized primitive ranges. Nested
    // children authorize the same arena as their parent, never a second owner.
    let mut result = Checked {
        borrowed_reads: borrowed.reads,
        copied_results,
        flow_sites: paths::sites(record, checked_constructors, &sites)?,
        ..Checked::default()
    };
    let mut owned = sites.iter().filter(|site| site.borrowed.is_none()).collect::<Vec<_>>();
    owned.sort_by_key(|site| (site.owner, site.start, std::cmp::Reverse(site.end)));
    for site in owned {
        let constructor = checked_constructors
            .get(site.owner)
            .into_iter()
            .flat_map(|sites| sites.values())
            .find(|constructor| constructor.offset == i64::from(site.offset))
            .ok_or_else(|| invalid("owned primitive has no checked arena region"))?;
        let ranges = result.owned_primitives.entry(site.owner.to_string()).or_default();
        if let Some((_, previous)) = ranges.last_key_value()
            && site.start < previous.end
        {
            if site.end > previous.end || previous.offset != constructor.offset || previous.extent != constructor.extent {
                return Err(invalid("overlapping owned primitives disagree on their arena region"));
            }
            continue;
        }
        ranges.insert(site.start, OwnedPrimitive { end: site.end, offset: constructor.offset, extent: constructor.extent });
    }
    Ok(result)
}
