//! Experimental allocation declarations and typed operation shapes. This does
//! not certify emitted allocation checks or machine owner lifetimes; the proof blocker
//! is mandatory even after outer hashes are recomputed.
use crate::{
    CheckerError, CheckerRejectionCode, TypedSemanticOperation, TypedSemanticOperationDetail, TypedSemanticRecord, TypedSemanticType,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

mod lifetime;
pub(crate) mod machine;
pub(crate) use lifetime::{owner_types, validate as validate_lifetimes};

pub const PROFILE: &str = "cellscript-allocated-values-v1";
pub const PROOF_BLOCKER: &str = "allocated-values-artifact-proof";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Operation {
    New,
    Drop,
    Len,
    Capacity,
    IsEmpty,
    Reserve,
    LoadSorted,
    Push,
    Insert,
    Set,
    Get,
    GetOr,
    Remove,
    Pop,
    Contains,
    EntryKey,
    EntryValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum MemberKind {
    Vec { item: String },
    OrderedMap { key: String, value: String },
    OrderedSet { key: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    pub name: String,
    pub items: u32,
    pub shape: MemberKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub profile: String,
    pub owner: String,
    pub bytes: u32,
    pub allocations: u32,
    pub members: Vec<Member>,
}

fn width(ty: &str) -> Option<u32> {
    match ty {
        "bool" | "u8" => Some(1),
        "u16" => Some(2),
        "u32" | "i32" => Some(4),
        "u64" => Some(8),
        "u128" => Some(16),
        "hash" | "address" => Some(32),
        _ => ty.strip_prefix("[u8; ")?.strip_suffix(']')?.parse::<u32>().ok().filter(|n| (1..=64).contains(n)),
    }
}

fn key_width(ty: &str) -> Option<u32> {
    if matches!(ty, "u8" | "u16" | "u32" | "u64" | "hash" | "address") || ty.starts_with("[u8; ") {
        width(ty).filter(|n| *n <= 32)
    } else {
        None
    }
}

fn value_width(ty: &str, types: &BTreeMap<&str, &TypedSemanticType>, visiting: &mut BTreeSet<String>) -> Option<u32> {
    if let Some(width) = width(ty) {
        return Some(width);
    }
    if visiting.len() >= 32 || !visiting.insert(ty.to_string()) {
        return None;
    }
    let result = (|| {
        let layout = types.get(ty).filter(|layout| layout.kind == "struct")?;
        let total = layout.fields.iter().try_fold(0u32, |offset, field| {
            let width = value_width(&field.ty, types, visiting)?;
            (field.offset == offset && field.width_bytes == Some(width)).then_some(())?;
            offset.checked_add(width)
        })?;
        (layout.encoded_size == Some(total) && (1..=64).contains(&total)).then_some(total)
    })();
    visiting.remove(ty);
    result
}

fn name(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|ch| ch.is_alphabetic() || ch == '_') && chars.all(|ch| ch.is_alphanumeric() || ch == '_')
}

impl Contract {
    fn valid(&self, types: &BTreeMap<&str, &TypedSemanticType>) -> bool {
        if self.profile != PROFILE
            || !name(&self.owner)
            || !(64..=65536).contains(&self.bytes)
            || !self.bytes.is_multiple_of(16)
            || !(1..=256).contains(&self.allocations)
            || !(1..=32).contains(&self.members.len())
        {
            return false;
        }
        let mut seen = BTreeSet::new();
        self.members.iter().all(|member| {
            let width = match &member.shape {
                MemberKind::Vec { item } => value_width(item, types, &mut BTreeSet::new()),
                MemberKind::OrderedSet { key } => key_width(key),
                MemberKind::OrderedMap { key, value } => {
                    key_width(key).zip(value_width(value, types, &mut BTreeSet::new()).filter(|n| *n <= 32)).map(|(a, b)| a + b)
                }
            };
            name(&member.name)
                && !matches!(member.name.as_str(), "limits" | "new" | "drop")
                && seen.insert(&member.name)
                && (1..=1024).contains(&member.items)
                && width.is_some_and(|n| (n * member.items).div_ceil(16) * 16 + 32 <= self.bytes)
        })
    }
}

pub(crate) fn valid_operation(
    op: &TypedSemanticOperation,
    destination: Option<&str>,
    types: &BTreeMap<&str, &TypedSemanticType>,
) -> bool {
    let TypedSemanticOperationDetail::AllocatedValue { contract, member, operation } = &op.detail else {
        return false;
    };
    if !contract.valid(types) || op.call.is_some() {
        return false;
    }
    if *operation == Operation::New {
        return member.is_none()
            && op.operands.is_empty()
            && op.destinations.len() == 1
            && destination == Some(contract.owner.as_str());
    }
    let Some(owner) = op.operands.first() else {
        return false;
    };
    if owner.ty != contract.owner
        && (owner.ty != format!("&{}", contract.owner)
            || !matches!(
                operation,
                Operation::Len
                    | Operation::Capacity
                    | Operation::IsEmpty
                    | Operation::Get
                    | Operation::GetOr
                    | Operation::Contains
                    | Operation::EntryKey
                    | Operation::EntryValue
            ))
    {
        return false;
    }
    if *operation == Operation::Drop {
        return member.is_none() && op.operands.len() == 1 && op.destinations.is_empty();
    }
    let Some(member) = member.and_then(|index| contract.members.get(index as usize)) else {
        return false;
    };
    use Operation as O;
    if *operation == O::LoadSorted {
        let element = match &member.shape {
            MemberKind::OrderedMap { key, value } if width(value).is_some() => format!("({key}, {value})"),
            MemberKind::OrderedSet { key } => key.clone(),
            _ => return false,
        };
        let Some(input) = op.operands.get(1) else { return false };
        let Some((ty, count)) = input.ty.strip_prefix('[').and_then(|ty| ty.strip_suffix(']')).and_then(|ty| ty.rsplit_once("; "))
        else {
            return false;
        };
        return op.operands.len() == 2
            && op.destinations.is_empty()
            && destination.is_none()
            && ty == element
            && count.parse::<usize>().ok().is_some_and(|count| count <= 1024);
    }
    let (args, result): (Vec<&str>, Option<&str>) = match (operation, &member.shape) {
        (O::Len | O::Capacity, _) => (vec![], Some("u64")),
        (O::IsEmpty, _) => (vec![], Some("bool")),
        (O::Reserve, _) => (vec!["u64"], None),
        (O::Push, MemberKind::Vec { item }) => (vec![item], None),
        (O::Insert | O::Set, MemberKind::Vec { item }) => (vec!["u64", item], None),
        (O::Get | O::Remove | O::EntryKey, MemberKind::Vec { item }) => (vec!["u64"], Some(item)),
        (O::Pop, MemberKind::Vec { item }) => (vec![], Some(item)),
        (O::Insert, MemberKind::OrderedMap { key, value }) => (vec![key, value], Some("bool")),
        (O::Get, MemberKind::OrderedMap { key, value }) => (vec![key], Some(value)),
        (O::GetOr, MemberKind::OrderedMap { key, value }) => (vec![key, value], Some(value)),
        (O::Remove | O::Contains, MemberKind::OrderedMap { key, .. })
        | (O::Insert | O::Remove | O::Contains, MemberKind::OrderedSet { key }) => (vec![key], Some("bool")),
        (O::EntryKey, MemberKind::OrderedMap { key, .. } | MemberKind::OrderedSet { key }) => (vec!["u64"], Some(key)),
        (O::EntryValue, MemberKind::OrderedMap { value, .. }) => (vec!["u64"], Some(value)),
        _ => return false,
    };
    op.operands.len() == args.len() + 1
        && op.destinations.len() == usize::from(result.is_some())
        && destination == result
        && op.operands[1..].iter().zip(args).all(|(operand, expected)| operand.ty == expected)
}

pub(crate) fn validate_metadata(metadata: &serde_json::Value, typed: &TypedSemanticRecord) -> Result<(), CheckerError> {
    let mut present = false;
    // A helper borrows the same declared owner layout as its caller. A valid
    // budget in isolation must not allow a callee to reinterpret that arena.
    let mut contracts = BTreeMap::new();
    for entry in &typed.entries {
        for op in entry.blocks.iter().flat_map(|block| &block.operations) {
            if let TypedSemanticOperationDetail::AllocatedValue { contract, .. } = &op.detail {
                present = true;
                if contracts.insert(&contract.owner, contract).is_some_and(|old| old != contract) {
                    return Err(CheckerError::new(
                        CheckerRejectionCode::V2419TypedSemanticsInvalid,
                        "allocated owner declaration differs between operations or helper entries",
                    ));
                }
            }
        }
    }
    if present
        && !metadata
            .get("runtime")
            .and_then(|r| r.get("fail_closed_runtime_features"))
            .and_then(serde_json::Value::as_array)
            .is_some_and(|features| features.iter().any(|f| f.as_str() == Some(PROOF_BLOCKER)))
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2410MetadataBindingMismatch,
            "allocated values require independent allocation/ownership proof admission blocker",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TypedSemanticField;

    fn record(name: &str, field_ty: &str, size: u32) -> TypedSemanticType {
        TypedSemanticType {
            name: name.into(),
            kind: "struct".into(),
            encoded_size: Some(size),
            fields: vec![TypedSemanticField {
                name: "value".into(),
                ty: field_ty.into(),
                width_bytes: Some(size),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn allocated_record_width_is_derived_recursively_not_trusted() {
        let inner = record("Inner", "u64", 8);
        let outer = record("Outer", "Inner", 8);
        let derive = |inner: &TypedSemanticType, outer: &TypedSemanticType| {
            value_width("Outer", &BTreeMap::from([("Inner", inner), ("Outer", outer)]), &mut BTreeSet::new())
        };
        assert_eq!(derive(&inner, &outer), Some(8));
        assert_eq!(value_width("Outer", &BTreeMap::from([("Outer", &outer)]), &mut BTreeSet::new()), None);
        for mutation in 0..8 {
            let mut changed = inner.clone();
            match mutation {
                0 => changed.kind = "resource".into(),
                1 => changed.encoded_size = Some(7),
                2 => changed.fields[0].offset = 1,
                3 => changed.fields[0].width_bytes = Some(7),
                4 => changed.fields[0].ty = "Outer".into(),
                5 => changed.fields[0].ty = "Vec<u64>".into(),
                6 => changed.fields[0].ty = "&u64".into(),
                _ => changed.fields.clear(),
            }
            assert_eq!(derive(&changed, &outer), None, "mutation {mutation}");
        }
        for size in [0, 1, 32, 64, 65] {
            let wide = record("Wide", &format!("[u8; {size}]"), size);
            let types = BTreeMap::from([("Wide", &wide)]);
            let contract = |shape| Contract {
                profile: PROFILE.into(),
                owner: "Work".into(),
                bytes: 256,
                allocations: 2,
                members: vec![Member { name: "values".into(), items: 1, shape }],
            };
            assert_eq!(contract(MemberKind::Vec { item: "Wide".into() }).valid(&types), (1..=64).contains(&size));
            assert_eq!(
                contract(MemberKind::OrderedMap { key: "u64".into(), value: "Wide".into() }).valid(&types),
                (1..=32).contains(&size)
            );
            assert!(!contract(MemberKind::OrderedSet { key: "Wide".into() }).valid(&types));
        }
    }
}
