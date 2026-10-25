//! Shared declaration scenarios for CLI and editor budget explanations.
//! These costs are not an arbitrary program-path or worst-case proof.
use crate::ast::{CollectionsDef, Type, ValueCollectionKind};
use crate::error::{CompileError, Result};
use serde::Serialize;

#[derive(Default, Serialize)]
struct Cost {
    payload_bytes: u64,
    alignment_bytes: u64,
    record_bytes: u64,
    bytes: u64,
    allocations: u64,
}

impl Cost {
    fn add(&mut self, other: &Self) {
        self.payload_bytes += other.payload_bytes;
        self.alignment_bytes += other.alignment_bytes;
        self.record_bytes += other.record_bytes;
        self.bytes += other.bytes;
        self.allocations += other.allocations;
    }

    fn allocation(payload: u64) -> Self {
        let rounded = payload.div_ceil(16) * 16;
        Self { payload_bytes: payload, alignment_bytes: rounded - payload, record_bytes: 32, bytes: rounded + 32, allocations: 1 }
    }
}

#[derive(Serialize)]
struct Fit {
    cost: Cost,
    bytes_shortfall: u64,
    allocations_shortfall: u64,
    fits: bool,
}

impl Fit {
    fn new(cost: Cost, def: &CollectionsDef) -> Self {
        let bytes_shortfall = cost.bytes.saturating_sub(u64::from(def.bytes));
        let allocations_shortfall = cost.allocations.saturating_sub(u64::from(def.allocations));
        Self { cost, bytes_shortfall, allocations_shortfall, fits: bytes_shortfall == 0 && allocations_shortfall == 0 }
    }
}

#[derive(Serialize)]
struct Member {
    name: String,
    element_bytes: u64,
    items: u32,
    final_payload_bytes: u64,
    reserve_once: Cost,
    incremental: Cost,
    growth_capacities: Vec<u32>,
    retired_bytes: u64,
}

#[derive(Serialize)]
pub(crate) struct Owner {
    pub(crate) module: String,
    pub(crate) name: String,
    byte_budget: u32,
    allocation_budget: u32,
    members: Vec<Member>,
    reserve_all_once: Fit,
    grow_all_incrementally: Fit,
    /// Fixed owner region, not a whole-function stack or peak-memory estimate.
    frame_region_bytes: u64,
    state_bytes: u64,
    descriptor_bytes: u64,
    scratch_bytes: u64,
}

pub(crate) fn analyze(module: &str, def: &CollectionsDef, width: impl Fn(&Type) -> Option<u32>) -> Result<Owner> {
    crate::allocated_values::validate_resolved_declaration(def, &width)?;
    let fixed = |ty| width(ty).map(u64::from).ok_or_else(|| CompileError::without_span("collection budget has no checked layout"));
    let mut members = Vec::new();
    let mut reserve_total = Cost::default();
    let mut growth_total = Cost::default();
    for member in &def.members {
        let element_bytes = match &member.kind {
            ValueCollectionKind::Vec(item) | ValueCollectionKind::OrderedSet(item) => fixed(item)?,
            ValueCollectionKind::OrderedMap { key, value } => fixed(key)? + fixed(value)?,
        };
        let final_payload_bytes = element_bytes * u64::from(member.items);
        let reserve_once = Cost::allocation(final_payload_bytes);
        let mut incremental = Cost::default();
        let mut growth_capacities = Vec::new();
        let mut capacity = 0u32;
        while capacity < member.items {
            capacity = capacity.saturating_mul(2).max(1).min(member.items);
            growth_capacities.push(capacity);
            incremental.add(&Cost::allocation(element_bytes * u64::from(capacity)));
        }
        let retired_bytes = incremental.bytes - reserve_once.bytes;
        reserve_total.add(&reserve_once);
        growth_total.add(&incremental);
        members.push(Member {
            name: member.name.clone(),
            element_bytes,
            items: member.items,
            final_payload_bytes,
            reserve_once,
            incremental,
            growth_capacities,
            retired_bytes,
        });
    }
    let descriptor_bytes = def.members.len() as u64 * 32;
    Ok(Owner {
        module: module.into(),
        name: def.name.clone(),
        byte_budget: def.bytes,
        allocation_budget: def.allocations,
        members,
        reserve_all_once: Fit::new(reserve_total, def),
        grow_all_incrementally: Fit::new(growth_total, def),
        frame_region_bytes: 32 + u64::from(def.bytes) + descriptor_bytes + 128,
        state_bytes: 32,
        descriptor_bytes,
        scratch_bytes: 128,
    })
}

