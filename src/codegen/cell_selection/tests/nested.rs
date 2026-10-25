//! Variable Cell payload execution prerequisite. The fixed-role oracle
//! independently checks selected identities; source transactions live next door.
use super::*;
use crate::codegen::nested::{NestedCellField, NestedCellLayout};
use cellscript_artifact_checker::bounded_nested::{ElementShape, NestedValueContract, PROFILE};

fn table(fields: &[Vec<u8>]) -> Vec<u8> {
    if fields.is_empty() {
        return 4u32.to_le_bytes().to_vec();
    }
    let header = 4 * (fields.len() + 1);
    let total = header + fields.iter().map(Vec::len).sum::<usize>();
    let mut result = (total as u32).to_le_bytes().to_vec();
    let mut offset = header;
    for field in fields {
        result.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += field.len();
    }
    for field in fields {
        result.extend_from_slice(field);
    }
    result
}

fn bytes(payload: &[u8]) -> Vec<u8> {
    let mut result = (payload.len() as u32).to_le_bytes().to_vec();
    result.extend_from_slice(payload);
    result
}

fn nested_contract() -> ExactSelection {
    let mut result = derived(&contract());
    configure_role(&mut result.roles[0]);
    result
}

pub(super) fn configure_role(role: &mut ExactRole) {
    let values = |element| {
        NestedCellField::Values(NestedValueContract {
            profile: PROFILE.into(),
            element,
            max_items: 32,
            max_payload_bytes: 256,
            max_total_bytes: 4096,
        })
    };
    role.width = 8208 + role.key_width;
    role.key_offset = 0;
    role.nested = Some(NestedRole {
        layout: NestedCellLayout {
            capacity: role.width,
            fields: vec![
                values(ElementShape::Bytes {}),
                NestedCellField::Fixed(role.key_width),
                values(ElementShape::Record { fixed_field_widths: vec![1] }),
            ],
        },
        key_field: 1,
    });
}

fn encode_cell(cell: &mut Cell, first: &[u8], last: &[u8]) {
    encode_role_cell(cell, 0, 8, first, last);
}

pub(super) fn encode_role_cell(cell: &mut Cell, key_offset: usize, key_width: usize, first: &[u8], last: &[u8]) {
    let key = cell.data[key_offset..key_offset + key_width].to_vec();
    cell.data = table(&[table(&[bytes(first)]), key, table(&[table(&[vec![7], bytes(last)])])]);
}

fn run(contract: &ExactSelection, inputs: &[Cell], outputs: &[Cell], expected: Option<&reference::Selection>) -> (i8, u64, usize) {
    execute_vm(
        program(contract, expected),
        Transaction { current: current(), inputs: inputs.to_vec(), outputs: outputs.to_vec(), fault: None },
    )
}

#[test]
fn nested_role_machine_preserves_identity_after_variable_fields_and_charges_actual_bytes() {
    let model = contract();
    let original = vec![fee(), cell(&model, 0, 1), cell(&model, 1, 2), cell(&model, 0, 3)];
    let expected = reference::select(&model, current(), &original, &original.iter().cloned().rev().collect::<Vec<_>>()).unwrap();
    let mut inputs = original.clone();
    encode_cell(&mut inputs[1], &[1], &[2; 256]);
    encode_cell(&mut inputs[3], &[3; 256], &[]);
    let mut outputs = original.into_iter().rev().collect::<Vec<_>>();
    // Payload lengths and bytes change while identity/correspondence remains.
    encode_cell(&mut outputs[0], &[9; 7], &[8; 31]);
    encode_cell(&mut outputs[2], &[], &[4; 9]);
    let mut contract = nested_contract();
    contract.aggregate_bytes = inputs.iter().chain(&outputs).filter(|cell| cell.type_hash.is_some()).map(|cell| cell.data.len()).sum();
    assert_eq!(run(&contract, &inputs, &outputs, Some(&expected)).0, 0);
    contract.aggregate_bytes -= 1;
    assert_eq!(run(&contract, &inputs, &outputs, None).0, status(reference::Error::ByteBound));
}