impl Owner {
    pub(crate) fn lines(&self) -> Vec<String> {
        let owner = self;
        let mut lines = Vec::new();
        lines.push(format!(
            "{}::{}: shared budget {} bytes / {} allocation events",
            owner.module, owner.name, owner.byte_budget, owner.allocation_budget
        ));
        for member in &owner.members {
            lines.push(format!(
                "  {}: {} items x {} bytes = {} payload + {} alignment + {} record = {} bytes, 1 allocation (reserve once)",
                member.name,
                member.items,
                member.element_bytes,
                member.final_payload_bytes,
                member.reserve_once.alignment_bytes,
                member.reserve_once.record_bytes,
                member.reserve_once.bytes
            ));
            lines.push(format!(
                "    incremental capacities {:?}: {} bytes / {} allocations; {} retired bytes remain charged",
                member.growth_capacities, member.incremental.bytes, member.incremental.allocations, member.retired_bytes
            ));
        }
        for (name, fit) in
            [("Reserve all members once", &owner.reserve_all_once), ("Grow all members incrementally", &owner.grow_all_incrementally)]
        {
            lines.push(format!(
                "  {name}: {} bytes / {} allocations — {}",
                fit.cost.bytes,
                fit.cost.allocations,
                if fit.fits { "fits" } else { "does not fit" }
            ));
            if !fit.fits {
                lines.push(format!("    shortfall: {} bytes / {} allocation events", fit.bytes_shortfall, fit.allocations_shortfall));
            }
        }
        lines.push(format!(
            "  Owner frame region: {} bytes ({} arena + {} state + {} descriptors + {} scratch)",
            owner.frame_region_bytes, owner.byte_budget, owner.state_bytes, owner.descriptor_bytes, owner.scratch_bytes
        ));
        lines
    }
}

pub(crate) const ASSUMPTIONS: [&str; 5] = [
        "Each scenario starts empty and reaches all declared item limits in one owner lifetime.",
        "Reserve-once reserves each member's maximum before insertion; incremental means one new element or distinct key at a time, with no explicit reserve calls.",
        "Records cost 32 bytes per allocation; payloads round up to 16 bytes. Retired buffers and allocation events are not refunded.",
        "These are capacity scenarios, not a program-path or worst-case proof. Actual reserve sequences and owner recreation require separate analysis.",
        "Frame regions exclude preceding alignment, scalar slots, other locals and helper/call frames; they are not total stack or peak VM memory.",
    ];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Item;

    #[test]
    fn budget_scenarios_include_alignment_records_retired_buffers_and_shared_limits() {
        let ast = crate::frontend::parse("module budgets\ncollections Work { limits { bytes: 64, allocations: 1 }\nvalues: Vec<u8> { items: 3 }\nother: OrderedSet<u64> { items: 2 } }", crate::NEXT_EDITION).unwrap();
        let Item::Collections(def) = &ast.items[0] else { unreachable!() };
        let report = analyze("budgets", def, crate::allocated_values::fixed_width).unwrap();
        assert_eq!(report.members[0].growth_capacities, [1, 2, 3]);
        assert_eq!(report.members[0].reserve_once.bytes, 48);
        assert_eq!(report.members[0].reserve_once.alignment_bytes, 13);
        assert_eq!(report.members[0].incremental.bytes, 144);
        assert_eq!(report.members[0].retired_bytes, 96);
        assert_eq!(report.reserve_all_once.cost.bytes, 96);
        assert_eq!(report.reserve_all_once.bytes_shortfall, 32);
        assert_eq!(report.reserve_all_once.allocations_shortfall, 1);
        assert!(!report.reserve_all_once.fits);
        assert_eq!(report.grow_all_incrementally.cost.bytes, 240);
        assert_eq!(report.grow_all_incrementally.cost.allocations, 5);
        assert_eq!(report.frame_region_bytes, 288);
    }
}