#[test]
fn nested_role_machine_rejects_malformed_unused_payloads_and_preserves_other_obligations() {
    let model = contract();
    let mut cells = vec![cell(&model, 0, 1), cell(&model, 1, 2)];
    encode_cell(&mut cells[0], &[1, 2, 3], &[4, 5]);
    let contract = nested_contract();
    assert_eq!(run(&contract, &cells, &cells, None).0, 0);
    for change in 0..8 {
        let mut outputs = cells.clone();
        let data = &mut outputs[0].data;
        let expected = match change {
            0 => {
                data[0] ^= 1;
                2
            }
            1 => {
                data[4] = 17;
                16
            }
            2 => {
                data[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
                2
            }
            3 => {
                let at = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
                data[at + 4] += 1;
                16
            }
            4 => {
                data.resize(8217, 0);
                16
            }
            5 => {
                let at = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
                data[at] ^= 1;
                status(reference::Error::Correspondence)
            }
            6 => {
                outputs[0].lock_hash[31] ^= 1;
                status(reference::Error::LockIdentity)
            }
            _ => {
                outputs.push(outputs[0].clone());
                status(reference::Error::DuplicateKey)
            }
        };
        assert_eq!(run(&contract, &cells, &outputs, None).0, expected, "mutation {change}");
    }
}

#[test]
fn nested_role_machine_rejects_incoherent_internal_bounds() {
    for mutation in 0..6 {
        let mut contract = nested_contract();
        let role = &mut contract.roles[0];
        let nested = role.nested.as_mut().unwrap();
        match mutation {
            0 => nested.layout.capacity = 512,
            1 => nested.key_field = 0,
            2 => nested.key_field = 3,
            3 => nested.layout.capacity = 32769,
            4 => role.key_offset = 1,
            _ => {
                nested.layout.capacity += 1;
                role.width += 1;
            }
        }
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        assert!(generator.emit_exact_role_selection(&contract).is_err());
        assert!(generator.assembly.is_empty());
    }
}

#[test]
fn nested_role_machine_maximum_catalog_keeps_tables_disjoint_from_large_payload_scratch() {
    let mut model = contract();
    model.roles = (0..8)
        .map(|index| Role {
            type_hash: [index + 1; 32],
            lock_hash: [index + 11; 32],
            schema: [index + 21; 32],
            width: 512,
            key_offset: 480,
            key_width: 32,
            inputs: Count { min: 4, max: 4 },
            outputs: Count { min: 4, max: 4 },
            access: if index == 0 { Access::LocalType } else { Access::Observe },
            correspondence: reference::Correspondence::Preserve,
        })
        .collect();
    let normalized: Vec<_> = (0..32).flat_map(|index| [fee(), cell(&model, index % 8, index as u8)]).collect();
    let expected = reference::select(&model, current(), &normalized, &normalized.iter().cloned().rev().collect::<Vec<_>>()).unwrap();
    let mut inputs = normalized;
    for cell in inputs.iter_mut().filter(|cell| cell.type_hash.is_some()) {
        encode_role_cell(cell, 480, 32, &[1; 210], &[2; 210]);
    }
    let mut outputs = inputs.iter().cloned().rev().collect::<Vec<_>>();
    let mut contract = derived(&model);
    for role in &mut contract.roles {
        configure_role(role);
    }
    contract.aggregate_bytes = 32_320;
    let valid = run(&contract, &inputs, &outputs, Some(&expected));
    assert_eq!(valid.0, 0);
    let last = outputs.iter_mut().rev().find(|cell| cell.type_hash.is_some()).unwrap();
    // The last selected Cell has a malformed final nested payload length.
    let offset = last.data.len() - 211;
    last.data[offset] ^= 1;
    let invalid = run(&contract, &inputs, &outputs, None);
    assert_eq!(invalid.0, 16);
    eprintln!(
        "[nested-role primitive] roles=8 selected=64 actual_data=32320 frame={} valid_cycles={} late_invalid_cycles={} test_ELF={}",
        contract.frame_bytes(),
        valid.1,
        invalid.1,
        valid.2
    );
}
